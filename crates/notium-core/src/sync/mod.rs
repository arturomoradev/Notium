//! Sync engine: op journal, per-note revisions (merge bases), tombstones, and
//! remote-op application with per-format merge rules (ADR-06/08).

pub mod merge;

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::{self, Hlc};
use crate::frontmatter;
use crate::hash;
use crate::vault::LocalVault;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    #[serde(alias = "Create", alias = "create")]
    Create,
    #[serde(alias = "Update", alias = "update")]
    Update,
    #[serde(alias = "Delete", alias = "delete")]
    Delete,
}

impl OpKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OpKind::Create => "create",
            OpKind::Update => "update",
            OpKind::Delete => "delete",
        }
    }
}

/// A single syncable mutation of a note's content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Op {
    pub op_id: String,
    pub device_id: String,
    pub note_id: String,
    pub kind: OpKind,
    pub base_rev: u64,
    pub new_rev: u64,
    /// New content bytes (None for delete).
    pub content: Option<Vec<u8>>,
    pub hlc: Hlc,
    pub ts: u64,
    /// Vault-relative path set by this op (create, rename, move). `None` keeps
    /// the current path; older peers ignore the field and older ops lack it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// On a delete: also remove the note from every trash (permanent delete).
    /// Older peers see a plain delete of an already deleted note.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub purge: bool,
    /// Hash of the content this op was built on: identifies the merge base
    /// across devices, where `base_rev` numbers can collide. Absent on ops
    /// from older peers, which fall back to `base_rev`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hash: Option<String>,
    /// Hash of a content too big to travel inline: it is uploaded to the
    /// server (`/sync/content`) and `content` stays empty on the wire. Only
    /// set between push and pull; the journal keeps the content itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
}

impl Op {
    /// Attach the note path this op sets.
    pub fn with_path(mut self, path: impl Into<String>) -> Op {
        self.path = Some(path.into());
        self
    }

    /// Name the content this op was built on (`hash` of the current content;
    /// an empty hash, a deleted note, names none).
    pub fn with_base(mut self, hash: &str) -> Op {
        self.base_hash = (!hash.is_empty()).then(|| hash.to_string());
        self
    }
}

/// Total order of ops (path changes, concurrent contents): the HLC, then the
/// device id as tie-break.
pub fn op_stamp(op: &Op) -> String {
    stamp_of(&op.hlc, &op.device_id)
}

fn stamp_of(hlc: &Hlc, device_id: &str) -> String {
    // Zero-padded so the string order matches the numeric order.
    format!("{:020}:{:010}:{}", hlc.ts, hlc.cnt, device_id)
}

/// Parse the `ts:cnt` prefix of a stamp or a stored clock.
fn parse_hlc(s: &str) -> Option<Hlc> {
    let mut it = s.splitn(3, ':');
    Some(Hlc {
        ts: it.next()?.parse().ok()?,
        cnt: it.next()?.parse().ok()?,
    })
}

/// Remote clocks further ahead than this are not adopted (see `observe_hlc`).
pub const MAX_CLOCK_SKEW_SECS: u64 = 24 * 60 * 60;

/// How long a past note content is kept as a possible merge base (see
/// `SyncDb::maybe_prune`): edits made offline for longer than this conflict
/// into a copy instead of merging line by line.
pub const CONTENT_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

/// Outcome of applying a remote op to a device.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyOutcome {
    /// Note did not exist locally; created from the op.
    Created,
    /// Op base rev matched local rev; content overwritten.
    FastForward,
    /// Concurrent edits merged (3-way for markdown).
    Merged { conflict: bool },
    /// Deletion applied (tombstone recorded, file trashed).
    Deleted,
    /// Delete arrived concurrently with a local edit; file kept, flagged.
    DeleteConflict,
    /// Stale update against a tombstoned note (resurrection blocked).
    RejectedTombstone,
    /// Remote rev is behind local rev and content is identical.
    Noop,
}

/// Largest attachment synced (stored in R2 by the server).
pub const MAX_BLOB_BYTES: u64 = 15_000_000;
/// Largest note content synced.
pub const MAX_NOTE_BYTES: usize = 5_000_000;
/// Note contents above this travel through R2 (`Op::content_ref`) instead of
/// inline in the op, which D1 caps at 2 MB and JSON inflates about 3.5×.
pub const INLINE_CONTENT_MAX: usize = 1_000_000;

/// Normalize a vault partition identifier (`[a-z0-9_-]{1,64}`), defaulting to `"default"`.
pub fn normalize_vault_id(raw: &str) -> String {
    let cleaned = raw.trim().to_ascii_lowercase();
    if cleaned.is_empty() || cleaned.len() > 64 {
        return "default".to_string();
    }
    if cleaned
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        cleaned
    } else {
        "default".to_string()
    }
}

/// Validate that an attachment relative path points inside `attachments/` without traversal.
pub fn is_safe_attachment_rel_path(path: &str) -> bool {
    if !path.starts_with("attachments/") {
        return false;
    }
    if path.contains("..") || path.contains('\\') || path.starts_with('/') {
        return false;
    }
    path.len() > "attachments/".len()
}

const SYNC_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS revisions (
  note_id TEXT NOT NULL,
  rev INTEGER NOT NULL,
  content BLOB,
  hash TEXT NOT NULL,
  PRIMARY KEY (note_id, rev)
);
CREATE TABLE IF NOT EXISTS current (
  note_id TEXT PRIMARY KEY,
  rev INTEGER NOT NULL,
  hash TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS journal (
  op_id TEXT PRIMARY KEY,
  device_id TEXT NOT NULL,
  note_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_rev INTEGER NOT NULL,
  new_rev INTEGER NOT NULL,
  content BLOB,
  hlc_ts INTEGER NOT NULL,
  hlc_cnt INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  synced INTEGER NOT NULL DEFAULT 0,
  path TEXT,
  purge INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tombstones (
  note_id TEXT PRIMARY KEY,
  ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS synced_blobs (
  hash TEXT NOT NULL,
  path TEXT NOT NULL,
  PRIMARY KEY (hash, path)
);
-- Attachment images already compressed by the app (or that came from another
-- device, which compressed them): the rest wait for compression before upload.
CREATE TABLE IF NOT EXISTS checked_images (
  hash TEXT PRIMARY KEY
);
-- Every note content seen (local saves, remote ops, merges), by hash: merge
-- bases are found by hash, and `stamp` orders concurrent versions.
CREATE TABLE IF NOT EXISTS contents (
  note_id TEXT NOT NULL,
  hash TEXT NOT NULL,
  content BLOB NOT NULL,
  stamp TEXT NOT NULL DEFAULT '',
  seen_at INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (note_id, hash)
);
-- Remote ops already applied: re-delivered ops are skipped.
CREATE TABLE IF NOT EXISTS applied_ops (
  op_id TEXT PRIMARY KEY
);
-- Remote ops that failed to apply (e.g. a locked file), retried next cycle in order.
CREATE TABLE IF NOT EXISTS deferred_ops (
  op_id TEXT PRIMARY KEY,
  note_id TEXT NOT NULL,
  op TEXT NOT NULL
);
"#;

/// Per-device sync state backed by `.app/sync.db`.
pub struct SyncDb {
    conn: Connection,
}

impl SyncDb {
    pub fn open(path: &Path) -> rusqlite::Result<SyncDb> {
        let conn = Connection::open(path)?;
        crate::index::configure_connection(&conn)?;
        Self::migrate(conn)
    }

    pub fn open_in_memory() -> rusqlite::Result<SyncDb> {
        Self::migrate(Connection::open_in_memory()?)
    }

    fn migrate(conn: Connection) -> rusqlite::Result<SyncDb> {
        conn.execute_batch(SYNC_SCHEMA)?;
        // Columns added after the first release; "duplicate column" errors are expected.
        for col in [
            "synced INTEGER NOT NULL DEFAULT 0",
            "path TEXT",
            "purge INTEGER NOT NULL DEFAULT 0",
            "base_hash TEXT",
        ] {
            let _ = conn.execute(&format!("ALTER TABLE journal ADD COLUMN {col}"), []);
        }
        let _ = conn.execute(
            "ALTER TABLE contents ADD COLUMN seen_at INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // Attachments synced before compression existed count as compressed.
        conn.execute(
            "INSERT OR IGNORE INTO checked_images (hash) SELECT hash FROM synced_blobs",
            [],
        )?;
        // Rows from before `seen_at` start their retention now.
        conn.execute(
            "UPDATE contents SET seen_at = unixepoch() WHERE seen_at = 0",
            [],
        )?;
        Ok(SyncDb { conn })
    }

    pub fn get_current(&self, note_id: &str) -> rusqlite::Result<Option<(u64, String)>> {
        self.conn
            .query_row(
                "SELECT rev, hash FROM current WHERE note_id = ?1",
                params![note_id],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get(1)?)),
            )
            .optional()
    }

    /// Content of `note_id` at this device's `rev` (legacy merge base for ops
    /// without `base_hash`). Older rows keep their own copy of the content.
    pub fn get_revision(&self, note_id: &str, rev: u64) -> rusqlite::Result<Option<Vec<u8>>> {
        self.conn
            .query_row(
                "SELECT COALESCE(r.content, c.content) FROM revisions r
                 LEFT JOIN contents c ON c.note_id = r.note_id AND c.hash = r.hash
                 WHERE r.note_id = ?1 AND r.rev = ?2",
                params![note_id, rev as i64],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()
            .map(|o| o.flatten())
    }

    /// A content of `note_id` seen before, by hash.
    pub fn content_by_hash(&self, note_id: &str, hash: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        self.conn
            .query_row(
                "SELECT content FROM contents WHERE note_id = ?1 AND hash = ?2",
                params![note_id, hash],
                |r| r.get(0),
            )
            .optional()
    }

    /// Stamp (see `op_stamp`) of the newest op known to have produced this content;
    /// empty when unknown.
    pub fn content_stamp(&self, note_id: &str, hash: &str) -> rusqlite::Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT stamp FROM contents WHERE note_id = ?1 AND hash = ?2",
                params![note_id, hash],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Remember a content of `note_id`, keeping the highest stamp seen for it.
    pub fn remember_content(
        &mut self,
        note_id: &str,
        content: &[u8],
        stamp: &str,
    ) -> rusqlite::Result<String> {
        let h = hash::content_hash(content);
        self.conn.execute(
            "INSERT INTO contents (note_id, hash, content, stamp, seen_at)
             VALUES (?1,?2,?3,?4,unixepoch())
             ON CONFLICT (note_id, hash)
             DO UPDATE SET stamp = max(stamp, excluded.stamp), seen_at = excluded.seen_at",
            params![note_id, h, content, stamp],
        )?;
        Ok(h)
    }

    /// Record a revision snapshot (the content at `rev`) and update the current pointer.
    pub fn set_current(
        &mut self,
        note_id: &str,
        rev: u64,
        content: Option<&[u8]>,
    ) -> rusqlite::Result<()> {
        let h = match content {
            Some(c) => self.remember_content(note_id, c, "")?,
            None => String::new(),
        };
        self.conn.execute(
            "INSERT OR REPLACE INTO revisions (note_id, rev, content, hash) VALUES (?1,?2,NULL,?3)",
            params![note_id, rev as i64, h],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO current (note_id, rev, hash) VALUES (?1,?2,?3)",
            params![note_id, rev as i64, h],
        )?;
        Ok(())
    }

    pub fn append_journal(&mut self, op: &Op) -> rusqlite::Result<()> {
        if let Some(content) = &op.content {
            self.remember_content(&op.note_id, content, &op_stamp(op))?;
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO journal (op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge, base_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                op.op_id,
                op.device_id,
                op.note_id,
                op.kind.as_str(),
                op.base_rev as i64,
                op.new_rev as i64,
                op.content,
                op.hlc.ts as i64,
                op.hlc.cnt as i64,
                op.ts as i64,
                op.path,
                op.purge as i64,
                op.base_hash
            ],
        )?;
        Ok(())
    }

    /// Whether `op_id` was journaled here or already applied: such an op
    /// arriving from the server is skipped (re-delivery, or our own op after
    /// the device id changed).
    pub fn is_known_op(&self, op_id: &str) -> rusqlite::Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM journal WHERE op_id = ?1)
                 OR EXISTS(SELECT 1 FROM applied_ops WHERE op_id = ?1)",
            params![op_id],
            |r| r.get(0),
        )
    }

    pub fn mark_applied(&mut self, op_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO applied_ops (op_id) VALUES (?1)",
            params![op_id],
        )?;
        Ok(())
    }

    /// Queue a remote op that could not be applied, to retry it next cycle.
    pub fn defer_op(&mut self, op: &Op) -> rusqlite::Result<()> {
        let json = serde_json::to_string(op).map_err(rusqlite_err)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO deferred_ops (op_id, note_id, op) VALUES (?1,?2,?3)",
            params![op.op_id, op.note_id, json],
        )?;
        Ok(())
    }

    /// Take every deferred op out of the queue, oldest first.
    pub fn take_deferred_ops(&mut self) -> rusqlite::Result<Vec<Op>> {
        let ops: Vec<String> = self
            .conn
            .prepare("SELECT op FROM deferred_ops ORDER BY rowid")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        self.conn.execute("DELETE FROM deferred_ops", [])?;
        Ok(ops
            .iter()
            .filter_map(|j| serde_json::from_str(j).ok())
            .collect())
    }

    pub fn deferred_count(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM deferred_ops", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
    }

    /// Stamp of the newest local op with content for `note_id` (fallback order
    /// for contents recorded before stamps existed).
    pub fn latest_local_stamp(&self, note_id: &str) -> rusqlite::Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT hlc_ts, hlc_cnt, device_id FROM journal
                 WHERE note_id = ?1 AND kind <> 'delete' ORDER BY rowid DESC LIMIT 1",
                params![note_id],
                |r| {
                    Ok(stamp_of(
                        &Hlc {
                            ts: r.get::<_, i64>(0)? as u64,
                            cnt: r.get::<_, i64>(1)? as u32,
                        },
                        &r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// This device's clock for a new local op at wall time `now` (Unix
    /// seconds): never behind any op seen, so a device whose clock is late
    /// still orders its later edits after what it already received.
    pub fn tick_hlc(&mut self, now: u64) -> rusqlite::Result<Hlc> {
        let next = self.stored_hlc()?.tick(now);
        self.store_hlc(next)?;
        Ok(next)
    }

    /// Advance the clock past a remote op's HLC. Stamps more than a day in the
    /// future (a device with a wrong clock) are ignored so they cannot drag
    /// every clock along.
    pub fn observe_hlc(&mut self, remote: Hlc, now: u64) -> rusqlite::Result<()> {
        let cur = self.stored_hlc()?;
        if remote.ts <= now + MAX_CLOCK_SKEW_SECS && remote.key() > cur.key() {
            self.store_hlc(remote)?;
        }
        Ok(())
    }

    fn stored_hlc(&self) -> rusqlite::Result<Hlc> {
        Ok(self
            .get_meta("hlc")?
            .and_then(|s| parse_hlc(&s))
            .unwrap_or_else(Hlc::origin))
    }

    fn store_hlc(&mut self, hlc: Hlc) -> rusqlite::Result<()> {
        self.set_meta("hlc", &format!("{}:{}", hlc.ts, hlc.cnt))
    }

    pub fn tombstone(&mut self, note_id: &str, ts: u64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO tombstones (note_id, ts) VALUES (?1,?2)",
            params![note_id, ts as i64],
        )?;
        Ok(())
    }

    pub fn is_tombstoned(&self, note_id: &str) -> rusqlite::Result<bool> {
        let n = self.conn.query_row(
            "SELECT COUNT(*) FROM tombstones WHERE note_id = ?1",
            params![note_id],
            |r| r.get::<_, i64>(0),
        )?;
        Ok(n > 0)
    }

    /// True if the journal already contains a delete op for `note_id`.
    pub fn has_delete_op(&self, note_id: &str) -> rusqlite::Result<bool> {
        let n = self.conn.query_row(
            "SELECT COUNT(*) FROM journal WHERE note_id = ?1 AND kind = 'delete'",
            params![note_id],
            |r| r.get::<_, i64>(0),
        )?;
        Ok(n > 0)
    }

    pub fn clear_tombstone(&mut self, note_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM tombstones WHERE note_id = ?1",
            params![note_id],
        )?;
        Ok(())
    }

    pub fn get_unpushed_ops(&self) -> rusqlite::Result<Vec<Op>> {
        // Journal order: two ops stamped in the same second keep the order they happened in.
        let mut stmt = self.conn.prepare(
            "SELECT op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge, base_hash
             FROM journal WHERE synced = 0 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            let kind_str: String = r.get(3)?;
            let kind = match kind_str.as_str() {
                "create" => OpKind::Create,
                "delete" => OpKind::Delete,
                _ => OpKind::Update,
            };
            Ok(Op {
                op_id: r.get(0)?,
                device_id: r.get(1)?,
                note_id: r.get(2)?,
                kind,
                base_rev: r.get::<_, i64>(4)? as u64,
                new_rev: r.get::<_, i64>(5)? as u64,
                content: r.get(6)?,
                hlc: Hlc {
                    ts: r.get::<_, i64>(7)? as u64,
                    cnt: r.get::<_, i64>(8)? as u32,
                },
                ts: r.get::<_, i64>(9)? as u64,
                path: r.get(10)?,
                purge: r.get::<_, i64>(11)? != 0,
                base_hash: r.get(12)?,
                content_ref: None,
            })
        })?;
        let mut ops = Vec::new();
        for r in rows {
            ops.push(r?);
        }
        Ok(ops)
    }

    pub fn mark_ops_synced(&mut self, op_ids: &[&str]) -> rusqlite::Result<()> {
        for id in op_ids {
            self.conn.execute(
                "UPDATE journal SET synced = 1 WHERE op_id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    pub fn unpushed_count(&self) -> rusqlite::Result<usize> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM journal WHERE synced = 0", [], |r| {
                    r.get(0)
                })?;
        Ok(count as usize)
    }

    pub fn get_meta(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM sync_meta WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Decide whether `op`'s path should win for its note: last writer wins by
    /// (HLC, device id), so every device converges on the same path. Records
    /// the op as the latest path change when it wins. Ops without a path never win.
    pub fn claim_path(&mut self, op: &Op) -> rusqlite::Result<bool> {
        if op.path.is_none() {
            return Ok(false);
        }
        let key = format!("path_stamp:{}", op.note_id);
        let stamp = op_stamp(op);
        if self.get_meta(&key)?.is_some_and(|current| current >= stamp) {
            return Ok(false);
        }
        self.set_meta(&key, &stamp)?;
        Ok(true)
    }

    /// HLC for a new local path change of `note_id` at wall time `now`: later
    /// than the last recorded change, so two renames in the same second still
    /// order correctly on every device.
    pub fn next_path_hlc(&mut self, note_id: &str, now: u64) -> rusqlite::Result<Hlc> {
        let last = self
            .get_meta(&format!("path_stamp:{note_id}"))?
            .and_then(|s| parse_hlc(&s))
            .unwrap_or_else(Hlc::origin);
        self.observe_hlc(last, u64::MAX / 2)?;
        self.tick_hlc(now)
    }

    pub fn is_blob_synced(&self, hash: &str, path: &str) -> rusqlite::Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM synced_blobs WHERE hash = ?1 AND path = ?2",
            params![hash, path],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn mark_blob_synced(&mut self, hash: &str, path: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO synced_blobs (hash, path) VALUES (?1, ?2)",
            params![hash, path],
        )?;
        self.mark_image_checked(hash)?;
        Ok(())
    }

    /// Drop history no longer needed, at most once a day (`last_prune` meta):
    /// contents not seen for [`CONTENT_RETENTION_SECS`] (except each note's
    /// current one), the bodies of pushed journal ops that old, and the legacy
    /// per-revision copies (`contents` holds them now). A merge whose base was
    /// dropped conflicts into a copy instead of merging; nothing is lost.
    /// `applied_ops` is kept: a server or vault change re-pulls every op, and
    /// it is what stops old ops being applied again. Returns whether it ran.
    pub fn maybe_prune(&mut self, now: u64) -> rusqlite::Result<bool> {
        let last = self
            .get_meta("last_prune")?
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if now < last + 24 * 60 * 60 {
            return Ok(false);
        }
        let cutoff = now.saturating_sub(CONTENT_RETENTION_SECS) as i64;
        self.conn.execute(
            "DELETE FROM contents WHERE seen_at < ?1 AND NOT EXISTS (
               SELECT 1 FROM current c WHERE c.note_id = contents.note_id AND c.hash = contents.hash)",
            params![cutoff],
        )?;
        self.conn.execute(
            "UPDATE journal SET content = NULL
             WHERE synced = 1 AND content IS NOT NULL AND hlc_ts < ?1",
            params![cutoff],
        )?;
        self.conn.execute(
            "UPDATE revisions SET content = NULL WHERE content IS NOT NULL",
            [],
        )?;
        self.set_meta("last_prune", &now.to_string())?;
        let (pages, free): (i64, i64) = self.conn.query_row(
            "SELECT (SELECT page_count FROM pragma_page_count()),
                    (SELECT freelist_count FROM pragma_freelist_count())",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if free * 4 > pages {
            self.conn.execute_batch("VACUUM")?;
        }
        Ok(true)
    }

    /// Every `(hash, path)` synced with the server.
    pub fn synced_blobs(&self) -> rusqlite::Result<Vec<(String, String)>> {
        self.conn
            .prepare("SELECT hash, path FROM synced_blobs")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    pub fn unmark_blob_synced(&mut self, hash: &str, path: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM synced_blobs WHERE hash = ?1 AND path = ?2",
            params![hash, path],
        )?;
        Ok(())
    }

    /// Record that this attachment content needs no (more) compression.
    pub fn mark_image_checked(&mut self, hash: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO checked_images (hash) VALUES (?1)",
            params![hash],
        )?;
        Ok(())
    }

    pub fn is_image_checked(&self, hash: &str) -> rusqlite::Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM checked_images WHERE hash = ?1)",
            params![hash],
            |r| r.get(0),
        )
    }

    pub fn clear_synced_blobs(&mut self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM synced_blobs", [])?;
        Ok(())
    }
}

/// Build a local create op.
pub fn local_create_op(device_id: &str, note_id: &str, content: Vec<u8>, hlc: Hlc, ts: u64) -> Op {
    Op {
        op_id: clock::op_id(),
        device_id: device_id.to_string(),
        note_id: note_id.to_string(),
        kind: OpKind::Create,
        base_rev: 0,
        new_rev: 0,
        content: Some(content),
        hlc,
        ts,
        path: None,
        purge: false,
        base_hash: None,
        content_ref: None,
    }
}

/// Build a local delete op.
pub fn local_delete_op(device_id: &str, note_id: &str, current_rev: u64, hlc: Hlc, ts: u64) -> Op {
    Op {
        op_id: clock::op_id(),
        device_id: device_id.to_string(),
        note_id: note_id.to_string(),
        kind: OpKind::Delete,
        base_rev: current_rev,
        new_rev: current_rev + 1,
        content: None,
        hlc,
        ts,
        path: None,
        purge: false,
        base_hash: None,
        content_ref: None,
    }
}

/// Build a local permanent-delete op: removes the note from every trash.
pub fn local_purge_op(device_id: &str, note_id: &str, current_rev: u64, hlc: Hlc, ts: u64) -> Op {
    Op {
        purge: true,
        ..local_delete_op(device_id, note_id, current_rev, hlc, ts)
    }
}

/// Build a local update op (Create/Update) against a device's current rev.
pub fn local_update_op(
    device_id: &str,
    note_id: &str,
    current_rev: u64,
    content: Vec<u8>,
    hlc: Hlc,
    ts: u64,
) -> Op {
    Op {
        op_id: clock::op_id(),
        device_id: device_id.to_string(),
        note_id: note_id.to_string(),
        kind: OpKind::Update,
        base_rev: current_rev,
        new_rev: current_rev + 1,
        content: Some(content),
        hlc,
        ts,
        path: None,
        purge: false,
        base_hash: None,
        content_ref: None,
    }
}

/// The losing version of a conflicting note, saved as a note of its own (with
/// its own id) next to the original. Every device that sees the same conflict
/// derives the same id, path and content, so the copies converge.
#[derive(Clone, Debug, PartialEq)]
pub struct ConflictCopy {
    pub note_id: String,
    pub path: String,
    pub content: Vec<u8>,
}

/// Apply a remote op to a device using 3-way Markdown merge.
///
/// `format` is `"markdown"`; `path` is the note's vault-relative path.
pub fn apply_remote(
    db: &mut SyncDb,
    vault: &LocalVault,
    op: &Op,
    format: &str,
    path: &str,
) -> rusqlite::Result<ApplyOutcome> {
    apply_remote_ex(db, vault, op, format, path).map(|(outcome, _)| outcome)
}

/// `apply_remote`, also returning the conflict copy it wrote, if any.
pub fn apply_remote_ex(
    db: &mut SyncDb,
    vault: &LocalVault,
    op: &Op,
    _format: &str,
    path: &str,
) -> rusqlite::Result<(ApplyOutcome, Option<ConflictCopy>)> {
    let cur = db.get_current(&op.note_id)?;

    if op.kind == OpKind::Delete {
        let outcome = match cur {
            Some((local_rev, _)) => {
                // The delete applies only to the version it was made on; a
                // concurrent local edit keeps the note (`DeleteConflict`).
                let unchanged = match (vault.read(path).ok(), &op.base_hash) {
                    (None, _) => true,
                    (Some(mine), Some(base)) => hash::content_hash(&mine) == *base,
                    (Some(_), None) => op.base_rev == local_rev,
                };
                if unchanged {
                    let _ = vault.delete_to_trash(path);
                    db.tombstone(&op.note_id, op.ts)?;
                    db.set_current(&op.note_id, local_rev.max(op.new_rev), None)?;
                    ApplyOutcome::Deleted
                } else {
                    ApplyOutcome::DeleteConflict
                }
            }
            None => {
                db.tombstone(&op.note_id, op.ts)?;
                db.set_current(&op.note_id, op.new_rev, None)?;
                ApplyOutcome::Deleted
            }
        };
        return Ok((outcome, None));
    }

    let Some(content) = &op.content else {
        return Ok((ApplyOutcome::Noop, None));
    };
    // Kept even when rejected below: a later op may name it as its base.
    let theirs_hash = db.remember_content(&op.note_id, content, &op_stamp(op))?;

    let Some((local_rev, local_hash)) = cur else {
        if db.is_tombstoned(&op.note_id)? {
            return Ok((ApplyOutcome::RejectedTombstone, None));
        }
        vault.write_atomic(path, content).map_err(rusqlite_err)?;
        db.set_current(&op.note_id, op.new_rev, Some(content))?;
        return Ok((ApplyOutcome::Created, None));
    };
    // Reject stale pushes against a tombstoned note (no resurrection).
    if db.is_tombstoned(&op.note_id)? && op.base_rev < local_rev {
        return Ok((ApplyOutcome::RejectedTombstone, None));
    }
    let mine = vault.read(path).map_err(rusqlite_err)?;
    let mine_hash = hash::content_hash(&mine);
    let rev = local_rev.max(op.new_rev);
    if mine_hash == theirs_hash {
        db.set_current(&op.note_id, rev, Some(content))?;
        return Ok((ApplyOutcome::Noop, None));
    }
    // A file that differs from what sync.db recorded was edited outside the app.
    let external_edit = mine_hash != local_hash;
    let fast_forward = match &op.base_hash {
        Some(base) => *base == mine_hash,
        None => !external_edit && op.base_rev >= local_rev,
    };
    if fast_forward {
        vault.write_atomic(path, content).map_err(rusqlite_err)?;
        db.set_current(&op.note_id, rev, Some(content))?;
        return Ok((ApplyOutcome::FastForward, None));
    }

    // Concurrent edits. The base is the content the op was built on (by hash;
    // revision numbers collide across devices); older ops name it by rev. An
    // unknown base merges against nothing, which conflicts instead of letting
    // one side silently overwrite the other.
    let base_rev = if external_edit {
        local_rev
    } else {
        op.base_rev
    };
    let base = match &op.base_hash {
        Some(h) => db.content_by_hash(&op.note_id, h)?,
        None => db.get_revision(&op.note_id, base_rev)?,
    }
    .map(|b| String::from_utf8_lossy(&b).into_owned())
    .unwrap_or_default();
    let mut mine_s = String::from_utf8_lossy(&mine).into_owned();
    let mut theirs_s = String::from_utf8_lossy(content).into_owned();
    align_updated(&mut mine_s, &mut theirs_s);
    let (merged, conflict) = merge::merge_md(&base, &mine_s, &theirs_s);
    if !conflict {
        vault
            .write_atomic(path, merged.as_bytes())
            .map_err(rusqlite_err)?;
        db.set_current(&op.note_id, rev, Some(merged.as_bytes()))?;
        return Ok((ApplyOutcome::Merged { conflict: false }, None));
    }

    // Overlapping edits: the newest version stays in the note (the same choice
    // on every device), the other goes to a conflict copy. A local version
    // with no known stamp is a fresh edit on disk, so it stays.
    let mut mine_stamp = db.content_stamp(&op.note_id, &mine_hash)?;
    if mine_stamp.is_empty() {
        mine_stamp = db.latest_local_stamp(&op.note_id)?;
    }
    let theirs_stamp = op_stamp(op);
    let mine_wins = mine_stamp.is_empty() || mine_stamp > theirs_stamp;
    let (winner, loser, loser_hash, loser_stamp) = if mine_wins {
        (&mine, content, &theirs_hash, theirs_stamp)
    } else {
        (content, &mine, &mine_hash, mine_stamp)
    };
    let mut parts = loser_stamp.splitn(3, ':');
    let loser_ts: u64 = parts.next().and_then(|t| t.parse().ok()).unwrap_or(0);
    let loser_device = parts.nth(1).unwrap_or("device");
    let seed = format!("{}/{loser_hash}", op.note_id);
    let copy_id = format!("note_{}", &hash::content_hash(seed.as_bytes())[..32]);
    let loser_text = String::from_utf8_lossy(loser);
    let has_id = frontmatter::split(&loser_text)
        .0
        .is_some_and(|fm| frontmatter::notium_id(&fm).is_some());
    let copy_text = if has_id {
        frontmatter::upsert_field(&loser_text, "notium_id", &copy_id)
    } else {
        frontmatter::inject_notium_id(&loser_text, &copy_id)
    };
    let copy = ConflictCopy {
        note_id: copy_id,
        path: merge::conflict_copy_path(path, loser_device, loser_ts),
        content: copy_text.into_bytes(),
    };
    if !mine_wins {
        vault.write_atomic(path, winner).map_err(rusqlite_err)?;
    }
    vault
        .write_atomic(&copy.path, &copy.content)
        .map_err(rusqlite_err)?;
    db.set_current(&op.note_id, rev, Some(winner))?;
    Ok((ApplyOutcome::Merged { conflict: true }, Some(copy)))
}

/// Give both versions the later `updated:` frontmatter value, so the field
/// every save rewrites does not make each pair of concurrent edits conflict.
fn align_updated(a: &mut String, b: &mut String) {
    let updated = |t: &str| {
        frontmatter::split(t)
            .0
            .and_then(|fm| frontmatter::field(&fm, "updated"))
    };
    let (Some(x), Some(y)) = (updated(a), updated(b)) else {
        return;
    };
    if x == y {
        return;
    }
    let later = match (x.parse::<u64>(), y.parse::<u64>()) {
        (Ok(nx), Ok(ny)) if nx >= ny => x,
        (Ok(_), Ok(_)) => y,
        _ => x.max(y),
    };
    *a = frontmatter::upsert_field(a, "updated", &later);
    *b = frontmatter::upsert_field(b, "updated", &later);
}

fn rusqlite_err<E: std::fmt::Display>(e: E) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    fn op_at(ts: u64, device: &str, path: Option<&str>) -> Op {
        let mut op = local_update_op(device, "n1", 0, b"x".to_vec(), Hlc { ts, cnt: 0 }, ts);
        op.path = path.map(str::to_string);
        op
    }

    #[test]
    fn op_without_path_deserializes_and_omits_field() {
        let json = r#"{"op_id":"o","device_id":"d","note_id":"n","kind":"update","base_rev":0,"new_rev":1,"content":null,"hlc":{"ts":1,"cnt":0},"ts":1}"#;
        let op: Op = serde_json::from_str(json).unwrap();
        assert_eq!(op.path, None);
        assert!(!serde_json::to_string(&op).unwrap().contains("path"));
    }

    #[test]
    fn journal_keeps_purge_for_push() {
        let mut db = SyncDb::open_in_memory().unwrap();
        db.append_journal(&local_purge_op("pc", "n1", 3, Hlc { ts: 9, cnt: 0 }, 9))
            .unwrap();
        let op = &db.get_unpushed_ops().unwrap()[0];
        assert!(op.purge);
        assert_eq!((op.kind, op.base_rev, op.new_rev), (OpKind::Delete, 3, 4));
        assert!(serde_json::to_string(op)
            .unwrap()
            .contains("\"purge\":true"));
    }

    #[test]
    fn journal_keeps_path_for_push() {
        let mut db = SyncDb::open_in_memory().unwrap();
        db.append_journal(&op_at(5, "pc", Some("notes/markdown/A.md")))
            .unwrap();
        let ops = db.get_unpushed_ops().unwrap();
        assert_eq!(ops[0].path.as_deref(), Some("notes/markdown/A.md"));
    }

    #[test]
    fn latest_path_change_wins_regardless_of_arrival_order() {
        let newer = op_at(20, "pc", Some("notes/markdown/B.md"));
        let older = op_at(10, "phone", Some("notes/markdown/A.md"));
        for order in [[&newer, &older], [&older, &newer]] {
            let mut db = SyncDb::open_in_memory().unwrap();
            let winners: Vec<_> = order
                .iter()
                .filter(|op| db.claim_path(op).unwrap())
                .map(|op| op.path.clone().unwrap())
                .collect();
            assert_eq!(winners.last().unwrap(), "notes/markdown/B.md");
        }
        let mut db = SyncDb::open_in_memory().unwrap();
        assert!(!db.claim_path(&op_at(30, "pc", None)).unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_db_journal_and_meta() {
        let mut db = SyncDb::open_in_memory().unwrap();
        assert_eq!(db.unpushed_count().unwrap(), 0);

        let op1 = local_create_op(
            "dev1",
            "note1",
            b"hello".to_vec(),
            Hlc::origin().tick(1),
            100,
        );
        let op2 = local_update_op(
            "dev1",
            "note1",
            0,
            b"hello world".to_vec(),
            Hlc::origin().tick(2),
            200,
        );

        db.append_journal(&op1).unwrap();
        db.append_journal(&op2).unwrap();

        assert_eq!(db.unpushed_count().unwrap(), 2);
        let unpushed = db.get_unpushed_ops().unwrap();
        assert_eq!(unpushed.len(), 2);
        assert_eq!(unpushed[0].op_id, op1.op_id);
        assert_eq!(unpushed[1].op_id, op2.op_id);

        db.mark_ops_synced(&[&op1.op_id]).unwrap();
        assert_eq!(db.unpushed_count().unwrap(), 1);
        let unpushed_after = db.get_unpushed_ops().unwrap();
        assert_eq!(unpushed_after.len(), 1);
        assert_eq!(unpushed_after[0].op_id, op2.op_id);

        assert!(db.get_meta("last_pull_cursor").unwrap().is_none());
        db.set_meta("last_pull_cursor", "cursor_12345").unwrap();
        assert_eq!(
            db.get_meta("last_pull_cursor").unwrap().as_deref(),
            Some("cursor_12345")
        );
    }

    #[test]
    fn test_op_kind_serde_case_insensitivity() {
        let json_lower = r#""create""#;
        let json_upper = r#""Create""#;
        let op_lower: OpKind = serde_json::from_str(json_lower).unwrap();
        let op_upper: OpKind = serde_json::from_str(json_upper).unwrap();
        assert_eq!(op_lower, OpKind::Create);
        assert_eq!(op_upper, OpKind::Create);
        assert_eq!(serde_json::to_string(&op_lower).unwrap(), r#""create""#);
    }

    #[test]
    fn test_has_delete_op() {
        let mut db = SyncDb::open_in_memory().unwrap();
        assert!(!db.has_delete_op("n1").unwrap());

        let create = local_create_op("dev1", "n1", b"hello".to_vec(), Hlc::origin().tick(1), 100);
        db.append_journal(&create).unwrap();
        assert!(
            !db.has_delete_op("n1").unwrap(),
            "a create op is not a delete op"
        );

        let delete = local_delete_op("dev1", "n1", 0, Hlc::origin().tick(2), 200);
        db.append_journal(&delete).unwrap();
        assert!(db.has_delete_op("n1").unwrap());
        assert!(!db.has_delete_op("n2").unwrap());
    }

    #[test]
    fn prune_drops_old_history_but_keeps_current_and_pending() {
        let mut db = SyncDb::open_in_memory().unwrap();
        let now: u64 = crate::vault::now_ts().parse().unwrap();
        let old = local_create_op("dev1", "n1", b"v1".to_vec(), Hlc { ts: now, cnt: 0 }, now);
        db.append_journal(&old).unwrap();
        db.mark_ops_synced(&[&old.op_id]).unwrap();
        db.set_current("n1", 1, Some(b"v2")).unwrap();
        let pending = local_create_op("dev1", "n2", b"p".to_vec(), Hlc { ts: now, cnt: 1 }, now);
        db.append_journal(&pending).unwrap();
        db.mark_applied("remote-op").unwrap();

        let later = now + CONTENT_RETENTION_SECS + 10;
        assert!(db.maybe_prune(later).unwrap());
        assert!(!db.maybe_prune(later + 60).unwrap(), "once a day");

        let v1 = hash::content_hash(b"v1");
        let v2 = hash::content_hash(b"v2");
        assert!(db.content_by_hash("n1", &v1).unwrap().is_none());
        assert_eq!(
            db.content_by_hash("n1", &v2).unwrap().as_deref(),
            Some(&b"v2"[..])
        );
        assert_eq!(
            db.get_unpushed_ops().unwrap()[0].content.as_deref(),
            Some(&b"p"[..])
        );
        assert!(db.is_known_op(&old.op_id).unwrap());
        assert!(db.is_known_op("remote-op").unwrap());
        assert!(!db.latest_local_stamp("n1").unwrap().is_empty());
    }

    #[test]
    fn test_synced_blobs_tracking_and_reset() {
        let mut db = SyncDb::open_in_memory().unwrap();
        assert!(!db.is_blob_synced("h1", "attachments/a.png").unwrap());

        db.mark_blob_synced("h1", "attachments/a.png").unwrap();
        assert!(db.is_blob_synced("h1", "attachments/a.png").unwrap());
        assert!(!db.is_blob_synced("h1", "attachments/b.png").unwrap());

        db.clear_synced_blobs().unwrap();
        assert!(!db.is_blob_synced("h1", "attachments/a.png").unwrap());
    }

    #[test]
    fn test_normalize_vault_id() {
        assert_eq!(normalize_vault_id(""), "default");
        assert_eq!(normalize_vault_id("   "), "default");
        assert_eq!(normalize_vault_id("  Trabajo_2026-A "), "trabajo_2026-a");
        assert_eq!(normalize_vault_id("invalid space"), "default");
        assert_eq!(normalize_vault_id("invalid/slash"), "default");
        assert_eq!(normalize_vault_id(&"a".repeat(65)), "default");
    }

    #[test]
    fn test_is_safe_attachment_rel_path() {
        assert!(is_safe_attachment_rel_path("attachments/photo.png"));
        assert!(is_safe_attachment_rel_path("attachments/sub/doc.pdf"));
        assert!(!is_safe_attachment_rel_path("attachments/"));
        assert!(!is_safe_attachment_rel_path("notes/markdown/test.md"));
        assert!(!is_safe_attachment_rel_path("attachments/../secret.txt"));
        assert!(!is_safe_attachment_rel_path("attachments\\photo.png"));
        assert!(!is_safe_attachment_rel_path("/attachments/photo.png"));
    }
}
