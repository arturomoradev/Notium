//! Filesystem vault layout + stable identity resolution (Workstream 1).
//!
//! The vault is a plain, open folder. All application state lives under `.app/`
//! and is derived/rebuildable. Every write is atomic (temp file + rename).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use crate::paths::{sanitize_folder_path, sanitize_title, title_from_path};

use serde::{Deserialize, Serialize};

use crate::extract::EditorKind;
use crate::frontmatter;
use crate::hash;

pub const NOTES_MD: &str = "notes/markdown";
pub const ATTACHMENTS: &str = "attachments";
pub const APP_DIR: &str = ".app";
pub const INDEX_DB: &str = "index.db";
pub const SYNC_DB: &str = "sync.db";
pub const SETTINGS: &str = "settings.json";
pub const TRASH: &str = "trash";
/// Individual retention period for trashed notes: 30 days in seconds.
pub const TRASH_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;
const TRASH_PATH_SEP: &str = "___";

fn encode_trash_rel_path(rel_path: &str) -> String {
    let normalized = rel_path.replace('\\', "/");
    let sub = normalized
        .strip_prefix("notes/markdown/")
        .unwrap_or(&normalized);
    let cleaned = sub.trim_matches('/');
    if cleaned.is_empty() {
        "note.md".to_string()
    } else {
        cleaned.replace('/', TRASH_PATH_SEP)
    }
}

fn decode_trash_rel_path(encoded: &str) -> (String, String) {
    let parts: Vec<&str> = encoded
        .split(TRASH_PATH_SEP)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();
    if parts.is_empty() {
        return (String::new(), "Nota restaurada.md".to_string());
    }
    let file_name = parts
        .last()
        .copied()
        .unwrap_or("Nota restaurada.md")
        .to_string();
    let folder = if parts.len() > 1 {
        parts[..parts.len() - 1].join("/")
    } else {
        String::new()
    };
    (folder, file_name)
}

/// Vault-level settings stored in `.app/settings.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSettings {
    /// When true, the `.md` editable mode shows raw Source Mode instead of Live Preview.
    #[serde(default)]
    pub source_mode: bool,
    /// Visual theme: `"dark"` (default), `"light"` or `"system"` (follow the device).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Editor font size in pixels.
    #[serde(default = "default_font_size")]
    pub font_size: u16,
    /// Resolution new image attachments are scaled down to: `"720p"`, `"1080p"` (default) or `"1440p"`.
    #[serde(default = "default_image_quality")]
    pub image_quality: String,
}

fn default_theme() -> String {
    "dark".to_string()
}

fn default_font_size() -> u16 {
    15
}

pub fn default_image_quality() -> String {
    "1080p".to_string()
}

impl Default for VaultSettings {
    fn default() -> Self {
        VaultSettings {
            source_mode: false,
            theme: default_theme(),
            font_size: default_font_size(),
            image_quality: default_image_quality(),
        }
    }
}

/// A note file discovered on disk.
#[derive(Clone, Debug)]
pub struct NoteEntry {
    /// Vault-relative path, `/`-separated.
    pub rel_path: String,
    pub editor: EditorKind,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub size: u64,
}

/// An attachment file discovered on disk.
#[derive(Clone, Debug)]
pub struct AttachmentEntry {
    pub rel_path: String,
    pub bytes: Vec<u8>,
    pub hash: String,
    pub size: u64,
}

/// An entry inside `.app/trash/`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashEntry {
    pub trash_id: String,
    pub note_id: Option<String>,
    pub title: String,
    pub deleted_at: u64,
    pub days_remaining: u32,
    pub size: u64,
}

/// A local filesystem vault.
#[derive(Clone, Debug)]
pub struct LocalVault {
    pub root: PathBuf,
}

impl LocalVault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalVault { root: root.into() }
    }

    /// Create the canonical directory structure + default settings if absent.
    pub fn init(&self) -> io::Result<()> {
        for dir in [NOTES_MD, ATTACHMENTS] {
            fs::create_dir_all(self.root.join(dir))?;
        }
        let app = self.root.join(APP_DIR);
        fs::create_dir_all(app.join(TRASH))?;
        let settings = app.join(SETTINGS);
        if !settings.exists() {
            self.write_settings(&VaultSettings::default())?;
        }
        let _ = self.purge_expired_trash();
        Ok(())
    }

    pub fn app_dir(&self) -> PathBuf {
        self.root.join(APP_DIR)
    }

    pub fn index_db_path(&self) -> PathBuf {
        self.app_dir().join(INDEX_DB)
    }

    pub fn sync_db_path(&self) -> PathBuf {
        self.app_dir().join(SYNC_DB)
    }

    pub fn read(&self, rel_path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.root.join(rel_path))
    }

    pub fn exists(&self, rel_path: &str) -> bool {
        self.root.join(rel_path).exists()
    }

    /// Atomically write bytes to a vault-relative path (temp file + rename).
    pub fn write_atomic(&self, rel_path: &str, bytes: &[u8]) -> io::Result<()> {
        atomic_write(&self.root.join(rel_path), bytes)
    }

    /// Like [`Self::write_atomic`] but without the per-file fsync. Only for bulk
    /// maintenance passes (e.g. identity injection across a whole vault during a
    /// rebuild), where thousands of fsyncs would dominate the run time.
    pub fn write_atomic_bulk(&self, rel_path: &str, bytes: &[u8]) -> io::Result<()> {
        atomic_write_with(&self.root.join(rel_path), bytes, false)
    }

    /// Move a file into `.app/trash/`, returning the trash path.
    pub fn delete_to_trash(&self, rel_path: &str) -> io::Result<PathBuf> {
        let src = self.root.join(rel_path);
        let encoded_rel = encode_trash_rel_path(rel_path);
        let trash_dir = self.app_dir().join(TRASH);
        fs::create_dir_all(&trash_dir)?;
        let ts = now_secs();
        let dest = trash_dir.join(format!(
            "{ts}_{}-{encoded_rel}",
            uuid::Uuid::now_v7().simple()
        ));
        fs::rename(&src, &dest)?;
        Ok(dest)
    }

    /// Permanently purge trashed notes whose individual age is >= 30 days.
    pub fn purge_expired_trash(&self) -> io::Result<usize> {
        self.purge_expired_trash_at(now_secs())
    }

    /// Permanently purge trashed notes whose individual `deleted_at` is at least
    /// [`TRASH_RETENTION_SECS`] older than `now`.
    pub fn purge_expired_trash_at(&self, now: u64) -> io::Result<usize> {
        let trash_dir = self.app_dir().join(TRASH);
        if !trash_dir.exists() {
            return Ok(0);
        }
        let mut purged = 0;
        for entry in fs::read_dir(&trash_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let trash_id = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let meta = entry.metadata()?;
            let deleted_at = parse_trash_deleted_at(&trash_id, &meta);
            if deleted_at > 0
                && now.saturating_sub(deleted_at) >= TRASH_RETENTION_SECS
                && fs::remove_file(&path).is_ok()
            {
                purged += 1;
            }
        }
        Ok(purged)
    }

    /// List all files currently inside `.app/trash/` (purging expired ones first), newest first.
    pub fn list_trash(&self) -> io::Result<Vec<TrashEntry>> {
        self.list_trash_at(now_secs())
    }

    /// List all files currently inside `.app/trash/` as of `now`, purging any item
    /// that has been in the trash for 30 days or more.
    pub fn list_trash_at(&self, now: u64) -> io::Result<Vec<TrashEntry>> {
        let _ = self.purge_expired_trash_at(now);
        let trash_dir = self.app_dir().join(TRASH);
        if !trash_dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&trash_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let trash_id = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let meta = entry.metadata()?;
            let size = meta.len();
            let deleted_at = parse_trash_deleted_at(&trash_id, &meta);
            let elapsed = now.saturating_sub(deleted_at);
            let remaining_secs = TRASH_RETENTION_SECS.saturating_sub(elapsed);
            let days_remaining = remaining_secs.div_ceil(86_400).clamp(1, 30) as u32;
            let bytes = fs::read(&path).unwrap_or_default();
            let note_id = extract_embedded_id(&bytes, EditorKind::Markdown);
            let orig_encoded = trash_id
                .split_once('-')
                .map(|(_, rest)| rest)
                .unwrap_or(&trash_id);
            let (_folder, orig_name) = decode_trash_rel_path(orig_encoded);
            let title = Path::new(&orig_name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| orig_name.to_string());
            out.push(TrashEntry {
                trash_id,
                note_id,
                title,
                deleted_at,
                days_remaining,
                size,
            });
        }
        out.sort_by(|a, b| {
            b.deleted_at
                .cmp(&a.deleted_at)
                .then_with(|| b.trash_id.cmp(&a.trash_id))
        });
        Ok(out)
    }

    /// Restore a file from `.app/trash/` back to its original subfolder under
    /// `notes/markdown/` (deduplicating the filename if needed) and return
    /// `(restored_rel_path, embedded_note_id)`.
    pub fn restore_from_trash(&self, trash_id: &str) -> io::Result<(String, Option<String>)> {
        if trash_id.contains("..") || trash_id.contains('/') || trash_id.contains('\\') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid trash id",
            ));
        }
        let src = self.app_dir().join(TRASH).join(trash_id);
        if !src.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "trash item not found",
            ));
        }
        let bytes = fs::read(&src)?;
        let note_id = extract_embedded_id(&bytes, EditorKind::Markdown);
        let orig_encoded = trash_id
            .split_once('-')
            .map(|(_, rest)| rest)
            .unwrap_or(trash_id);
        let (folder, orig_name) = decode_trash_rel_path(orig_encoded);
        let stem = Path::new(&orig_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Nota restaurada".to_string());
        let title = sanitize_title(&stem);
        let clean_folder = sanitize_folder_path(&folder).unwrap_or_default();
        let target_dir = if clean_folder.is_empty() {
            NOTES_MD.to_string()
        } else {
            format!("{NOTES_MD}/{clean_folder}")
        };
        let rel = self.dedup_path(&target_dir, &title, "md");
        let dst = self.root.join(&rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, &dst)?;
        Ok((rel, note_id))
    }

    /// Permanently delete a single item from `.app/trash/`.
    pub fn delete_trash_item(&self, trash_id: &str) -> io::Result<()> {
        if trash_id.contains("..") || trash_id.contains('/') || trash_id.contains('\\') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid trash id",
            ));
        }
        let p = self.app_dir().join(TRASH).join(trash_id);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    /// Permanently remove all files inside `.app/trash/`.
    pub fn empty_trash(&self) -> io::Result<()> {
        let trash_dir = self.app_dir().join(TRASH);
        if !trash_dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&trash_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    /// Remove a file entirely (no trash).
    pub fn remove_file(&self, rel_path: &str) -> io::Result<()> {
        let p = self.root.join(rel_path);
        if p.exists() {
            fs::remove_file(p)?;
        }
        Ok(())
    }

    /// List all `.md` notes.
    pub fn scan_notes(&self) -> io::Result<Vec<NoteEntry>> {
        let mut out = Vec::new();
        let abs = self.root.join(NOTES_MD);
        for path in walk_files(&abs)? {
            let rel = rel_path(&self.root, &path);
            let editor = match crate::extract::editor_from_path(&rel) {
                Some(e) => e,
                None => continue,
            };
            let bytes = fs::read(&path)?;
            let hash = hash::content_hash(&bytes);
            let size = bytes.len() as u64;
            out.push(NoteEntry {
                rel_path: rel,
                editor,
                bytes,
                hash,
                size,
            });
        }
        Ok(out)
    }

    /// List all attachment files.
    pub fn scan_attachments(&self) -> io::Result<Vec<AttachmentEntry>> {
        let mut out = Vec::new();
        let abs = self.root.join(ATTACHMENTS);
        for path in walk_files(&abs)? {
            let rel = rel_path(&self.root, &path);
            let bytes = fs::read(&path)?;
            let hash = hash::content_hash(&bytes);
            let size = bytes.len() as u64;
            out.push(AttachmentEntry {
                rel_path: rel,
                bytes,
                hash,
                size,
            });
        }
        Ok(out)
    }

    pub fn read_settings(&self) -> io::Result<VaultSettings> {
        let p = self.app_dir().join(SETTINGS);
        if p.exists() {
            let s = fs::read_to_string(p)?;
            Ok(serde_json::from_str(&s).unwrap_or_default())
        } else {
            Ok(VaultSettings::default())
        }
    }

    pub fn write_settings(&self, s: &VaultSettings) -> io::Result<()> {
        let p = self.app_dir().join(SETTINGS);
        atomic_write(&p, &serde_json::to_vec_pretty(s).unwrap())
    }
}

/// Atomically and durably write a file: write a temp sibling, fsync it, then
/// rename over the target.
pub fn atomic_write(abs: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_with(abs, bytes, true)
}

fn atomic_write_with(abs: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = abs
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let tmp = abs.with_file_name(format!(".{}.{}.tmp", name, uuid::Uuid::now_v7().simple()));
    // Flush the temp file to stable storage before the rename; otherwise a
    // crash or power loss can leave the renamed file empty or truncated.
    let written = (|| {
        let mut f = fs::File::create(&tmp)?;
        io::Write::write_all(&mut f, bytes)?;
        if durable {
            f.sync_all()?;
        }
        Ok(())
    })();
    match written.and_then(|()| fs::rename(&tmp, abs)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn walk_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(d)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn rel_path(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| abs.to_string_lossy().replace('\\', "/"))
}

/// Extract the embedded `notium_id` from file bytes.
pub fn extract_embedded_id(bytes: &[u8], editor: EditorKind) -> Option<String> {
    match editor {
        EditorKind::Markdown => {
            let text = std::str::from_utf8(bytes).ok()?;
            let (fm, _) = frontmatter::split(text);
            fm.as_deref().and_then(frontmatter::notium_id)
        }
    }
}

/// Inject an identity into file bytes, returning new bytes.
///
/// Surgical single-line frontmatter injection (byte-preserving).
pub fn inject_id(bytes: &[u8], editor: EditorKind, id: &str) -> Option<Vec<u8>> {
    match editor {
        EditorKind::Markdown => {
            let text = std::str::from_utf8(bytes).ok()?;
            Some(frontmatter::inject_notium_id(text, id).into_bytes())
        }
    }
}

/// Current Unix time in seconds.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Current Unix time in seconds, as a string (Phase 1 timestamp; ISO formatting
/// is a later polish).
pub fn now_ts() -> String {
    now_secs().to_string()
}

fn parse_trash_deleted_at(trash_id: &str, meta: &fs::Metadata) -> u64 {
    let head = trash_id.split('-').next().unwrap_or("");
    if let Some((ts_str, _uuid)) = head.split_once('_') {
        if let Ok(ts) = ts_str.parse::<u64>() {
            return ts;
        }
    }
    if head.len() == 32 {
        if let Ok(ms) = u64::from_str_radix(&head[..12], 16) {
            return ms / 1000;
        }
    }
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl LocalVault {
    /// Create a new note file (frontmatter/envelope + initial content) and return
    /// its vault-relative path. Deduplicates the filename if it already exists.
    pub fn create_note(&self, editor: EditorKind, title: &str, id: &str) -> io::Result<String> {
        self.create_note_in(editor, title, id, "")
    }

    /// Like [`create_note`] but inside an optional subfolder of the format root.
    pub fn create_note_in(
        &self,
        editor: EditorKind,
        title: &str,
        id: &str,
        folder: &str,
    ) -> io::Result<String> {
        let (dir, ext) = match editor {
            EditorKind::Markdown => (NOTES_MD, "md"),
        };
        let title = sanitize_title(title);
        let folder = sanitize_folder_path(folder)?;
        let base = if folder.is_empty() {
            dir.to_string()
        } else {
            format!("{dir}/{folder}")
        };
        let rel = self.dedup_path(&base, &title, ext);
        let ts = now_ts();
        let bytes = match editor {
            EditorKind::Markdown => {
                // No `# title` heading: the editor shows the file name as the title.
                format!("---\nnotium_id: {id}\ncreated: {ts}\nupdated: {ts}\n---\n")
                    .into_bytes()
            }
        };
        self.write_atomic(&rel, &bytes)?;
        Ok(rel)
    }

    /// Create a subfolder under the markdown note space and return its
    /// vault-relative path.
    pub fn create_folder(&self, name: &str) -> io::Result<String> {
        let folder = sanitize_folder_path(name)?;
        let folder = if folder.is_empty() {
            sanitize_title(name)
        } else {
            folder
        };
        let rel = format!("{NOTES_MD}/{folder}");
        fs::create_dir_all(self.root.join(&rel))?;
        Ok(rel)
    }

    /// Rename a subfolder under `notes/markdown` (either changing its leaf name
    /// or moving it to a new relative folder path) and return the new relative
    /// folder path (without the `notes/markdown/` prefix).
    pub fn rename_folder(&self, folder: &str, new_name: &str) -> io::Result<String> {
        let old_clean = sanitize_folder_path(folder)?;
        if old_clean.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot rename root folder",
            ));
        }
        let src_dir = self.root.join(NOTES_MD).join(&old_clean);
        if !src_dir.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "folder not found"));
        }

        let new_clean_input = sanitize_folder_path(new_name)?;
        if new_clean_input.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid new folder name",
            ));
        }
        // If `new_name` has no slashes, rename within the same parent folder;
        // otherwise treat it as a full folder path relative to `notes/markdown`.
        let new_clean = if !new_clean_input.contains('/') {
            match old_clean.rsplit_once('/') {
                Some((parent, _)) => format!("{parent}/{new_clean_input}"),
                None => new_clean_input,
            }
        } else {
            new_clean_input
        };

        if new_clean == old_clean {
            return Ok(old_clean);
        }
        if new_clean.starts_with(&format!("{old_clean}/")) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot move a folder inside itself",
            ));
        }

        let dst_dir = self.root.join(NOTES_MD).join(&new_clean);
        if dst_dir.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "destination folder already exists",
            ));
        }
        if let Some(parent) = dst_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src_dir, &dst_dir)?;
        Ok(new_clean)
    }

    /// Delete a subfolder under `notes/markdown`.
    ///
    /// - When `keep_content` is `true`, all notes inside `folder` (and its nested
    ///   subfolders) are moved into the parent folder (or root of `notes/markdown`),
    ///   deduplicating filenames if needed, before removing the folder directory.
    /// - When `keep_content` is `false`, all notes inside `folder` are moved to
    ///   `.app/trash/` (preserving their original paths for 30-day restore) and
    ///   their vault-relative paths are returned so the caller can tombstone them.
    pub fn delete_folder(&self, folder: &str, keep_content: bool) -> io::Result<Vec<String>> {
        let clean = sanitize_folder_path(folder)?;
        if clean.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot delete root folder",
            ));
        }
        let target_dir = self.root.join(NOTES_MD).join(&clean);
        if !target_dir.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "folder not found"));
        }
        let parent_folder = clean
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();

        let files = walk_files(&target_dir)?;
        let mut affected = Vec::new();
        for abs_file in files {
            let rel = rel_path(&self.root, &abs_file);
            if keep_content {
                let moved = self.move_note(&rel, &parent_folder)?;
                affected.push(moved);
            } else {
                self.delete_to_trash(&rel)?;
                affected.push(rel);
            }
        }
        fs::remove_dir_all(&target_dir)?;
        Ok(affected)
    }

    /// Rename a note (same folder, same extension) atomically; deduplicates.
    pub fn rename_note(&self, rel_path: &str, new_title: &str) -> io::Result<String> {
        let parent = Path::new(rel_path)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let ext = Path::new(rel_path)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let title = sanitize_title(new_title);
        let new_rel = self.dedup_path(&parent, &title, &ext);
        let src = self.root.join(rel_path);
        let dst = self.root.join(&new_rel);
        fs::rename(&src, &dst)?;
        Ok(new_rel)
    }

    /// Move a file to an exact vault-relative path, creating its folder.
    /// Used to apply a rename/move received from another device.
    pub fn move_to(&self, from: &str, to: &str) -> io::Result<()> {
        let dst = self.root.join(to);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(self.root.join(from), dst)
    }

    /// Move a note into a subfolder of `notes/markdown` (deduplicating the filename).
    pub fn move_note(&self, rel_path: &str, folder: &str) -> io::Result<String> {
        let dir = NOTES_MD;
        let title = title_from_path(rel_path);
        let ext = Path::new(rel_path)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let folder = sanitize_folder_path(folder)?;
        let base = if folder.is_empty() {
            dir.to_string()
        } else {
            format!("{dir}/{folder}")
        };
        let new_rel = self.dedup_path(&base, &title, &ext);
        if new_rel == rel_path {
            return Ok(new_rel);
        }
        let src = self.root.join(rel_path);
        let dst = self.root.join(&new_rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&src, &dst)?;
        Ok(new_rel)
    }

    /// Compute a collision-free `dir/<title>.ext` path.
    pub fn dedup_path(&self, dir: &str, title: &str, ext: &str) -> String {
        let candidate = if dir.is_empty() {
            format!("{title}.{ext}")
        } else {
            format!("{dir}/{title}.{ext}")
        };
        let mut rel = candidate;
        let mut n = 2;
        while self.root.join(&rel).exists() {
            rel = if dir.is_empty() {
                format!("{title} {n}.{ext}")
            } else {
                format!("{dir}/{title} {n}.{ext}")
            };
            n += 1;
        }
        rel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_layout_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        assert!(v.root.join(NOTES_MD).exists());
        assert!(v.root.join(ATTACHMENTS).exists());
        assert!(v.app_dir().join(SETTINGS).exists());
        assert!(!v.read_settings().unwrap().source_mode);
        assert_eq!(v.read_settings().unwrap().image_quality, "1080p");
    }

    #[test]
    fn settings_without_image_quality_load_with_default() {
        let s: VaultSettings = serde_json::from_str(r#"{"source_mode":true}"#).unwrap();
        assert_eq!(s.image_quality, "1080p");
    }

    #[test]
    fn atomic_write_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        v.write_atomic("notes/markdown/A.md", b"hello").unwrap();
        assert_eq!(v.read("notes/markdown/A.md").unwrap(), b"hello");
    }

    #[test]
    fn scan_detects_notes_and_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        v.write_atomic("notes/markdown/A.md", b"# A").unwrap();
        v.write_atomic("attachments/x.jpg", b"jpg").unwrap();
        let notes = v.scan_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(v.scan_attachments().unwrap().len(), 1);
        assert!(notes.iter().any(|n| n.editor == EditorKind::Markdown));
    }

    #[test]
    fn extract_and_inject_md_identity() {
        let md = b"---\ntitle: X\n---\nbody";
        assert_eq!(extract_embedded_id(md, EditorKind::Markdown), None);
        let injected = inject_id(md, EditorKind::Markdown, "note_1").unwrap();
        assert_eq!(
            extract_embedded_id(&injected, EditorKind::Markdown),
            Some("note_1".into())
        );
    }

    #[test]
    fn sanitize_title_strips_reserved_chars() {
        assert_eq!(sanitize_title("a<b>:c"), "abc");
        assert_eq!(sanitize_title("  "), "Sin título");
        assert_eq!(sanitize_title("ok."), "ok");
    }

    #[test]
    fn create_note_md() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        let md = v
            .create_note(EditorKind::Markdown, "Idea", "note_md")
            .unwrap();
        assert_eq!(md, "notes/markdown/Idea.md");
        let s = String::from_utf8(v.read(&md).unwrap()).unwrap();
        assert!(s.contains("notium_id: note_md"));
    }

    #[test]
    fn rename_note_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        v.create_note(EditorKind::Markdown, "A", "note_1").unwrap();
        v.create_note(EditorKind::Markdown, "B", "note_2").unwrap();
        let renamed = v.rename_note("notes/markdown/A.md", "B").unwrap();
        assert_eq!(renamed, "notes/markdown/B 2.md");
    }

    #[test]
    fn nested_folders_and_path_traversal_rejection() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();

        let created_folder = v.create_folder("Proyectos/2026").unwrap();
        assert_eq!(created_folder, "notes/markdown/Proyectos/2026");

        let note = v
            .create_note(EditorKind::Markdown, "Plan", "note_1")
            .unwrap();
        let moved = v.move_note(&note, "Proyectos/2026/Sub").unwrap();
        assert_eq!(moved, "notes/markdown/Proyectos/2026/Sub/Plan.md");
        assert!(v.root.join(&moved).exists());

        assert!(v.create_folder("../outside").is_err());
        assert!(v
            .create_note_in(EditorKind::Markdown, "X", "note_2", "../outside")
            .is_err());
        assert!(v.move_note(&moved, "../../escape").is_err());
    }

    #[test]
    fn trash_retains_individual_30_day_timer_and_purges_expired() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();

        let n1 = v
            .create_note(EditorKind::Markdown, "Reciente", "note_recent")
            .unwrap();
        v.delete_to_trash(&n1).unwrap();

        // Simulate an older trashed note deleted 31 days ago
        let now = now_secs();
        let old_ts = now - (31 * 24 * 60 * 60);
        let trash_dir = v.app_dir().join(TRASH);
        let old_file = trash_dir.join(format!(
            "{old_ts}_{}-Antigua.md",
            uuid::Uuid::now_v7().simple()
        ));
        fs::write(&old_file, b"# Antigua").unwrap();

        // Simulate a note deleted 10 days ago (20 days remaining)
        let mid_ts = now - (10 * 24 * 60 * 60);
        let mid_file = trash_dir.join(format!(
            "{mid_ts}_{}-Media.md",
            uuid::Uuid::now_v7().simple()
        ));
        fs::write(&mid_file, b"# Media").unwrap();

        let items = v.list_trash_at(now).unwrap();
        assert_eq!(items.len(), 2);
        assert!(
            !old_file.exists(),
            "31-day-old trashed note must be auto-purged"
        );
        let recent = items.iter().find(|i| i.title == "Reciente").unwrap();
        assert_eq!(recent.days_remaining, 30);
        let mid = items.iter().find(|i| i.title == "Media").unwrap();
        assert_eq!(mid.days_remaining, 20);
    }

    #[test]
    fn trash_restore_preserves_original_nested_folder() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();

        let rel = v
            .create_note_in(
                EditorKind::Markdown,
                "Plan Q4",
                "note_sub",
                "Proyectos/2026",
            )
            .unwrap();
        assert_eq!(rel, "notes/markdown/Proyectos/2026/Plan Q4.md");

        v.delete_to_trash(&rel).unwrap();
        let trash_items = v.list_trash().unwrap();
        assert_eq!(trash_items.len(), 1);
        assert_eq!(trash_items[0].title, "Plan Q4");

        let (restored_rel, restored_id) = v.restore_from_trash(&trash_items[0].trash_id).unwrap();
        assert_eq!(restored_rel, "notes/markdown/Proyectos/2026/Plan Q4.md");
        assert_eq!(restored_id, Some("note_sub".to_string()));
        assert!(v.root.join(&restored_rel).exists());
    }
}
