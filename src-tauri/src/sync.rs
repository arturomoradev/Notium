use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use notium_core::clock;
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
            device_id: format!("dev_{}", &uuid::Uuid::now_v7().to_string()[..13]),
            vault_id: default_vault_id(),
            enabled: false,
            auto_sync: true,
            interval_secs: 20,
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

    pub fn pull_blob(&self, hash: &str) -> Result<Vec<u8>, String> {
        let url = format!(
            "{}/sync/blob/{}?vault_id={}",
            self.server_url,
            urlencoding::encode(hash),
            urlencoding::encode(&self.vault_id)
        );
        let req = self.agent.get(&url);
        let req = self.apply_auth(req);

        let res = req
            .call()
            .map_err(|e| format_ureq_error("GET /sync/blob", e))?;

        let mut bytes = Vec::new();
        use std::io::Read;
        res.into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Error leyendo bytes de adjunto: {e}"))?;
        Ok(bytes)
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
    const SERVICE: &str = "dev.notium.app.sync";

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
    vault::atomic_write(&app_dir.join("sync_config.json"), &json)?;
    Ok(())
}

/// Read sync configuration from `<vault>/.app/sync_config.json`, resolving the
/// auth token from the OS credential store. A plaintext token left in the file
/// by an older version is migrated to the credential store on first load.
pub fn load_sync_config(vault_root: &Path) -> SyncConfig {
    let p = vault_root.join(".app").join("sync_config.json");
    if let Ok(bytes) = fs::read(&p) {
        if let Ok(mut cfg) = serde_json::from_slice::<SyncConfig>(&bytes) {
            if cfg.auth_token.is_empty() {
                cfg.auth_token = fetch_token(&cfg.device_id);
            } else if store_token(&cfg.device_id, &cfg.auth_token) {
                let _ = write_config_file(vault_root, &cfg, false);
            }
            return cfg;
        }
    }
    let def = SyncConfig::default();
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
    let rev = h
        .sync
        .get_current(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .map(|(r, _)| r)
        .unwrap_or(0);
    let op = sync::local_delete_op(
        &config.device_id,
        note_id,
        rev,
        clock::Hlc { ts: now, cnt: 0 },
        now,
    );
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
    let op = sync::local_purge_op(
        &config.device_id,
        note_id,
        rev,
        clock::Hlc { ts: now, cnt: 0 },
        now,
    );
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
    let rev = h
        .sync
        .get_current(note_id)
        .map_err(sql_err)?
        .map(|(r, _)| r)
        .unwrap_or(0);
    let hlc = h.sync.next_path_hlc(note_id, now).map_err(sql_err)?;
    let op = sync::local_update_op(&config.device_id, note_id, rev, bytes.clone(), hlc, now)
        .with_path(rel_path);
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

/// Folders ride the note journal as content-less ops on `folder:<path under
/// notes/markdown>`, so empty folders reach other devices without a server change.
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
    let prefix = format!("{}/", vault::NOTES_MD);
    let local: BTreeSet<String> = crate::commands::list_folders_impl(h)?
        .into_iter()
        .filter_map(|f| f.path.strip_prefix(&prefix).map(str::to_string))
        .collect();
    let known = synced_folders(h)?;
    if local == known {
        return Ok(());
    }
    let now = vault::now_secs();
    let mut cnt = 0;
    let mut stamp = || {
        cnt += 1;
        clock::Hlc { ts: now, cnt }
    };
    // Reverse order deletes children before their parents.
    let gone: Vec<&String> = known.difference(&local).collect();
    for path in gone.into_iter().rev() {
        let id = format!("{FOLDER_OP_PREFIX}{path}");
        let op = sync::local_delete_op(device_id, &id, 0, stamp(), now);
        h.sync.append_journal(&op).map_err(sql_err)?;
    }
    for path in local.difference(&known) {
        let id = format!("{FOLDER_OP_PREFIX}{path}");
        let mut op = sync::local_create_op(device_id, &id, Vec::new(), stamp(), now);
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
    let dir = h.vault.root.join(vault::NOTES_MD).join(&clean);
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

/// Apply pulled remote ops to the local vault. Returns
/// `(ops_pulled, conflicts, any_local_change)`.
fn apply_pulled_ops(
    h: &mut VaultHandle,
    ops: &[Op],
    device_id: &str,
) -> Result<(usize, usize, bool), CommandError> {
    let mut pulled = 0usize;
    let mut conflicts = 0usize;
    let mut any_local_change = false;
    for op in ops {
        // Skip operations that this device originated
        if op.device_id == device_id {
            continue;
        }

        if let Some(folder) = op.note_id.strip_prefix(FOLDER_OP_PREFIX) {
            if apply_folder_op(h, folder, op.kind == sync::OpKind::Delete)? {
                any_local_change = true;
                pulled += 1;
            }
            continue;
        }

        // Determine note path: existing note, the path the op sets, or one
        // derived from the content.
        let existing = h
            .index
            .get_note(&op.note_id)
            .map_err(|e| CommandError::Sql(e.to_string()))?;
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
            let mut candidate = h.vault.dedup_path("notes/markdown", &title, "md");
            let mut n = 2;
            while h
                .index
                .is_path_taken(&candidate, Some(&op.note_id))
                .unwrap_or(false)
            {
                candidate = h
                    .vault
                    .dedup_path("notes/markdown", &format!("{title} {n}"), "md");
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
                any_local_change = true;
                pulled += 1;
            }
            continue;
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
                if h.vault.exists(&target) {
                    let title = derive_note_title(&op.content, &op.note_id);
                    let dir = target.rsplit_once('/').map_or("notes/markdown", |(d, _)| d);
                    target = h.vault.dedup_path(dir, &title, "md");
                }
                h.vault.write_atomic(&target, content)?;
                h.sync.clear_tombstone(&op.note_id).map_err(sql_err)?;
                h.sync
                    .set_current(&op.note_id, op.new_rev, Some(content))
                    .map_err(sql_err)?;
                let _ = h.index.reconcile(&h.vault);
                any_local_change = true;
                pulled += 1;
                continue;
            }
        }

        let outcome = match sync::apply_remote(&mut h.sync, &h.vault, op, "markdown", &rel_path) {
            Ok(o) => o,
            Err(e) => {
                log::warn!("sync: error applying remote op {}: {}", op.op_id, e);
                conflicts += 1;
                continue;
            }
        };

        match outcome {
            ApplyOutcome::Created | ApplyOutcome::FastForward => {
                if let Ok(bytes) = h.vault.read(&rel_path) {
                    let _ =
                        h.index
                            .reindex_note(&op.note_id, &rel_path, EditorKind::Markdown, &bytes);
                    any_local_change = true;
                    pulled += 1;
                }
            }
            ApplyOutcome::Merged { conflict } => {
                if let Ok(bytes) = h.vault.read(&rel_path) {
                    let _ =
                        h.index
                            .reindex_note(&op.note_id, &rel_path, EditorKind::Markdown, &bytes);
                    any_local_change = true;
                    pulled += 1;
                    if conflict {
                        conflicts += 1;
                    }
                }
            }
            ApplyOutcome::Deleted => {
                let _ = h.index.mark_tombstoned(&op.note_id);
                any_local_change = true;
                pulled += 1;
            }
            ApplyOutcome::DeleteConflict => {
                conflicts += 1;
            }
            ApplyOutcome::Noop | ApplyOutcome::RejectedTombstone => {}
        }

        // A rename/move from another device: move the file to the new path.
        if let (Some(rec), Some(target)) = (&existing, wanted_path) {
            if target != rec.path && h.vault.exists(&rec.path) {
                match h.vault.move_to(&rec.path, target) {
                    Ok(()) => {
                        let _ = h.index.reconcile(&h.vault);
                        any_local_change = true;
                    }
                    Err(e) => log::warn!("sync: could not move {} to {target}: {e}", rec.path),
                }
            }
        }
    }
    Ok((pulled, conflicts, any_local_change))
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
                rec.path == path || !h.vault.exists(path)
            })
}

/// Execute a complete sync cycle (Push mutations -> Pull remote -> Sync blobs).
///
/// The vault lock is taken only for the short local steps (`with_vault`); every
/// HTTP request runs unlocked so IPC commands such as `note_save` are never
/// blocked behind a slow or unreachable server.
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
        heal_tombstoned_deletes(h)?;
        journal_folder_changes(h, &config.device_id)?;
        h.sync.get_unpushed_ops().map_err(sql_err)
    })?;
    const CHUNK_SIZE: usize = 50;
    for chunk in unpushed.chunks(CHUNK_SIZE) {
        let push_res = client.push(chunk).map_err(CommandError::Io)?;
        let accepted: Vec<String> = if !push_res.accepted_op_ids.is_empty() {
            push_res.accepted_op_ids
        } else {
            chunk.iter().map(|o| o.op_id.clone()).collect()
        };
        with_vault(shared, root, |h| {
            let ids: Vec<&str> = accepted.iter().map(|s| s.as_str()).collect();
            h.sync.mark_ops_synced(&ids).map_err(sql_err)
        })?;
        report.ops_pushed += accepted.len();
    }

    // 2. Pull remote mutations and apply them locally.
    let last_cursor = with_vault(shared, root, |h| {
        h.sync.get_meta("last_pull_cursor").map_err(sql_err)
    })?;
    let pull_res = client
        .pull(last_cursor.as_deref())
        .map_err(CommandError::Io)?;

    let (pulled, conflicts, mut any_local_change) = with_vault(shared, root, |h| {
        apply_pulled_ops(h, &pull_res.ops, &config.device_id)
    })?;
    report.ops_pulled += pulled;
    report.conflicts += conflicts;

    // 3. Sync attachments
    // 3a. Download missing or updated remote blobs from D1
    let to_download: Vec<BlobRef> = with_vault(shared, root, |h| {
        let mut out = Vec::new();
        for blob in &pull_res.blobs {
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

    let mut blob_download_failed = false;
    for blob in &to_download {
        match client.pull_blob(&blob.hash) {
            Ok(bytes) => {
                if (bytes.len() as u64) <= MAX_D1_BLOB_BYTES
                    && notium_core::hash::content_hash(&bytes) == blob.hash
                {
                    let written = with_vault(shared, root, |h| {
                        if h.vault.write_atomic(&blob.path, &bytes).is_ok() {
                            let _ = h.sync.mark_blob_synced(&blob.hash, &blob.path);
                            Ok(true)
                        } else {
                            Ok(false)
                        }
                    })?;
                    if written {
                        report.blobs_synced += 1;
                        any_local_change = true;
                    }
                }
            }
            Err(_) => {
                blob_download_failed = true;
            }
        }
    }

    if !blob_download_failed {
        if let Some(new_cursor) = pull_res.cursor.as_deref() {
            if !new_cursor.is_empty() {
                with_vault(shared, root, |h| {
                    let _ = h.sync.set_meta("last_pull_cursor", new_cursor);
                    Ok(())
                })?;
            }
        }
    }

    // 3b. Upload local attachments if within D1 900 KB limit and not yet in synced_blobs
    let to_upload = with_vault(shared, root, |h| {
        let known_server_hashes: std::collections::HashSet<&str> =
            pull_res.blobs.iter().map(|b| b.hash.as_str()).collect();
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
                if known_server_hashes.contains(att.hash.as_str()) {
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

    Ok(report)
}

fn derive_note_title(content: &Option<Vec<u8>>, note_id: &str) -> String {
    if let Some(bytes) = content {
        if let Ok(extracted) = notium_core::extract::extract(EditorKind::Markdown, note_id, bytes) {
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

fn sync_worker_loop(
    receiver: Receiver<SyncTrigger>,
    state: Arc<Mutex<InnerSyncState>>,
    active_vault: Arc<Mutex<Option<VaultHandle>>>,
    app: AppHandle,
) {
    let mut last_run = Instant::now() - Duration::from_secs(120);

    while let Ok(trigger) = receiver.recv() {
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
                // Check if enough time elapsed since last sync cycle
                if last_run.elapsed() < Duration::from_secs(15) {
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
            }
            Err(e) => {
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
            content: Some(format!("---\nnotium_id: {note_id}\n---\n\n# Nota\n").into_bytes()),
            hlc: Hlc { ts, cnt: 0 },
            ts,
            path: path.map(str::to_string),
            purge: false,
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
        let sub = b.vault.root.join("notes/markdown/Vacía/Sub");
        assert!(sub.is_dir());
        // B received them, so it does not announce them back.
        journal_folder_changes(&mut b, "dev_b").unwrap();
        assert!(b.sync.get_unpushed_ops().unwrap().is_empty());

        // A deletes the tree; B keeps "Vacía" while it still holds a note.
        a.sync
            .mark_ops_synced(&ops.iter().map(|o| o.op_id.as_str()).collect::<Vec<_>>())
            .unwrap();
        fs::remove_dir_all(a.vault.root.join("notes/markdown/Vacía")).unwrap();
        journal_folder_changes(&mut a, "dev_a").unwrap();
        let deletes = a.sync.get_unpushed_ops().unwrap();
        assert_eq!(deletes[0].note_id, "folder:Vacía/Sub", "children go first");
        fs::write(b.vault.root.join("notes/markdown/Vacía/n.md"), "x").unwrap();
        apply_pulled_ops(&mut b, &deletes, "dev_b").unwrap();
        assert!(!sub.exists());
        assert!(b.vault.root.join("notes/markdown/Vacía").is_dir());
    }

    #[test]
    fn remote_delete_restore_and_purge_round_trip() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/A.md"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();

        // Deleted elsewhere: lands in our trash.
        apply_pulled_ops(&mut h, &[remote_delete("n1", 0, 20, false)], "dev_local").unwrap();
        assert!(!h.vault.exists("notes/markdown/A.md"));
        assert_eq!(trashed(&h, "n1"), 1);

        // A stale edit (base below the tombstone) does not bring it back.
        let stale = remote("n1", sync::OpKind::Update, 1, 25, None);
        apply_pulled_ops(&mut h, &[stale], "dev_local").unwrap();
        assert!(!h.vault.exists("notes/markdown/A.md"));

        // Restored elsewhere (built on the tombstone rev 1): back, trash empty.
        let mut restore = remote(
            "n1",
            sync::OpKind::Update,
            2,
            30,
            Some("notes/markdown/A.md"),
        );
        restore.base_rev = 1;
        apply_pulled_ops(&mut h, &[restore], "dev_local").unwrap();
        assert!(h.vault.exists("notes/markdown/A.md"));
        assert!(!h.index.get_note("n1").unwrap().unwrap().tombstoned);
        assert_eq!(trashed(&h, "n1"), 0);

        // Deleted again, then purged elsewhere: gone from our trash too.
        apply_pulled_ops(&mut h, &[remote_delete("n1", 2, 40, false)], "dev_local").unwrap();
        assert_eq!(trashed(&h, "n1"), 1);
        apply_pulled_ops(&mut h, &[remote_delete("n1", 3, 50, true)], "dev_local").unwrap();
        assert_eq!(trashed(&h, "n1"), 0);
    }

    #[test]
    fn purge_never_touches_a_live_note() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/A.md"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        apply_pulled_ops(&mut h, &[remote_delete("n1", 5, 20, true)], "dev_local").unwrap();
        assert!(h.vault.exists("notes/markdown/A.md"));
    }

    #[test]
    fn local_delete_and_restore_journal_ops_other_devices_accept() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/A.md"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        h.vault.delete_to_trash("notes/markdown/A.md").unwrap();
        h.index.mark_tombstoned("n1").unwrap();
        journal_local_delete(&mut h, "n1", 100).unwrap();
        let del = h.sync.get_unpushed_ops().unwrap().pop().unwrap();
        assert_eq!((del.base_rev, del.new_rev), (0, 1));
        assert_eq!(h.sync.get_current("n1").unwrap().unwrap().0, 1);

        let trash_id = h.vault.list_trash().unwrap()[0].trash_id.clone();
        h.vault.restore_from_trash(&trash_id).unwrap();
        h.sync.clear_tombstone("n1").unwrap();
        h.index.reconcile(&h.vault).unwrap();
        journal_local_path(&mut h, "n1", "notes/markdown/A.md").unwrap();
        let restore = h.sync.get_unpushed_ops().unwrap().pop().unwrap();
        // Built on the tombstone rev, so peers treat it as a restore.
        assert_eq!((restore.base_rev, restore.new_rev), (1, 2));
        assert!(restore.content.is_some());
        assert_eq!(restore.path.as_deref(), Some("notes/markdown/A.md"));
    }

    #[test]
    fn remote_create_uses_op_path_and_rename_moves_the_file() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/Dir/Plan.md"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/markdown/Dir/Plan.md");

        let rename = remote(
            "n1",
            sync::OpKind::Update,
            1,
            20,
            Some("notes/markdown/Plan final.md"),
        );
        apply_pulled_ops(&mut h, &[rename], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/markdown/Plan final.md");
        assert!(h.vault.exists("notes/markdown/Plan final.md"));
        assert!(!h.vault.exists("notes/markdown/Dir/Plan.md"));

        // An older path change arriving late does not move it back.
        let stale = remote(
            "n1",
            sync::OpKind::Update,
            2,
            15,
            Some("notes/markdown/Viejo.md"),
        );
        apply_pulled_ops(&mut h, &[stale], "dev_local").unwrap();
        assert_eq!(path_of(&h, "n1"), "notes/markdown/Plan final.md");
    }

    #[test]
    fn remote_path_is_ignored_when_unsafe_or_taken() {
        let (_d, mut h) = temp_vault();
        let a = remote(
            "a",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/A.md"),
        );
        let b = remote(
            "b",
            sync::OpKind::Create,
            0,
            11,
            Some("notes/markdown/B.md"),
        );
        apply_pulled_ops(&mut h, &[a, b], "dev_local").unwrap();

        let clash = remote(
            "b",
            sync::OpKind::Update,
            1,
            20,
            Some("notes/markdown/A.md"),
        );
        let escape = remote(
            "a",
            sync::OpKind::Update,
            1,
            21,
            Some("notes/markdown/../../x.md"),
        );
        apply_pulled_ops(&mut h, &[clash, escape], "dev_local").unwrap();
        assert_eq!(path_of(&h, "a"), "notes/markdown/A.md");
        assert_eq!(path_of(&h, "b"), "notes/markdown/B.md");
    }

    #[test]
    fn local_rename_journals_an_op_with_the_new_path() {
        let (_d, mut h) = temp_vault();
        let create = remote(
            "n1",
            sync::OpKind::Create,
            0,
            10,
            Some("notes/markdown/Uno.md"),
        );
        apply_pulled_ops(&mut h, &[create], "dev_local").unwrap();
        h.vault
            .move_to("notes/markdown/Uno.md", "notes/markdown/Dos.md")
            .unwrap();
        h.index.reconcile(&h.vault).unwrap();
        journal_local_path(&mut h, "n1", "notes/markdown/Dos.md").unwrap();
        let ops = h.sync.get_unpushed_ops().unwrap();
        let op = ops.last().unwrap();
        assert_eq!(op.path.as_deref(), Some("notes/markdown/Dos.md"));
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
            .write_atomic("notes/markdown/Foo.md", b"# Foo\n\nbody\n")
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
                            b"# Nota desde la nube\n\nContenido sincronizado.\n".to_vec(),
                        ),
                        hlc: Hlc::origin().tick(10),
                        ts: 1000,
                        path: None,
                        purge: false,
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
