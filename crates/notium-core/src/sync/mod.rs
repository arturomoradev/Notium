//! Sync engine: op journal, per-note revisions (merge bases), tombstones, and
//! remote-op application with per-format merge rules (ADR-06/08).

pub mod merge;

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clock::{self, Hlc};
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
}

impl Op {
    /// Attach the note path this op sets.
    pub fn with_path(mut self, path: impl Into<String>) -> Op {
        self.path = Some(path.into());
        self
    }
}

/// Total order of path changes: the HLC, then the device id as tie-break.
fn path_stamp(op: &Op) -> String {
    // Zero-padded so the string order matches the numeric order.
    format!("{:020}:{:010}:{}", op.hlc.ts, op.hlc.cnt, op.device_id)
}

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

/// Maximum size in bytes of an attachment binary stored in a Cloudflare D1 `BLOB` column.
pub const MAX_D1_BLOB_BYTES: u64 = 900_000;

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
"#;

/// Per-device sync state backed by `.app/sync.db`.
pub struct SyncDb {
    conn: Connection,
}

impl SyncDb {
    pub fn open(path: &Path) -> rusqlite::Result<SyncDb> {
        let conn = Connection::open(path)?;
        crate::index::configure_connection(&conn)?;
        conn.execute_batch(SYNC_SCHEMA)?;
        let _ = conn.execute(
            "ALTER TABLE journal ADD COLUMN synced INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE journal ADD COLUMN path TEXT", []);
        let _ = conn.execute(
            "ALTER TABLE journal ADD COLUMN purge INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(SyncDb { conn })
    }

    pub fn open_in_memory() -> rusqlite::Result<SyncDb> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SYNC_SCHEMA)?;
        let _ = conn.execute(
            "ALTER TABLE journal ADD COLUMN synced INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE journal ADD COLUMN path TEXT", []);
        let _ = conn.execute(
            "ALTER TABLE journal ADD COLUMN purge INTEGER NOT NULL DEFAULT 0",
            [],
        );
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

    pub fn get_revision(&self, note_id: &str, rev: u64) -> rusqlite::Result<Option<Vec<u8>>> {
        self.conn
            .query_row(
                "SELECT content FROM revisions WHERE note_id = ?1 AND rev = ?2",
                params![note_id, rev as i64],
                |r| r.get::<_, Option<Vec<u8>>>(0),
            )
            .optional()
            .map(|o| o.flatten())
    }

    /// Record a revision snapshot (the content at `rev`) and update the current pointer.
    pub fn set_current(
        &mut self,
        note_id: &str,
        rev: u64,
        content: Option<&[u8]>,
    ) -> rusqlite::Result<()> {
        let h = content.map(hash::content_hash).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO revisions (note_id, rev, content, hash) VALUES (?1,?2,?3,?4)",
            params![note_id, rev as i64, content, h],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO current (note_id, rev, hash) VALUES (?1,?2,?3)",
            params![note_id, rev as i64, h],
        )?;
        Ok(())
    }

    pub fn append_journal(&mut self, op: &Op) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO journal (op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
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
                op.purge as i64
            ],
        )?;
        Ok(())
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
        let mut stmt = self.conn.prepare(
            "SELECT op_id, device_id, note_id, kind, base_rev, new_rev, content, hlc_ts, hlc_cnt, ts, path, purge
             FROM journal WHERE synced = 0 ORDER BY hlc_ts ASC, hlc_cnt ASC, ts ASC",
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
        let stamp = path_stamp(op);
        if self.get_meta(&key)?.is_some_and(|current| current >= stamp) {
            return Ok(false);
        }
        self.set_meta(&key, &stamp)?;
        Ok(true)
    }

    /// HLC for a new local path change of `note_id` at wall time `now`: later
    /// than the last recorded change, so two renames in the same second still
    /// order correctly on every device.
    pub fn next_path_hlc(&self, note_id: &str, now: u64) -> rusqlite::Result<Hlc> {
        let last = self
            .get_meta(&format!("path_stamp:{note_id}"))?
            .and_then(|s| {
                let mut it = s.splitn(3, ':');
                Some(Hlc {
                    ts: it.next()?.parse().ok()?,
                    cnt: it.next()?.parse().ok()?,
                })
            })
            .unwrap_or_else(Hlc::origin);
        Ok(last.tick(now))
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
        Ok(())
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
    }
}

/// Apply a remote op to a device using 3-way Markdown merge.
///
/// `_format` is `"markdown"`; `path` is the note's vault-relative path.
#[allow(clippy::too_many_arguments)]
pub fn apply_remote(
    db: &mut SyncDb,
    vault: &LocalVault,
    op: &Op,
    _format: &str,
    path: &str,
) -> rusqlite::Result<ApplyOutcome> {
    let cur = db.get_current(&op.note_id)?;

    // Deletion handling
    if op.kind == OpKind::Delete {
        return match cur {
            Some((local_rev, _)) => {
                if op.base_rev == local_rev {
                    let _ = vault.delete_to_trash(path);
                    db.tombstone(&op.note_id, op.ts)?;
                    db.set_current(&op.note_id, op.new_rev, None)?;
                    Ok(ApplyOutcome::Deleted)
                } else {
                    Ok(ApplyOutcome::DeleteConflict)
                }
            }
            None => {
                db.tombstone(&op.note_id, op.ts)?;
                db.set_current(&op.note_id, op.new_rev, None)?;
                Ok(ApplyOutcome::Deleted)
            }
        };
    }

    // Content op
    let Some(content) = &op.content else {
        return Ok(ApplyOutcome::Noop);
    };
    let content = content.clone();

    match cur {
        None => {
            if db.is_tombstoned(&op.note_id)? {
                return Ok(ApplyOutcome::RejectedTombstone);
            }
            vault.write_atomic(path, &content).map_err(rusqlite_err)?;
            db.set_current(&op.note_id, op.new_rev, Some(&content))?;
            Ok(ApplyOutcome::Created)
        }
        Some((local_rev, local_hash)) => {
            // Reject stale pushes against a tombstoned note (no resurrection).
            if db.is_tombstoned(&op.note_id)? && op.base_rev < local_rev {
                return Ok(ApplyOutcome::RejectedTombstone);
            }
            let mine = vault.read(path).map_err(rusqlite_err)?;
            let mine_hash = hash::content_hash(&mine);
            // Detect an external disk edit (file differs from what sync.db recorded).
            let external_edit = mine_hash != local_hash;

            if !external_edit && local_hash == hash::content_hash(&content) {
                // Already have this exact content
                if op.new_rev > local_rev {
                    db.set_current(&op.note_id, op.new_rev, Some(&content))?;
                }
                return Ok(ApplyOutcome::Noop);
            }
            if !external_edit && op.base_rev >= local_rev {
                // Clean fast-forward (up to date, or server ahead).
                vault.write_atomic(path, &content).map_err(rusqlite_err)?;
                db.set_current(&op.note_id, op.new_rev, Some(&content))?;
                return Ok(ApplyOutcome::FastForward);
            }

            // Merge path: either concurrent edits (base_rev < local_rev) or an
            // external disk edit (never silently overwritten).
            let mine_s = String::from_utf8_lossy(&mine).to_string();
            let theirs_s = String::from_utf8_lossy(&content).to_string();

            // markdown 3-way merge. Base = last synced content (for an external
            // edit) or the content at op.base_rev (for a concurrent edit).
            let base_rev = if external_edit {
                local_rev
            } else {
                op.base_rev
            };
            let base = db
                .get_revision(&op.note_id, base_rev)?
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_else(|| mine_s.clone());
            let (merged, conflict) = merge::merge_md(&base, &mine_s, &theirs_s);
            let merged_bytes = merged.as_bytes().to_vec();
            vault
                .write_atomic(path, &merged_bytes)
                .map_err(rusqlite_err)?;
            if conflict {
                let copy = merge::conflict_copy_path(path, &op.device_id, op.ts);
                vault.write_atomic(&copy, &content).map_err(rusqlite_err)?;
            }
            db.set_current(&op.note_id, op.new_rev, Some(&merged_bytes))?;
            Ok(ApplyOutcome::Merged { conflict })
        }
    }
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
