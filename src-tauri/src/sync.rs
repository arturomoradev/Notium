use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use notium_core::extract::EditorKind;
use notium_core::sync::{self, ApplyOutcome, Op};
pub use notium_core::sync::{is_safe_attachment_rel_path, normalize_vault_id, MAX_D1_BLOB_BYTES};
use notium_core::vault;

use crate::error::CommandError;
use crate::state::VaultHandle;

/// Client-side sync configuration stored in `.app/sync_config.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub server_url: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default = "default_vault_id")]
    pub vault_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub auto_sync: bool,
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
    /// Installation and folder the `device_id` belongs to (see `vault_binding`).
    #[serde(default)]
    pub bound_to: String,
}

fn default_vault_id() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    20
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            server_url: String::new(),
            auth_token: String::new(),
            device_id: new_device_id(),
            vault_id: default_vault_id(),
            enabled: false,
            auto_sync: true,
            interval_secs: 20,
            bound_to: String::new(),
        }
    }
}

/// Status reported to the UI and IPC callers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncStatusDto {
    pub enabled: bool,
    pub server_url: String,
    pub device_id: String,
    #[serde(default = "default_vault_id")]
    pub vault_id: String,
    pub state: String, // "synced" | "syncing" | "offline" | "error" | "paused"
    pub last_synced_at: Option<u64>,
    pub pending_ops: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncCycleResult {
    pub ops_pushed: usize,
    pub ops_pulled: usize,
    pub conflicts: usize,
    pub blobs_synced: usize,
    /// Notes edited outside the app that this cycle journaled (their rev moved).
    #[serde(default)]
    pub external_edits: usize,
}

// --- Wire protocol models ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushRequest {
    #[serde(default = "default_vault_id")]
    pub vault_id: String,
    pub device_id: String,
    pub ops: Vec<Op>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PushResponse {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub accepted_op_ids: Vec<String>,
    #[serde(default)]
    pub rejected_op_ids: Vec<String>,
    #[serde(default)]
    pub server_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BlobRef {
    pub hash: String,
    pub path: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub ts: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PullResponse {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub ops: Vec<Op>,
    #[serde(default)]
    pub blobs: Vec<BlobRef>,
}

/// Whether `url` may be used as a sync server. The bearer token and note
/// contents travel in every request, so plain `http://` is accepted only for
/// loopback hosts (local `wrangler dev`, tests) and in debug builds (testing a
/// phone against a dev machine on the LAN).
pub fn is_allowed_server_url(url: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    if url.starts_with("https://") {
        return url.len() > "https://".len();
    }
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
        || (cfg!(debug_assertions) && !host.is_empty())
}

/// HTTP network client for communicating with the sync server.
pub struct SyncClient {
    pub server_url: String,
    pub auth_token: String,
    pub device_id: String,
    pub vault_id: String,
    agent: ureq::Agent,
}

impl SyncClient {
    pub fn new(server_url: &str, auth_token: &str, device_id: &str, vault_id: &str) -> Self {
        let clean_url = server_url
            .trim()
            .replace(' ', "")
            .trim_end_matches('/')
            .to_string();
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .build();
        SyncClient {
            server_url: clean_url,
            auth_token: auth_token.trim().to_string(),
            device_id: device_id.trim().to_string(),
            vault_id: normalize_vault_id(vault_id),
            agent,
        }
    }

    fn apply_auth(&self, req: ureq::Request) -> ureq::Request {
        if !self.auth_token.is_empty() {
            req.set("Authorization", &format!("Bearer {}", self.auth_token))
        } else {
            req
        }
    }

    pub fn push(&self, ops: &[Op]) -> Result<PushResponse, String> {
        let url = format!("{}/sync/push", self.server_url);
        let req = self.agent.post(&url);
        let req = self.apply_auth(req);
        let payload = PushRequest {
            vault_id: self.vault_id.clone(),
            device_id: self.device_id.clone(),
            ops: ops.to_vec(),
        };

        let res = req
            .send_json(payload)
            .map_err(|e| format_ureq_error("POST /sync/push", e))?;

        res.into_json::<PushResponse>()
            .map_err(|e| format!("Error decodificando respuesta del servidor en push: {e}"))
    }

    pub fn pull(&self, since: Option<&str>) -> Result<PullResponse, String> {
        let mut url = format!(
            "{}/sync/pull?device_id={}&vault_id={}",
            self.server_url,
            urlencoding::encode(&self.device_id),
            urlencoding::encode(&self.vault_id)
        );
        if let Some(cursor) = since {
            if !cursor.is_empty() {
                url.push_str(&format!("&since={}", urlencoding::encode(cursor)));
            }
        }
        let req = self.agent.get(&url);
        let req = self.apply_auth(req);

        let res = req
            .call()
            .map_err(|e| format_ureq_error("GET /sync/pull", e))?;

        res.into_json::<PullResponse>()
            .map_err(|e| format!("Error decodificando respuesta del servidor en pull: {e}"))
    }

    pub fn push_blob(&self, hash: &str, path: &str, data: &[u8]) -> Result<(), String> {
        let url = format!(
            "{}/sync/blob/{}?vault_id={}&path={}",
            self.server_url,
            urlencoding::encode(hash),
            urlencoding::encode(&self.vault_id),
            urlencoding::encode(path)
        );
        let req = self.agent.post(&url);
        let req = self
            .apply_auth(req)
            .set("Content-Type", "application/octet-stream");

        req.send_bytes(data)
            .map_err(|e| format_ureq_error("POST /sync/blob", e))?;
        Ok(())
    }

    /// Download a blob; `None` when the server no longer has it (404).
    pub fn pull_blob(&self, hash: &str) -> Result<Option<Vec<u8>>, String> {
        let url = format!(
            "{}/sync/blob/{}?vault_id={}",
            self.server_url,
            urlencoding::encode(hash),
            urlencoding::encode(&self.vault_id)
        );
        let req = self.agent.get(&url);
        let req = self.apply_auth(req);

        let res = match req.call() {
            Ok(res) => res,
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(e) => return Err(format_ureq_error("GET /sync/blob", e)),
        };

        let mut bytes = Vec::new();
        use std::io::Read;
        res.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Error leyendo bytes de adjunto: {e}"))?;
        Ok(Some(bytes))
    }
}

fn format_ureq_error(endpoint: &str, err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!("Servidor {endpoint} respondió con error HTTP {code}: {body}")
        }
        ureq::Error::Transport(t) => {
            format!("Error de conexión al servidor {endpoint}: {t}")
        }
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                    out.push(b as char)
                }
                _ => {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
        out
    }
}

// --- Auth token storage ---
//
// On desktop the bearer token lives in the OS credential store (Windows
// Credential Manager, macOS Keychain, Secret Service) instead of the vault
// folder, which users may keep in a cloud-synced or shared location. The
// config file keeps the token only when no credential store is available
// (mobile, where the vault is already inside the app-private sandbox, or a
// Linux session without a Secret Service).

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod os_token_store {
    const SERVICE: &str = "dev.notium.html.sync";

    /// Store (or, for an empty token, delete) the credential. Returns whether
    /// the credential store now reflects `token`.
    pub fn set(account: &str, token: &str) -> bool {
        let Ok(entry) = keyring::Entry::new(SERVICE, account) else {
            return false;
        };
        if token.is_empty() {
            matches!(
                entry.delete_credential(),
                Ok(()) | Err(keyring::Error::NoEntry)
            )
        } else {
            entry.set_password(token).is_ok()
        }
    }

    pub fn get(account: &str) -> Option<String> {
        keyring::Entry::new(SERVICE, account)
            .ok()?
            .get_password()
            .ok()
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod os_token_store {
    pub fn set(_account: &str, _token: &str) -> bool {
        false
    }
    pub fn get(_account: &str) -> Option<String> {
        None
    }
}

/// Tokens already resolved this session, keyed by device id. The config is
/// loaded on every status update, so this avoids a credential-store round trip
/// each time.
static TOKEN_CACHE: std::sync::LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Unit tests must not touch the developer's real credential store.
fn os_store_enabled(account: &str) -> bool {
    !cfg!(test) && !account.is_empty()
}

fn store_token(account: &str, token: &str) -> bool {
    if !os_store_enabled(account) || !os_token_store::set(account, token) {
        return false;
    }
    if let Ok(mut cache) = TOKEN_CACHE.lock() {
        cache.insert(account.to_string(), token.to_string());
    }
    true
}

fn fetch_token(account: &str) -> String {
    if !os_store_enabled(account) {
        return String::new();
    }
    if let Some(hit) = TOKEN_CACHE
        .lock()
        .ok()
        .and_then(|c| c.get(account).cloned())
    {
        return hit;
    }
    let token = os_token_store::get(account).unwrap_or_default();
    if let Ok(mut cache) = TOKEN_CACHE.lock() {
        cache.insert(account.to_string(), token.clone());
    }
    token
}

fn write_config_file(
    vault_root: &Path,
    config: &SyncConfig,
    keep_token: bool,
) -> Result<(), CommandError> {
    let app_dir = vault_root.join(".app");
    fs::create_dir_all(&app_dir)?;
    let mut on_disk = config.clone();
    if !keep_token {
        on_disk.auth_token.clear();
    }
    let json = serde_json::to_vec_pretty(&on_disk)?;
    vault::atomic_write(&app_dir.join(CONFIG_FILE), &json)?;
    Ok(())
}

/// A new device id. The random tail of a UUIDv7 (its head is only the creation
/// time, which two devices set up together could share).
fn new_device_id() -> String {
    format!("dev_{}", &uuid::Uuid::now_v7().simple().to_string()[20..])
}

/// Id of this app installation (a file in the app data folder), set at startup.
static INSTALL_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Load (or create) this installation's id from `app_data`.
pub fn init_install_id(app_data: &Path) {
    let file = app_data.join("install_id");
    let id = fs::read_to_string(&file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let id = uuid::Uuid::now_v7().simple().to_string();
            let _ = fs::create_dir_all(app_data);
            let _ = fs::write(&file, &id);
            id
        });
    let _ = INSTALL_ID.set(id);
}

/// Which copy of a vault a device id belongs to: this installation and this
/// folder. A vault folder copied to another computer or place gets a new
/// device id; with the old one, each copy would skip the other's ops as its own.
fn vault_binding(vault_root: &Path) -> String {
    let install = INSTALL_ID.get().map_or("", String::as_str);
    let root = vault_root.to_string_lossy();
    let root = root.trim_end_matches(['/', '\\']).to_lowercase();
    notium_core::hash::content_hash(format!("{install}/{root}").as_bytes())[..16].to_string()
}

/// Read sync configuration from `<vault>/.app/sync_config.json`, resolving the
/// auth token from the OS credential store. A plaintext token left in the file
/// by an older version is migrated to the credential store on first load.
pub fn load_sync_config(vault_root: &Path) -> SyncConfig {
    #[cfg_attr(test, allow(unused_mut))]
    let mut cfg = load_sync_config_file(vault_root);
    // shortcut: fork bridge, sync is always off so this build never talks to the
    // real account; delete this line (and CONFIG_FILE) to re-enable sync.
    #[cfg(not(test))]
    {
        cfg.enabled = false;
    }
    cfg
}

/// Fork bridge: its own file, so it never reads or rewrites the main app's
/// `sync_config.json` when both open the same vault folder.
pub(crate) const CONFIG_FILE: &str = "sync_config.html-fork.json";

fn load_sync_config_file(vault_root: &Path) -> SyncConfig {
    let p = vault_root.join(".app").join(CONFIG_FILE);
    let binding = vault_binding(vault_root);
    if let Ok(bytes) = fs::read(&p) {
        if let Ok(mut cfg) = serde_json::from_slice::<SyncConfig>(&bytes) {
            let plaintext = !cfg.auth_token.is_empty();
            if !plaintext {
                cfg.auth_token = fetch_token(&cfg.device_id);
            }
            if cfg.bound_to != binding {
                // Configs from before the binding keep their id.
                if !cfg.bound_to.is_empty() {
                    cfg.device_id = new_device_id();
                }
                cfg.bound_to = binding;
                let _ = save_sync_config(vault_root, &cfg);
            } else if plaintext && store_token(&cfg.device_id, &cfg.auth_token) {
                let _ = write_config_file(vault_root, &cfg, false);
            }
            return cfg;
        }
    }
    let def = SyncConfig {
        bound_to: binding,
        ..SyncConfig::default()
    };
    let _ = save_sync_config(vault_root, &def);
    def
}

/// Save sync configuration to `<vault>/.app/sync_config.json`. The auth token
/// goes to the OS credential store when one is available and is then omitted
/// from the file.
pub fn save_sync_config(vault_root: &Path, config: &SyncConfig) -> Result<(), CommandError> {
    let in_os_store = store_token(&config.device_id, &config.auth_token);
    write_config_file(vault_root, config, !in_os_store)
}

/// Tombstone `note_id` in the sync journal and record a local delete op so the
/// deletion propagates to other devices.
pub fn journal_local_delete(
    h: &mut VaultHandle,
    note_id: &str,
    now: u64,
) -> Result<(), CommandError> {
    h.sync
        .tombstone(note_id, now)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let config = load_sync_config(&h.vault.root);
    let (rev, hash) = h
        .sync
        .get_current(note_id)
        .map_err(sql_err)?
        .unwrap_or_default();
    let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
    // The base names the version deleted: a peer that edited it since keeps the note.
    let op = sync::local_delete_op(&config.device_id, note_id, rev, hlc, now).with_base(&hash);
    // The tombstone rev: a restore from any device builds on it.
    h.sync
        .set_current(note_id, op.new_rev, None)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    h.sync
        .append_journal(&op)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    Ok(())
}

/// Journal a permanent delete of the trashed note `note_id`, so every device
/// drops it from its trash and the server erases its history.
pub fn journal_local_purge(h: &mut VaultHandle, note_id: &str) -> Result<(), CommandError> {
    let config = load_sync_config(&h.vault.root);
    let now = vault::now_secs();
    let rev = h
        .sync
        .get_current(note_id)
        .map_err(sql_err)?
        .map(|(r, _)| r)
        .unwrap_or(0);
    let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
    let op = sync::local_purge_op(&config.device_id, note_id, rev, hlc, now);
    h.sync
        .set_current(note_id, op.new_rev, None)
        .map_err(sql_err)?;
    h.sync.tombstone(note_id, now).map_err(sql_err)?;
    h.sync.append_journal(&op).map_err(sql_err)?;
    Ok(())
}

/// Permanently remove this device's trash copies of `note_id`.
fn drop_trashed_copies(h: &VaultHandle, note_id: &str) -> Result<(), CommandError> {
    for entry in h.vault.list_trash()? {
        if entry.note_id.as_deref() == Some(note_id) {
            h.vault.delete_trash_item(&entry.trash_id)?;
        }
    }
    Ok(())
}

/// Journal deletes for notes just removed from disk, except those whose id is
/// still carried by another file (a duplicate was deleted, not the note): the
/// reconcile hands the id to that file, and deleting it on every other device
/// would remove the real note there.
pub fn journal_deletes_unless_still_present(
    h: &mut VaultHandle,
    note_ids: &[String],
    now: u64,
) -> Result<(), CommandError> {
    h.index.reconcile(&h.vault).map_err(sql_err)?;
    for id in note_ids {
        let live = h
            .index
            .get_note(id)
            .map_err(sql_err)?
            .is_some_and(|rec| !rec.tombstoned);
        if !live {
            journal_local_delete(h, id, now)?;
        }
    }
    Ok(())
}

/// Journal a path change (rename or move) of `note_id`, now at `rel_path`, as
/// an update op with the unchanged content and the new path. Peers that know
/// about paths move the file; older peers see a no-op content update.
pub fn journal_local_path(
    h: &mut VaultHandle,
    note_id: &str,
    rel_path: &str,
) -> Result<(), CommandError> {
    let config = load_sync_config(&h.vault.root);
    let now = vault::now_secs();
    let bytes = h.vault.read(rel_path)?;
    let (rev, hash) = h
        .sync
        .get_current(note_id)
        .map_err(sql_err)?
        .unwrap_or_default();
    let hlc = h.sync.next_path_hlc(note_id, now).map_err(sql_err)?;
    let op = sync::local_update_op(&config.device_id, note_id, rev, bytes.clone(), hlc, now)
        .with_path(rel_path)
        .with_base(&hash);
    h.sync.claim_path(&op).map_err(sql_err)?;
    h.sync
        .set_current(note_id, rev + 1, Some(&bytes))
        .map_err(sql_err)?;
    h.sync.append_journal(&op).map_err(sql_err)?;
    Ok(())
}

/// Ensure every tombstoned note has a local delete op journaled. This heals
/// deletions that happened before the bulk-delete paths journaled ops, so they
/// propagate on the next push.
fn heal_tombstoned_deletes(h: &mut VaultHandle) -> Result<usize, CommandError> {
    let notes = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let now = vault::now_ts().parse::<u64>().unwrap_or(0);
    let mut healed = 0usize;
    for rec in notes {
        if rec.tombstoned
            && !h
                .sync
                .has_delete_op(&rec.note_id)
                .map_err(|e| CommandError::Sql(e.to_string()))?
        {
            journal_local_delete(h, &rec.note_id, now)?;
            healed += 1;
        }
    }
    Ok(healed)
}

/// Journal notes changed on disk outside the app (another editor, a script, a
/// file-sync tool, edits made while the app was closed): their indexed hash
/// differs from the last synced one, or sync has never seen them.
fn journal_external_edits(h: &mut VaultHandle, device_id: &str) -> Result<usize, CommandError> {
    // Cheap: unchanged files are not re-read (mtime cache in the index).
    h.index.reconcile(&h.vault).map_err(sql_err)?;
    let now = vault::now_secs();
    let mut journaled = 0usize;
    for rec in h.index.list_notes().map_err(sql_err)? {
        if rec.tombstoned || h.sync.is_tombstoned(&rec.note_id).map_err(sql_err)? {
            continue;
        }
        let current = h.sync.get_current(&rec.note_id).map_err(sql_err)?;
        if current
            .as_ref()
            .is_some_and(|(_, hash)| *hash == rec.content_hash)
        {
            continue;
        }
        let bytes = h.vault.read(&rec.path)?;
        // A file carrying another note's id (a conflict copy, a duplicated file)
        // would spread the collision to every device: leave it local.
        if vault::extract_embedded_id(&bytes, EditorKind::Tiptap).as_deref()
            != Some(rec.note_id.as_str())
        {
            continue;
        }
        let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
        let (op, rev) = match current {
            Some((rev, hash)) => (
                sync::local_update_op(device_id, &rec.note_id, rev, bytes.clone(), hlc, now)
                    .with_base(&hash),
                rev + 1,
            ),
            None => {
                let op = sync::local_create_op(device_id, &rec.note_id, bytes.clone(), hlc, now)
                    .with_path(&rec.path);
                h.sync.claim_path(&op).map_err(sql_err)?;
                (op, 0)
            }
        };
        h.sync
            .set_current(&rec.note_id, rev, Some(&bytes))
            .map_err(sql_err)?;
        h.sync.append_journal(&op).map_err(sql_err)?;
        journaled += 1;
    }
    Ok(journaled)
}

/// Folders ride the note journal as content-less ops on `folder:<path under
/// notes/tiptap>`, so empty folders reach other devices without a server change.
pub const FOLDER_OP_PREFIX: &str = "folder:";
const SYNCED_FOLDERS_META: &str = "synced_folders";

fn synced_folders(h: &VaultHandle) -> Result<BTreeSet<String>, CommandError> {
    let raw = h.sync.get_meta(SYNCED_FOLDERS_META).map_err(sql_err)?;
    Ok(raw
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

fn save_synced_folders(h: &mut VaultHandle, set: &BTreeSet<String>) -> Result<(), CommandError> {
    let json = serde_json::to_string(set).map_err(|e| CommandError::Io(e.to_string()))?;
    h.sync.set_meta(SYNCED_FOLDERS_META, &json).map_err(sql_err)
}

/// Journal a create or delete op for every folder that appeared or vanished
/// since the last sync. Diffing the disk catches every way a folder changes
/// (create, rename, delete, move, edits outside the app).
fn journal_folder_changes(h: &mut VaultHandle, device_id: &str) -> Result<(), CommandError> {
    let prefix = format!("{}/", vault::NOTES_DIR);
    let local: BTreeSet<String> = crate::commands::list_folders_impl(h)?
        .into_iter()
        .filter_map(|f| f.path.strip_prefix(&prefix).map(str::to_string))
        .collect();
    let known = synced_folders(h)?;
    if local == known {
        return Ok(());
    }
    let now = vault::now_secs();
    // Reverse order deletes children before their parents.
    let gone: Vec<&String> = known.difference(&local).collect();
    for path in gone.into_iter().rev() {
        let id = format!("{FOLDER_OP_PREFIX}{path}");
        let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
        let op = sync::local_delete_op(device_id, &id, 0, hlc, now);
        h.sync.append_journal(&op).map_err(sql_err)?;
    }
    for path in local.difference(&known) {
        let id = format!("{FOLDER_OP_PREFIX}{path}");
        let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
        let mut op = sync::local_create_op(device_id, &id, Vec::new(), hlc, now);
        op.content = None;
        h.sync.append_journal(&op).map_err(sql_err)?;
    }
    save_synced_folders(h, &local)
}

/// Apply a remote folder op. A delete removes the folder only when it is empty:
/// notes still in it here keep it, and the next diff announces it again.
fn apply_folder_op(h: &mut VaultHandle, folder: &str, delete: bool) -> Result<bool, CommandError> {
    let Ok(clean) = notium_core::paths::sanitize_folder_path(folder) else {
        return Ok(false);
    };
    if clean.is_empty() {
        return Ok(false);
    }
    let dir = h.vault.root.join(vault::NOTES_DIR).join(&clean);
    let mut known = synced_folders(h)?;
    let changed = if delete {
        known.remove(&clean);
        fs::remove_dir(&dir).is_ok()
    } else {
        known.insert(clean);
        !dir.exists() && fs::create_dir_all(&dir).is_ok()
    };
    save_synced_folders(h, &known)?;
    Ok(changed)
}

/// Shared handle to the currently open vault (the value in `AppState::vault`).
pub type SharedVault = Arc<Mutex<Option<VaultHandle>>>;

/// Serializes sync cycles: the background worker and `sync_now` must not
/// interleave now that the vault lock is released between steps.
static SYNC_CYCLE: Mutex<()> = Mutex::new(());

fn sql_err<E: std::fmt::Display>(e: E) -> CommandError {
    CommandError::Sql(e.to_string())
}

/// Root and sync config of the open vault, read under a short lock.
pub fn snapshot_config(shared: &SharedVault) -> Option<(PathBuf, SyncConfig)> {
    let guard = shared.lock().ok()?;
    let h = guard.as_ref()?;
    let root = h.vault.root.clone();
    let config = load_sync_config(&root);
    Some((root, config))
}

/// Run `f` against the open vault while holding the lock. Fails if the vault
/// was closed or switched to another root since the cycle started, so results
/// fetched for one vault are never applied to another.
fn with_vault<T>(
    shared: &SharedVault,
    root: &Path,
    f: impl FnOnce(&mut VaultHandle) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let mut guard = shared
        .lock()
        .map_err(|_| CommandError::Io("lock poisoned".into()))?;
    let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
    if h.vault.root.as_path() != root {
        return Err(CommandError::Io("vault changed during sync".into()));
    }
    f(h)
}

/// What applying one remote op changed on this device.
#[derive(Default)]
struct Applied {
    pulled: bool,
    conflict: bool,
    changed: bool,
}

/// Apply pulled remote ops to the local vault. Returns
/// `(ops_pulled, conflicts, any_local_change)`.
///
/// Ops that fail to apply (a locked file, a full disk) are queued and retried
/// first on the next call, and later ops of the same note wait behind them so
/// each note keeps its order.
fn apply_pulled_ops(
    h: &mut VaultHandle,
    ops: &[Op],
    device_id: &str,
) -> Result<(usize, usize, bool), CommandError> {
    let mut pulled = 0usize;
    let mut conflicts = 0usize;
    let mut any_local_change = false;
    let now = vault::now_secs();
    let mut queue = h.sync.take_deferred_ops().map_err(sql_err)?;
    queue.extend(ops.iter().cloned());
    let mut blocked = std::collections::HashSet::new();
    for op in &queue {
        // Our own ops (also those journaled under an earlier device id) and ops
        // already applied (pull pages repeat) change nothing.
        if op.device_id == device_id || h.sync.is_known_op(&op.op_id).map_err(sql_err)? {
            continue;
        }
        h.sync.observe_hlc(op.hlc, now).map_err(sql_err)?;
        if blocked.contains(&op.note_id) {
            h.sync.defer_op(op).map_err(sql_err)?;
            continue;
        }
        match apply_one(h, op, device_id) {
            Ok(step) => {
                h.sync.mark_applied(&op.op_id).map_err(sql_err)?;
                pulled += step.pulled as usize;
                conflicts += step.conflict as usize;
                any_local_change |= step.changed;
            }
            Err(e) => {
                log::warn!("sync: op {} deferred, could not apply it: {e}", op.op_id);
                h.sync.defer_op(op).map_err(sql_err)?;
                blocked.insert(op.note_id.clone());
            }
        }
    }
    Ok((pulled, conflicts, any_local_change))
}

/// Apply one remote op (see `apply_pulled_ops`).
fn apply_one(h: &mut VaultHandle, op: &Op, device_id: &str) -> Result<Applied, CommandError> {
    let done = Applied {
        pulled: true,
        changed: true,
        conflict: false,
    };
    if let Some(folder) = op.note_id.strip_prefix(FOLDER_OP_PREFIX) {
        let changed = apply_folder_op(h, folder, op.kind == sync::OpKind::Delete)?;
        return Ok(Applied {
            pulled: changed,
            changed,
            conflict: false,
        });
    }
    // Same NFC names as the local scan, whatever form the sender used.
    let normalized;
    let op = match op.path.as_deref().map(notium_core::paths::nfc) {
        Some(std::borrow::Cow::Owned(path)) => {
            normalized = op.clone().with_path(path);
            &normalized
        }
        _ => op,
    };

    // Determine note path: existing note, the path the op sets, or one
    // derived from the content.
    let existing = h.index.get_note(&op.note_id).map_err(sql_err)?;
    // Last path change wins (HLC order), so all devices converge.
    let path_wins = h.sync.claim_path(op).map_err(sql_err)?;
    let wanted_path = op
        .path
        .as_deref()
        .filter(|p| path_wins && is_free_note_path(h, p, &op.note_id));

    let rel_path = if let Some(rec) = &existing {
        rec.path.clone()
    } else if let Some(p) = wanted_path {
        p.to_string()
    } else {
        let title = derive_note_title(&op.content, &op.note_id);
        let mut candidate = h.vault.dedup_path("notes/tiptap", &title, "json");
        let mut n = 2;
        while h
            .index
            .is_path_taken(&candidate, Some(&op.note_id))
            .unwrap_or(false)
        {
            candidate = h
                .vault
                .dedup_path("notes/tiptap", &format!("{title} {n}"), "json");
            n += 1;
        }
        candidate
    };

    // Permanent delete from another device's trash: drop our trashed
    // copies. A note that is live here (it was edited concurrently) stays.
    if op.purge {
        if existing.as_ref().is_none_or(|rec| rec.tombstoned) {
            drop_trashed_copies(h, &op.note_id)?;
            h.sync.tombstone(&op.note_id, op.ts).map_err(sql_err)?;
            h.sync
                .set_current(&op.note_id, op.new_rev, None)
                .map_err(sql_err)?;
            return Ok(done);
        }
        return Ok(Applied::default());
    }

    // Restore from another device's trash: a content op built on our
    // tombstone rev brings the note back (stale edits have a lower base and
    // keep being rejected by `apply_remote`).
    let local_rev = h
        .sync
        .get_current(&op.note_id)
        .map_err(sql_err)?
        .map(|(r, _)| r);
    if let (Some(content), true, Some(rev)) = (
        &op.content,
        h.sync.is_tombstoned(&op.note_id).map_err(sql_err)?,
        local_rev,
    ) {
        if op.kind != sync::OpKind::Delete && op.base_rev >= rev {
            drop_trashed_copies(h, &op.note_id)?;
            let mut target = wanted_path.map_or(rel_path.clone(), str::to_string);
            // A file already there that is this same note (restored here, or
            // never trashed) is updated in place: writing next to it would
            // leave two files with one id.
            if h.vault.exists(&target) && !carries_id(h, &target, &op.note_id) {
                let title = vault::title_from_path(&target);
                let dir = target.rsplit_once('/').map_or("notes/tiptap", |(d, _)| d);
                target = h.vault.dedup_path(dir, &title, "json");
            }
            h.vault.write_atomic(&target, content)?;
            h.sync.clear_tombstone(&op.note_id).map_err(sql_err)?;
            h.sync
                .set_current(&op.note_id, op.new_rev, Some(content))
                .map_err(sql_err)?;
            let _ = h.index.reconcile(&h.vault);
            return Ok(done);
        }
    }

    let (outcome, copy) =
        sync::apply_remote_ex(&mut h.sync, &h.vault, op, "tiptap", &rel_path).map_err(sql_err)?;

    let mut step = Applied::default();
    match outcome {
        ApplyOutcome::Created | ApplyOutcome::FastForward | ApplyOutcome::Merged { .. } => {
            if let Ok(bytes) = h.vault.read(&rel_path) {
                let _ = h
                    .index
                    .reindex_note(&op.note_id, &rel_path, EditorKind::Tiptap, &bytes);
                step = Applied {
                    conflict: copy.is_some(),
                    ..done
                };
            }
            if let ApplyOutcome::Merged { .. } = outcome {
                journal_merge_result(h, op, &rel_path, device_id, copy)?;
            }
        }
        ApplyOutcome::Deleted => {
            let _ = h.index.mark_tombstoned(&op.note_id);
            step = done;
        }
        ApplyOutcome::DeleteConflict => {
            // Edited here while deleted elsewhere: the edit wins on every
            // device, so bring the note back there.
            if existing.as_ref().is_some_and(|rec| !rec.tombstoned) {
                journal_restore(h, &op.note_id, &rel_path, op.new_rev, device_id)?;
            }
            step.conflict = true;
        }
        ApplyOutcome::Noop | ApplyOutcome::RejectedTombstone => {}
    }

    // A rename/move from another device: move the file to the new path.
    if let (Some(rec), Some(target)) = (&existing, wanted_path) {
        if target != rec.path && h.vault.exists(&rec.path) {
            match h.vault.move_to(&rec.path, target) {
                Ok(()) => {
                    let _ = h.index.reconcile(&h.vault);
                    step.changed = true;
                }
                Err(e) => log::warn!("sync: could not move {} to {target}: {e}", rec.path),
            }
        }
    }
    Ok(step)
}

/// After a merge, journal what peers cannot work out alone: the conflict copy
/// (a note of its own) and the merged content when it differs from the op's,
/// so peers that do not merge (the web client) still converge.
fn journal_merge_result(
    h: &mut VaultHandle,
    op: &Op,
    rel_path: &str,
    device_id: &str,
    copy: Option<sync::ConflictCopy>,
) -> Result<(), CommandError> {
    let now = vault::now_secs();
    if let Some(copy) = copy {
        h.index
            .reindex_note(&copy.note_id, &copy.path, EditorKind::Tiptap, &copy.content)
            .map_err(sql_err)?;
        let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
        let create =
            sync::local_create_op(device_id, &copy.note_id, copy.content.clone(), hlc, now)
                .with_path(&copy.path);
        h.sync.claim_path(&create).map_err(sql_err)?;
        h.sync
            .set_current(&copy.note_id, 0, Some(&copy.content))
            .map_err(sql_err)?;
        h.sync.append_journal(&create).map_err(sql_err)?;
    }
    let merged = h.vault.read(rel_path)?;
    if op.content.as_deref() == Some(&merged[..]) {
        return Ok(());
    }
    let rev = h
        .sync
        .get_current(&op.note_id)
        .map_err(sql_err)?
        .map_or(0, |(r, _)| r);
    let base = op
        .content
        .as_deref()
        .map(notium_core::hash::content_hash)
        .unwrap_or_default();
    let hlc = h.sync.tick_hlc(now).map_err(sql_err)?;
    let update = sync::local_update_op(device_id, &op.note_id, rev, merged.clone(), hlc, now)
        .with_base(&base);
    h.sync
        .set_current(&op.note_id, rev + 1, Some(&merged))
        .map_err(sql_err)?;
    h.sync.append_journal(&update).map_err(sql_err)
}

/// Journal the note at `rel_path` as a restore built on the tombstone rev
/// `deleted_rev` of a concurrent delete, so devices that deleted it bring it back.
fn journal_restore(
    h: &mut VaultHandle,
    note_id: &str,
    rel_path: &str,
    deleted_rev: u64,
    device_id: &str,
) -> Result<(), CommandError> {
    let Ok(bytes) = h.vault.read(rel_path) else {
        return Ok(());
    };
    let now = vault::now_secs();
    let hlc = h.sync.next_path_hlc(note_id, now).map_err(sql_err)?;
    let op = sync::local_update_op(device_id, note_id, deleted_rev, bytes.clone(), hlc, now)
        .with_path(rel_path);
    h.sync.claim_path(&op).map_err(sql_err)?;
    h.sync
        .set_current(note_id, deleted_rev + 1, Some(&bytes))
        .map_err(sql_err)?;
    h.sync.append_journal(&op).map_err(sql_err)
}

/// Whether the note file at `rel_path` carries `note_id` in its frontmatter.
fn carries_id(h: &VaultHandle, rel_path: &str, note_id: &str) -> bool {
    h.vault
        .read(rel_path)
        .ok()
        .and_then(|b| vault::extract_embedded_id(&b, EditorKind::Tiptap))
        .is_some_and(|id| id == note_id)
}

/// Whether a remote op may place `note_id` at `path`: a safe note path that no
/// other note or file already uses.
fn is_free_note_path(h: &VaultHandle, path: &str, note_id: &str) -> bool {
    notium_core::paths::is_safe_note_rel_path(path)
        && !h.index.is_path_taken(path, Some(note_id)).unwrap_or(true)
        && h.index
            .get_note(note_id)
            .ok()
            .flatten()
            .map_or(!h.vault.exists(path), |rec| {
                // A case-only rename names the note's own file on a
                // case-insensitive file system (Windows, macOS).
                rec.path.to_lowercase() == path.to_lowercase() || !h.vault.exists(path)
            })
}

/// Page size of `/sync/pull` (server/cloudflare-sync/src/index.ts).
const PULL_PAGE: usize = 500;
/// Pages pulled per cycle at most; the next cycle continues from the cursor.
const MAX_PULL_PAGES: usize = 20;

/// Execute a complete sync cycle (Push mutations -> Pull remote -> Sync blobs).
///
/// The vault lock is taken only for the short local steps (`with_vault`); every
/// HTTP request runs unlocked so IPC commands such as `note_save` are never
/// blocked behind a slow or unreachable server.
///
/// A failed push does not skip the pull: this device keeps receiving changes
/// while the server refuses its own, and the push error is returned at the end.
pub fn run_sync_cycle(
    shared: &SharedVault,
    root: &Path,
    config: &SyncConfig,
) -> Result<SyncCycleResult, CommandError> {
    let mut report = SyncCycleResult {
        ops_pushed: 0,
        ops_pulled: 0,
        conflicts: 0,
        blobs_synced: 0,
        external_edits: 0,
    };
    if !config.enabled || config.server_url.trim().is_empty() {
        return Ok(report);
    }
    let _cycle = SYNC_CYCLE
        .lock()
        .map_err(|_| CommandError::Io("lock poisoned".into()))?;

    let client = SyncClient::new(
        &config.server_url,
        &config.auth_token,
        &config.device_id,
        &config.vault_id,
    );

    // 1. Push local unpushed mutations in chunks of 50.
    // Heal first: journal delete ops for tombstoned notes that were never synced
    // as deleted (e.g. deleted via a folder delete before that path journaled ops).
    let unpushed = with_vault(shared, root, |h| {
        // External edits first: their reconcile revives a note whose id a
        // surviving duplicate still carries, so healing never deletes it.
        report.external_edits = journal_external_edits(h, &config.device_id)?;
        heal_tombstoned_deletes(h)?;
        journal_folder_changes(h, &config.device_id)?;
        h.sync.get_unpushed_ops().map_err(sql_err)
    })?;
    const CHUNK_SIZE: usize = 50;
    let mut push_error = None;
    let mut rejected = 0usize;
    for chunk in unpushed.chunks(CHUNK_SIZE) {
        let push_res = match client.push(chunk) {
            Ok(res) => res,
            Err(e) => {
                push_error = Some(e);
                break;
            }
        };
        // Rejected ops stay queued. A reply that lists neither (no server
        // version does) is read as everything accepted.
        rejected += push_res.rejected_op_ids.len();
        let accepted: Vec<String> =
            if push_res.accepted_op_ids.is_empty() && push_res.rejected_op_ids.is_empty() {
                chunk.iter().map(|o| o.op_id.clone()).collect()
            } else {
                push_res.accepted_op_ids
            };
        with_vault(shared, root, |h| {
            let ids: Vec<&str> = accepted.iter().map(|s| s.as_str()).collect();
            h.sync.mark_ops_synced(&ids).map_err(sql_err)
        })?;
        report.ops_pushed += accepted.len();
    }

    // 2. Pull pages of remote mutations until caught up, applying each one.
    let mut cursor = with_vault(shared, root, |h| {
        h.sync.get_meta("last_pull_cursor").map_err(sql_err)
    })?;
    let mut any_local_change = false;
    let mut server_hashes = std::collections::HashSet::new();
    for _ in 0..MAX_PULL_PAGES {
        let page = client.pull(cursor.as_deref()).map_err(CommandError::Io)?;
        let (pulled, conflicts, changed) = with_vault(shared, root, |h| {
            apply_pulled_ops(h, &page.ops, &config.device_id)
        })?;
        report.ops_pulled += pulled;
        report.conflicts += conflicts;
        any_local_change |= changed;
        server_hashes.extend(page.blobs.iter().map(|b| b.hash.clone()));

        // 3a. Download the page's missing or updated blobs. One that failed to
        // download (not one the server no longer has) keeps the cursor here,
        // so the page is read again next cycle.
        let (downloaded, failed) = download_blobs(shared, root, &client, &page.blobs)?;
        report.blobs_synced += downloaded;
        any_local_change |= downloaded > 0;
        if failed {
            break;
        }
        let next = page.cursor.filter(|c| !c.is_empty());
        let advanced = next.is_some() && next != cursor;
        if let Some(c) = &next {
            with_vault(shared, root, |h| {
                h.sync.set_meta("last_pull_cursor", c).map_err(sql_err)
            })?;
        }
        let full = page.ops.len() >= PULL_PAGE || page.blobs.len() >= PULL_PAGE;
        cursor = next.or(cursor);
        if !full || !advanced {
            break;
        }
    }

    // 3b. Upload local attachments if within D1 900 KB limit and not yet in synced_blobs
    let to_upload = with_vault(shared, root, |h| {
        let mut out = Vec::new();
        if let Ok(local_atts) = h.vault.scan_attachments() {
            for att in local_atts {
                if (att.bytes.len() as u64) > MAX_D1_BLOB_BYTES {
                    continue;
                }
                if h.sync
                    .is_blob_synced(&att.hash, &att.rel_path)
                    .unwrap_or(false)
                {
                    continue;
                }
                if server_hashes.contains(&att.hash) {
                    let _ = h.sync.mark_blob_synced(&att.hash, &att.rel_path);
                    continue;
                }
                out.push(att);
            }
        }
        Ok(out)
    })?;
    for att in &to_upload {
        if client
            .push_blob(&att.hash, &att.rel_path, &att.bytes)
            .is_ok()
        {
            with_vault(shared, root, |h| {
                let _ = h.sync.mark_blob_synced(&att.hash, &att.rel_path);
                Ok(())
            })?;
            report.blobs_synced += 1;
        }
    }

    with_vault(shared, root, |h| {
        if any_local_change {
            let _ = h.index.reconcile(&h.vault);
        }
        let now = vault::now_ts().parse::<u64>().unwrap_or(0);
        let _ = h.sync.set_meta("last_synced_at", &now.to_string());
        Ok(())
    })?;

    if let Some(e) = push_error {
        return Err(CommandError::Io(e));
    }
    if rejected > 0 {
        return Err(CommandError::Io(format!(
            "El servidor rechazó {rejected} cambios (¿nota demasiado grande?); siguen pendientes"
        )));
    }
    Ok(report)
}

/// Download the blobs of a pull page that are missing or changed here.
/// Returns `(written, failed)`; a blob the server no longer has is skipped.
fn download_blobs(
    shared: &SharedVault,
    root: &Path,
    client: &SyncClient,
    blobs: &[BlobRef],
) -> Result<(usize, bool), CommandError> {
    let to_download: Vec<BlobRef> = with_vault(shared, root, |h| {
        let mut out = Vec::new();
        for blob in blobs {
            if !is_safe_attachment_rel_path(&blob.path) {
                continue;
            }
            if h.sync
                .is_blob_synced(&blob.hash, &blob.path)
                .unwrap_or(false)
                && h.vault.exists(&blob.path)
            {
                continue;
            }
            if h.vault.exists(&blob.path) {
                if let Ok(existing) = h.vault.read(&blob.path) {
                    if notium_core::hash::content_hash(&existing) == blob.hash {
                        let _ = h.sync.mark_blob_synced(&blob.hash, &blob.path);
                        continue;
                    }
                }
            }
            out.push(blob.clone());
        }
        Ok(out)
    })?;

    let mut written = 0usize;
    let mut failed = false;
    for blob in &to_download {
        match client.pull_blob(&blob.hash) {
            Ok(Some(bytes)) => {
                if (bytes.len() as u64) <= MAX_D1_BLOB_BYTES
                    && notium_core::hash::content_hash(&bytes) == blob.hash
                {
                    let ok = with_vault(shared, root, |h| {
                        if h.vault.write_atomic(&blob.path, &bytes).is_ok() {
                            let _ = h.sync.mark_blob_synced(&blob.hash, &blob.path);
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    })?;
                    written += ok as usize;
                }
            }
            Ok(None) => log::warn!("sync: blob {} is gone from the server", blob.path),
            Err(_) => failed = true,
        }
    }
    Ok((written, failed))
}

fn derive_note_title(content: &Option<Vec<u8>>, note_id: &str) -> String {
    if let Some(bytes) = content {
        if let Ok(extracted) = notium_core::extract::extract(EditorKind::Tiptap, note_id, bytes) {
            if !extracted.title.trim().is_empty() {
                return extracted.title;
            }
        }
    }
    format!("Nota {}", &note_id[..note_id.len().min(8)])
}

// --- Sync Manager & Background Polling ---

enum SyncTrigger {
    Tick,
    Immediate,
    ConfigChanged,
    Pause,
    Resume,
    #[allow(dead_code)]
    Shutdown,
}

struct InnerSyncState {
    pub state: String,
    pub last_synced_at: Option<u64>,
    pub last_error: Option<String>,
    pub paused: bool,
}

pub struct SyncManager {
    sender: Sender<SyncTrigger>,
    state: Arc<Mutex<InnerSyncState>>,
}

impl SyncManager {
    pub fn start(active_vault: Arc<Mutex<Option<VaultHandle>>>, app: AppHandle) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel::<SyncTrigger>();
        let state = Arc::new(Mutex::new(InnerSyncState {
            state: "offline".to_string(),
            last_synced_at: None,
            last_error: None,
            paused: false,
        }));

        let mgr = Arc::new(SyncManager {
            sender: sender.clone(),
            state: state.clone(),
        });

        let worker_state = state.clone();
        let worker_vault = active_vault.clone();
        let worker_app = app.clone();

        thread::spawn(move || {
            sync_worker_loop(receiver, worker_state, worker_vault, worker_app);
        });

        // Background adaptive ticker
        let ticker_sender = sender.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(15));
            if ticker_sender.send(SyncTrigger::Tick).is_err() {
                break;
            }
        });

        mgr
    }

    pub fn trigger_sync(&self) {
        let _ = self.sender.send(SyncTrigger::Immediate);
    }

    pub fn trigger_config_changed(&self) {
        let _ = self.sender.send(SyncTrigger::ConfigChanged);
    }

    pub fn pause(&self) {
        let _ = self.sender.send(SyncTrigger::Pause);
    }

    pub fn resume(&self) {
        let _ = self.sender.send(SyncTrigger::Resume);
    }

    pub fn get_status(&self, h: &VaultHandle) -> SyncStatusDto {
        let config = load_sync_config(&h.vault.root);
        let pending = h.sync.unpushed_count().unwrap_or(0);
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());

        let effective_state = if !config.enabled {
            "offline".to_string()
        } else if st.paused {
            "paused".to_string()
        } else {
            st.state.clone()
        };

        let last_synced = st.last_synced_at.or_else(|| {
            h.sync
                .get_meta("last_synced_at")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<u64>().ok())
        });

        SyncStatusDto {
            enabled: config.enabled,
            server_url: config.server_url,
            device_id: config.device_id,
            vault_id: normalize_vault_id(&config.vault_id),
            state: effective_state,
            last_synced_at: last_synced,
            pending_ops: pending,
            last_error: st.last_error.clone(),
        }
    }
}

/// Doubling retry delay after a failed cycle: 2 s, 4 s, 8 s, 16 s, then 30 s.
fn next_retry(prev: Option<Duration>) -> Duration {
    prev.map_or(Duration::from_secs(2), |d| {
        (d * 2).min(Duration::from_secs(30))
    })
}

fn sync_worker_loop(
    receiver: Receiver<SyncTrigger>,
    state: Arc<Mutex<InnerSyncState>>,
    active_vault: Arc<Mutex<Option<VaultHandle>>>,
    app: AppHandle,
) {
    let mut last_run = Instant::now() - Duration::from_secs(120);
    // After a failed cycle (often the network still waking up on app resume),
    // retry soon instead of waiting for the next tick: 2 s, 4 s, ... up to 30 s.
    let mut retry_in: Option<Duration> = None;

    loop {
        let trigger = match retry_in {
            None => match receiver.recv() {
                Ok(t) => t,
                Err(_) => break,
            },
            Some(wait) => match receiver.recv_timeout(wait) {
                Ok(t) => t,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if state.lock().map(|s| s.paused).unwrap_or(false) {
                        retry_in = None;
                        continue;
                    }
                    SyncTrigger::Immediate
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
        };
        match trigger {
            SyncTrigger::Shutdown => break,
            SyncTrigger::Pause => {
                if let Ok(mut st) = state.lock() {
                    st.paused = true;
                    st.state = "paused".to_string();
                }
                emit_status(&state, &active_vault, &app);
                continue;
            }
            SyncTrigger::Resume => {
                if let Ok(mut st) = state.lock() {
                    st.paused = false;
                    st.state = "synced".to_string();
                }
                emit_status(&state, &active_vault, &app);
            }
            SyncTrigger::ConfigChanged | SyncTrigger::Immediate => {
                // proceed immediately
            }
            SyncTrigger::Tick => {
                let is_paused = state.lock().map(|s| s.paused).unwrap_or(false);
                if is_paused {
                    continue;
                }
                // Ticks come every 15 s and `last_run` is set when a cycle ends,
                // so a 15 s threshold skipped every other tick.
                if last_run.elapsed() < Duration::from_secs(10) {
                    continue;
                }
            }
        }

        // Execute sync cycle. Only short local steps take the vault lock; the
        // network I/O inside `run_sync_cycle` runs unlocked.
        let Some((root, config)) = snapshot_config(&active_vault) else {
            continue;
        };
        if !config.enabled || config.server_url.trim().is_empty() {
            if let Ok(mut st) = state.lock() {
                st.state = "offline".to_string();
                st.last_error = None;
            }
            emit_status(&state, &active_vault, &app);
            continue;
        }

        // Set status to syncing
        if let Ok(mut st) = state.lock() {
            st.state = "syncing".to_string();
        }
        emit_status(&state, &active_vault, &app);

        // Run cycle
        let result = run_sync_cycle(&active_vault, &root, &config);
        last_run = Instant::now();

        match result {
            Ok(report) => {
                retry_in = None;
                let now = vault::now_ts().parse::<u64>().unwrap_or(0);
                if let Ok(mut st) = state.lock() {
                    st.state = "synced".to_string();
                    st.last_synced_at = Some(now);
                    st.last_error = None;
                }
                emit_status(&state, &active_vault, &app);
                if report.ops_pulled > 0 || report.blobs_synced > 0 {
                    let _ = app.emit("vault://reconciled", ());
                }
                if report.external_edits > 0 {
                    // The open note's rev may have moved: the editor re-reads it
                    // (an empty path list means "any note may have changed").
                    let _ = app.emit("vault-changed", Vec::<String>::new());
                }
            }
            Err(e) => {
                retry_in = Some(next_retry(retry_in));
                let err_msg = e.to_string();
                if let Ok(mut st) = state.lock() {
                    st.state = "error".to_string();
                    st.last_error = Some(err_msg);
                }
                emit_status(&state, &active_vault, &app);
            }
        }
    }
}

fn emit_status_from_handle(state: &Arc<Mutex<InnerSyncState>>, h: &VaultHandle, app: &AppHandle) {
    let config = load_sync_config(&h.vault.root);
    let pending = h.sync.unpushed_count().unwrap_or(0);
    if let Ok(st) = state.lock() {
        let effective_state = if !config.enabled {
            "offline".to_string()
        } else if st.paused {
            "paused".to_string()
        } else {
            st.state.clone()
        };
        let status_dto = SyncStatusDto {
            enabled: config.enabled,
            server_url: config.server_url.clone(),
            device_id: config.device_id.clone(),
            vault_id: normalize_vault_id(&config.vault_id),
            state: effective_state,
            last_synced_at: st.last_synced_at,
            pending_ops: pending,
            last_error: st.last_error.clone(),
        };
        let _ = app.emit("sync://status-changed", status_dto);
    }
}

fn emit_status(
    state: &Arc<Mutex<InnerSyncState>>,
    active_vault: &Arc<Mutex<Option<VaultHandle>>>,
    app: &AppHandle,
) {
    if let Ok(guard) = active_vault.lock() {
        if let Some(h) = guard.as_ref() {
            emit_status_from_handle(state, h, app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notium_core::clock::Hlc;
    use notium_core::index::Index;
    use notium_core::sync::SyncDb;
    use notium_core::vault::LocalVault;

    fn temp_vault() -> (tempfile::TempDir, VaultHandle) {
        let dir = tempfile::tempdir().unwrap();
        let vault = LocalVault::new(dir.path());
        vault.init().unwrap();
        let index = Index::open(&vault.index_db_path()).unwrap();
        let sync = SyncDb::open(&vault.sync_db_path()).unwrap();
        (
            dir,
            VaultHandle {
                vault,
                index,
                sync,
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                watcher: None,
            },
        )
    }

    fn share(h: VaultHandle) -> (SharedVault, PathBuf) {
        let root = h.vault.root.clone();
        (Arc::new(Mutex::new(Some(h))), root)
    }

    /// Remote op from `dev_remote` with a fixed content and optional path.
    fn remote(note_id: &str, kind: sync::OpKind, rev: u64, ts: u64, path: Option<&str>) -> Op {
        Op {
            op_id: format!("op_{note_id}_{rev}_{ts}"),
            device_id: "dev_remote".into(),
            note_id: note_id.into(),
            kind,
            base_rev: rev.saturating_sub(1),
            new_rev: rev,
            content: Some(
                notium_core::envelope::note_from_text(&format!(
                    "---\nnotium_id: {note_id}\n---\n# Nota\n"
                ))
                .into_bytes(),
            ),
            hlc: Hlc { ts, cnt: 0 },
            ts,
            path: path.map(str::to_string),
            purge: false,
            base_hash: None,
        }
    }

    fn path_of(h: &VaultHandle, note_id: &str) -> String {
        h.index.get_note(note_id).unwrap().unwrap().path
    }

    fn remote_delete(note_id: &str, base: u64, ts: u64, purge: bool) -> Op {
        Op {
            kind: sync::OpKind::Delete,
            base_rev: base,
            new_rev: base + 1,
            content: None,
            path: None,
            purge,
            ..remote(note_id, sync::OpKind::Delete, base + 1, ts, None)
        }
    }

    fn trashed(h: &VaultHandle, note_id: &str) -> usize {
        h.vault
            .list_trash()
            .unwrap()
            .iter()
            .filter(|t| t.note_id.as_deref() == Some(note_id))
            .count()
    }

    #[test]
    fn empty_folders_sync_and_only_empty_ones_are_removed() {
        let (_a, mut a) = temp_vault();
        let (_b, mut b) = temp_vault();
        a.vault.create_folder("Vacía/Sub").unwrap();
        journal_folder_changes(&mut a, "dev_a").unwrap();
        let ops = a.sync.get_unpushed_ops().unwrap();
        assert_eq!(ops.len(), 2, "one create per folder, parent and child");
        // Nothing changed: no new ops on the next cycle.
        journal_folder_changes(&mut a, "dev_a").unwrap();
        assert_eq!(a.sync.get_unpushed_ops().unwrap().len(), 2);

        apply_pulled_ops(&mut b, &ops, "dev_b").unwrap();
        let sub = b.vault.root.join("notes/tiptap/Vacía/Sub");
        assert!(sub.is_dir());
        // B received them, so it does not announce them back.
        journal_folder_changes(&mut b, "dev_b").unwrap();
        assert!(b.sync.get_unpushed_ops().unwrap().is_empty());

        // A deletes the tree; B keeps "Vacía" while it still holds a note.
        a.sync
            .mark_ops_synced(&ops.iter().map(|o| o.op_id.as_str()).collect::<Vec<_>>())
            .unwrap();
        fs::remove_dir_all(a.vault.root.join("notes/tiptap/Vacía")).unwrap();
        journal_folder_changes(&mut a, "dev_a").unwrap();
        let deletes = a.sync.get_unpushed_ops().unwrap();
        assert_eq!(deletes[0].note_id, "folder:Vacía/Sub", "children go first");
        fs::write(b.vault.root.join("notes/tiptap/Vacía/n.json"), "x").unwrap();
        apply_pulled_ops(&mut b, &deletes, "dev_b").unwrap();
        assert!(!sub.exists());
        assert!(b.vault.root.join("notes/tiptap/Vacía").is_dir());
    }

    #[test]
    fn remote_delete_restore_and_purge_round_trip() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/A.json"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();

        // Deleted elsewhere: lands in our trash.
        apply_pulled_ops(&mut h, &[remote_delete("n1", 0, 20, false)], "dev_local").unwrap();
        assert!(!h.vault.exists("notes/tiptap/A.json"));
        assert_eq!(trashed(&h, "n1"), 1);

        // A stale edit (base below the tombstone) does not bring it back.
        let stale = remote("n1", sync::OpKind::Update, 1, 25, None);
        apply_pulled_ops(&mut h, &[stale], "dev_local").unwrap();
        assert!(!h.vault.exists("notes/tiptap/A.json"));

        // Restored elsewhere (built on the tombstone rev 1): back, trash empty.
        let mut restore = remote(
            "n1",
            sync::OpKind::Update,
            2,
            30,
            Some("notes/tiptap/A.json"),
        );
        restore.base_rev = 1;
        apply_pulled_ops(&mut h, &[restore], "dev_local").unwrap();
        assert!(h.vault.exists("notes/tiptap/A.json"));
        assert!(!h.index.get_note("n1").unwrap().unwrap().tombstoned);
        assert_eq!(trashed(&h, "n1"), 0);

        // Deleted again, then purged elsewhere: gone from our trash too.
        apply_pulled_ops(&mut h, &[remote_delete("n1", 2, 40, false)], "dev_local").unwrap();
        assert_eq!(trashed(&h, "n1"), 1);
        apply_pulled_ops(&mut h, &[remote_delete("n1", 3, 50, true)], "dev_local").unwrap();
        assert_eq!(trashed(&h, "n1"), 0);
    }

    #[test]
    fn restore_onto_the_notes_own_file_updates_it_in_place() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/A.json"),
        );
        let bytes = create.content.clone().unwrap();
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        apply_pulled_ops(&mut h, &[remote_delete("n1", 0, 20, false)], "dev_local").unwrap();
        // The note's file is back at its path before the restore arrives.
        h.vault.write_atomic("notes/tiptap/A.json", &bytes).unwrap();

        let mut restore = remote(
            "n1",
            sync::OpKind::Update,
            2,
            30,
            Some("notes/tiptap/A.json"),
        );
        restore.base_rev = 1;
        apply_pulled_ops(&mut h, &[restore], "dev_local").unwrap();
        let files: Vec<String> = h
            .vault
            .scan_notes()
            .unwrap()
            .into_iter()
            .map(|n| n.rel_path)
            .collect();
        assert_eq!(files, ["notes/tiptap/A.json"]);
        assert!(!h.index.get_note("n1").unwrap().unwrap().tombstoned);
    }

    #[test]
    fn deleting_a_duplicate_never_deletes_the_note_elsewhere() {
        let (_d, mut h) = temp_vault();
        let note = create_impl_for_test(&mut h, "Pendientes");
        let real = path_of(&h, &note);
        // An older copy carrying the same id, named so it sorts first.
        let copy = "notes/tiptap/Nota note_01a.json";
        h.vault
            .write_atomic(copy, &h.vault.read(&real).unwrap())
            .unwrap();
        h.index.reconcile(&h.vault).unwrap();
        assert_eq!(path_of(&h, &note), real, "the indexed file keeps the id");

        // Deleting the real file while the copy remains: the id moves to the
        // copy and no delete reaches other devices.
        crate::commands::delete_impl(&mut h, &note).unwrap();
        assert!(!h.sync.has_delete_op(&note).unwrap());
        assert_eq!(path_of(&h, &note), copy);
        assert!(!h.index.get_note(&note).unwrap().unwrap().tombstoned);

        // Deleting the last file that carries it is a real delete.
        crate::commands::delete_impl(&mut h, &note).unwrap();
        assert!(h.sync.has_delete_op(&note).unwrap());
    }

    #[test]
    fn purge_never_touches_a_live_note() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/A.json"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        apply_pulled_ops(&mut h, &[remote_delete("n1", 5, 20, true)], "dev_local").unwrap();
        assert!(h.vault.exists("notes/tiptap/A.json"));
    }

    #[test]
    fn local_delete_and_restore_journal_ops_other_devices_accept() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/A.json"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        h.vault.delete_to_trash("notes/tiptap/A.json").unwrap();
        h.index.mark_tombstoned("n1").unwrap();
        journal_local_delete(&mut h, "n1", 100).unwrap();
        let del = h.sync.get_unpushed_ops().unwrap().pop().unwrap();
        assert_eq!((del.base_rev, del.new_rev), (0, 1));
        assert_eq!(h.sync.get_current("n1").unwrap().unwrap().0, 1);

        let trash_id = h.vault.list_trash().unwrap()[0].trash_id.clone();
        h.vault.restore_from_trash(&trash_id).unwrap();
        h.sync.clear_tombstone("n1").unwrap();
        h.index.reconcile(&h.vault).unwrap();
        journal_local_path(&mut h, "n1", "notes/tiptap/A.json").unwrap();
        let restore = h.sync.get_unpushed_ops().unwrap().pop().unwrap();
        // Built on the tombstone rev, so peers treat it as a restore.
        assert_eq!((restore.base_rev, restore.new_rev), (1, 2));
        assert!(restore.content.is_some());
        assert_eq!(restore.path.as_deref(), Some("notes/tiptap/A.json"));
    }

    #[test]
    fn remote_create_uses_op_path_and_rename_moves_the_file() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/Dir/Plan.json"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/tiptap/Dir/Plan.json");

        let rename = remote(
            "n1",
            sync::OpKind::Update,
            1,
            20,
            Some("notes/tiptap/Plan final.json"),
        );
        apply_pulled_ops(&mut h, &[rename], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/tiptap/Plan final.json");
        assert!(h.vault.exists("notes/tiptap/Plan final.json"));
        assert!(!h.vault.exists("notes/tiptap/Dir/Plan.json"));

        // An older path change arriving late does not move it back.
        let stale = remote(
            "n1",
            sync::OpKind::Update,
            2,
            15,
            Some("notes/tiptap/Viejo.json"),
        );
        apply_pulled_ops(&mut h, &[stale], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/tiptap/Plan final.json");
    }

    #[test]
    fn remote_path_is_ignored_when_unsafe_or_taken() {
        let (_d, mut h) = temp_vault();
        let a = remote(
            "a",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/A.json"),
        );
        let b = remote(
            "b",
            sync::OpKind::Create,
            0,
            11,
            Some("notes/tiptap/B.json"),
        );
        apply_pulled_ops(&mut h, &[a, b], "dev_local").unwrap();

        let clash = remote(
            "b",
            sync::OpKind::Update,
            1,
            20,
            Some("notes/tiptap/A.json"),
        );
        let escape = remote(
            "a",
            sync::OpKind::Update,
            1,
            21,
            Some("notes/tiptap/../../x.json"),
        );
        apply_pulled_ops(&mut h, &[clash, escape], "dev_local").unwrap();
        assert_eq!(path_of(&h, "a"), "notes/tiptap/A.json");
        assert_eq!(path_of(&h, "b"), "notes/tiptap/B.json");
    }

    #[test]
    fn local_rename_journals_an_op_with_the_new_path() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/tiptap/Uno.json"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        h.vault
            .move_to("notes/tiptap/Uno.json", "notes/tiptap/Dos.json")
            .unwrap();
        h.index.reconcile(&h.vault).unwrap();
        journal_local_path(&mut h, "n1", "notes/tiptap/Dos.json").unwrap();
        let ops = h.sync.get_unpushed_ops().unwrap();
        let op = ops.last().unwrap();
        assert_eq!(op.path.as_deref(), Some("notes/tiptap/Dos.json"));
        assert_eq!((op.base_rev, op.new_rev), (0, 1));
        // The local change outranks the remote create it renamed.
        assert!(op.hlc.ts >= 10);
    }

    /// The vault lock must be free while the cycle waits on the network, so
    /// commands like `note_save` are not blocked by a slow server.
    #[test]
    fn test_sync_cycle_does_not_hold_vault_lock_during_network_io() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        // Accept the push request, then stall before dropping the connection.
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(1500));
            drop(stream);
        });

        let (_d, mut h) = temp_vault();
        let op = sync::local_create_op(
            "dev_local",
            "note_local_1",
            b"# Local note".to_vec(),
            Hlc::origin().tick(1),
            100,
        );
        h.sync.append_journal(&op).unwrap();
        let (shared, root) = share(h);
        let cfg = SyncConfig {
            server_url,
            auth_token: String::new(),
            device_id: "dev_local".into(),
            vault_id: "default".into(),
            enabled: true,
            auto_sync: true,
            interval_secs: 10,
            bound_to: String::new(),
        };

        let cycle = {
            let shared = shared.clone();
            thread::spawn(move || run_sync_cycle(&shared, &root, &cfg))
        };
        thread::sleep(Duration::from_millis(400));
        assert!(
            !cycle.is_finished(),
            "cycle should still be waiting on the server"
        );
        assert!(
            shared.try_lock().is_ok(),
            "vault lock must not be held during network I/O"
        );

        // The stalled request fails; the op stays queued for the next cycle.
        assert!(cycle.join().unwrap().is_err());
        server.join().unwrap();
        let guard = shared.lock().unwrap();
        assert_eq!(guard.as_ref().unwrap().sync.unpushed_count().unwrap(), 1);
    }

    #[test]
    fn server_url_requires_https_except_loopback() {
        assert!(is_allowed_server_url(
            "https://notium-sync.example.workers.dev"
        ));
        assert!(is_allowed_server_url(" HTTPS://sync.example.com/base "));
        assert!(is_allowed_server_url("http://127.0.0.1:8787"));
        assert!(is_allowed_server_url("http://localhost:8787/x"));
        assert!(is_allowed_server_url("http://[::1]:8787"));
        assert!(!is_allowed_server_url("https://"));
        assert!(!is_allowed_server_url("ftp://sync.example.com"));
        assert!(!is_allowed_server_url("sync.example.com"));
        // Plain http to a remote host is only tolerated in debug builds.
        assert_eq!(
            is_allowed_server_url("http://sync.example.com"),
            cfg!(debug_assertions)
        );
        assert_eq!(
            is_allowed_server_url("http://localhost.evil.com"),
            cfg!(debug_assertions)
        );
    }

    #[test]
    fn test_sync_config_load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load_sync_config(dir.path());
        assert!(!cfg.enabled);
        assert!(!cfg.device_id.is_empty());

        let mut custom = cfg.clone();
        custom.server_url = "https://sync.example.com".to_string();
        custom.auth_token = "secret123".to_string();
        custom.enabled = true;
        save_sync_config(dir.path(), &custom).unwrap();

        let loaded = load_sync_config(dir.path());
        assert_eq!(loaded.server_url, "https://sync.example.com");
        assert_eq!(loaded.auth_token, "secret123");
        assert!(loaded.enabled);
    }

    /// Round-trips a secret through the real OS credential store. Ignored by
    /// default; run with `cargo test --lib -- --ignored os_token_store`.
    #[test]
    #[ignore]
    fn os_token_store_roundtrip() {
        let account = format!("test_{}", uuid::Uuid::now_v7().simple());
        assert_eq!(os_token_store::get(&account), None);
        assert!(os_token_store::set(&account, "s3cret"));
        assert_eq!(os_token_store::get(&account).as_deref(), Some("s3cret"));
        assert!(os_token_store::set(&account, ""));
        assert_eq!(os_token_store::get(&account), None);
        // Deleting a missing entry is still a success.
        assert!(os_token_store::set(&account, ""));
    }

    #[test]
    fn test_sync_cycle_disabled_noop() {
        let (_d, h) = temp_vault();
        let (shared, root) = share(h);
        let cfg = SyncConfig {
            enabled: false,
            ..SyncConfig::default()
        };
        let res = run_sync_cycle(&shared, &root, &cfg).unwrap();
        assert_eq!(res.ops_pushed, 0);
        assert_eq!(res.ops_pulled, 0);
    }

    #[test]
    fn test_heal_tombstoned_deletes_journals_missing_ops() {
        let (_d, mut h) = temp_vault();
        h.vault
            .write_atomic(
                "notes/tiptap/Foo.json",
                notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                    b"# Foo\n\nbody\n".as_ref(),
                ))
                .as_bytes(),
            )
            .unwrap();
        h.index.reconcile(&h.vault).unwrap();
        let note_id = h.index.list_notes().unwrap()[0].note_id.clone();

        // Simulate a deletion that tombstones but does not journal a delete op.
        h.index.mark_tombstoned(&note_id).unwrap();
        h.sync.tombstone(&note_id, 100).unwrap();
        assert!(!h.sync.has_delete_op(&note_id).unwrap());

        let healed = heal_tombstoned_deletes(&mut h).unwrap();
        assert_eq!(healed, 1);
        assert!(h.sync.has_delete_op(&note_id).unwrap());

        // Idempotent: a second heal journals nothing new.
        assert_eq!(heal_tombstoned_deletes(&mut h).unwrap(), 0);
    }

    #[test]
    fn failed_cycles_retry_with_capped_backoff() {
        let secs: Vec<u64> =
            std::iter::successors(Some(next_retry(None)), |d| Some(next_retry(Some(*d))))
                .take(6)
                .map(|d| d.as_secs())
                .collect();
        assert_eq!(secs, [2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn external_edits_and_new_files_are_journaled_once() {
        let (_d, mut h) = temp_vault();
        let note = create_impl_for_test(&mut h, "Foo");
        let before = h.sync.get_unpushed_ops().unwrap().len();
        assert_eq!(journal_external_edits(&mut h, "dev_a").unwrap(), 0);

        // Edited outside the app: one update op built on the synced rev.
        let path = path_of(&h, &note);
        let old = String::from_utf8(h.vault.read(&path).unwrap()).unwrap();
        let doc = notium_core::envelope::doc_from_text("done").to_string();
        let bytes = notium_core::envelope::with_doc(&old, &doc)
            .unwrap()
            .into_bytes();
        std::fs::write(h.vault.root.join(&path), &bytes).unwrap();
        let rev = h.sync.get_current(&note).unwrap().unwrap().0;
        assert_eq!(journal_external_edits(&mut h, "dev_a").unwrap(), 1);
        let ops = h.sync.get_unpushed_ops().unwrap();
        let op = ops.last().unwrap();
        assert_eq!(
            (op.note_id.as_str(), op.kind, op.base_rev),
            (note.as_str(), sync::OpKind::Update, rev)
        );
        assert_eq!(op.content.as_deref(), Some(&bytes[..]));
        assert_eq!(h.sync.get_current(&note).unwrap().unwrap().0, rev + 1);

        // A file dropped into the vault: a create op with its path.
        h.vault
            .write_atomic(
                "notes/tiptap/Bar.json",
                notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                    b"# Bar\n".as_ref(),
                ))
                .as_bytes(),
            )
            .unwrap();
        assert_eq!(journal_external_edits(&mut h, "dev_a").unwrap(), 1);
        let op = h.sync.get_unpushed_ops().unwrap().pop().unwrap();
        assert_eq!(op.kind, sync::OpKind::Create);
        assert_eq!(op.path.as_deref(), Some("notes/tiptap/Bar.json"));

        // A copy carrying Foo's id (conflict copy) stays local.
        h.vault
            .write_atomic("notes/tiptap/Foo (conflict).json", &bytes)
            .unwrap();
        assert_eq!(journal_external_edits(&mut h, "dev_a").unwrap(), 0);

        // Idempotent.
        assert_eq!(journal_external_edits(&mut h, "dev_a").unwrap(), 0);
        assert_eq!(h.sync.get_unpushed_ops().unwrap().len(), before + 2);
    }

    /// A note created through the app's create path (journaled + current set).
    fn create_impl_for_test(h: &mut VaultHandle, title: &str) -> String {
        crate::commands::create_impl(h, "tiptap", Some(title), None)
            .unwrap()
            .note
            .note_id
    }

    #[test]
    fn test_sync_cycle_push_and_pull_mock_server() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server_url = format!("http://127.0.0.1:{}", port);

        // Spawn mock sync server for 1 push request and 1 pull request
        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut req_line = String::new();
                reader.read_line(&mut req_line).unwrap();

                // Consume headers
                let mut content_length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if header.to_lowercase().starts_with("content-length:") {
                        let parts: Vec<&str> = header.split(':').collect();
                        if parts.len() > 1 {
                            content_length = parts[1].trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }

                if req_line.starts_with("POST /sync/push") {
                    let mut body = vec![0u8; content_length];
                    use std::io::Read;
                    reader.read_exact(&mut body).unwrap();
                    let push_req: PushRequest = serde_json::from_slice(&body).unwrap();
                    let accepted: Vec<String> = push_req.ops.into_iter().map(|o| o.op_id).collect();

                    let push_resp = PushResponse {
                        status: Some("ok".into()),
                        accepted_op_ids: accepted,
                        rejected_op_ids: vec![],
                        server_cursor: Some("cur_1".into()),
                    };
                    let json = serde_json::to_string(&push_resp).unwrap();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        json.len(),
                        json
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                } else if req_line.starts_with("GET /sync/pull") {
                    let remote_op = Op {
                        op_id: "remote_op_1".into(),
                        device_id: "dev_remote".into(),
                        note_id: "note_cloud_1".into(),
                        kind: sync::OpKind::Create,
                        base_rev: 0,
                        new_rev: 0,
                        content: Some(
                            notium_core::envelope::note_from_text(
                                "# Nota desde la nube\n\nContenido sincronizado.\n",
                            )
                            .into_bytes(),
                        ),
                        hlc: Hlc::origin().tick(10),
                        ts: 1000,
                        path: None,
                        purge: false,
                        base_hash: None,
                    };
                    let pull_resp = PullResponse {
                        cursor: Some("cur_2".into()),
                        ops: vec![remote_op],
                        blobs: vec![],
                    };
                    let json = serde_json::to_string(&pull_resp).unwrap();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        json.len(),
                        json
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                }
            }
        });

        let (_d, mut h) = temp_vault();
        let local_op = sync::local_create_op(
            "dev_local",
            "note_local_1",
            b"# Local note".to_vec(),
            Hlc::origin().tick(1),
            100,
        );
        h.sync.append_journal(&local_op).unwrap();
        assert_eq!(h.sync.unpushed_count().unwrap(), 1);

        let cfg = SyncConfig {
            server_url,
            auth_token: "test_token".into(),
            device_id: "dev_local".into(),
            vault_id: "default".into(),
            enabled: true,
            auto_sync: true,
            interval_secs: 10,
            bound_to: String::new(),
        };

        let (shared, root) = share(h);
        let result = run_sync_cycle(&shared, &root, &cfg).unwrap();
        let guard = shared.lock().unwrap();
        let h = guard.as_ref().unwrap();
        assert_eq!(result.ops_pushed, 1);
        assert_eq!(result.ops_pulled, 1);
        assert_eq!(h.sync.unpushed_count().unwrap(), 0);

        // Check that the remote note was written and indexed!
        let note = h.index.get_note("note_cloud_1").unwrap();
        assert!(note.is_some());
        let rec = note.unwrap();
        assert_eq!(rec.title, "Nota desde la nube");
        assert!(h.vault.root.join(&rec.path).exists());
    }
}

#[cfg(test)]
#[path = "sync_audit_tests.rs"]
mod audit_tests;
