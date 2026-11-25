//! Derived SQLite index (notes, aliases, tags, links, attachments, chunks, FTS5).
//!
//! The index is a cache: it can always be rebuilt from the vault files. Identity
//! resolution (embedded UUIDv7 → content-hash → alias/path heuristic → new id) and
//! link resolution live here.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::clock;
use crate::extract;
use crate::hash;
use crate::links::normalize_title;
use crate::vault::{self, LocalVault};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS notes (
  note_id     TEXT PRIMARY KEY,
  path        TEXT NOT NULL,
  format      TEXT NOT NULL,
  title       TEXT NOT NULL,
  created_at  TEXT,
  updated_at  TEXT,
  content_hash TEXT NOT NULL,
  file_size   INTEGER NOT NULL DEFAULT 0,
  tombstoned  INTEGER NOT NULL DEFAULT 0,
  quarantined INTEGER NOT NULL DEFAULT 0,
  dirty       INTEGER NOT NULL DEFAULT 1,
  fts_rowid   INTEGER,
  mtime_ns    INTEGER
);
CREATE INDEX IF NOT EXISTS idx_notes_path ON notes(path);
-- Uniqueness applies only to live notes so a tombstoned note keeps its real
-- path without blocking a surviving note that is renamed/created onto it.
CREATE UNIQUE INDEX IF NOT EXISTS uq_notes_live_path ON notes(path) WHERE tombstoned = 0;

CREATE TABLE IF NOT EXISTS aliases (
  alias   TEXT PRIMARY KEY,
  note_id TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_aliases_note ON aliases(note_id);

CREATE TABLE IF NOT EXISTS tags (
  tag     TEXT NOT NULL,
  note_id TEXT NOT NULL,
  PRIMARY KEY (tag, note_id)
);
CREATE INDEX IF NOT EXISTS idx_tags_note ON tags(note_id);

CREATE TABLE IF NOT EXISTS links (
  source_id     TEXT NOT NULL,
  link_text     TEXT NOT NULL,
  kind          TEXT NOT NULL,
  target_raw    TEXT NOT NULL,
  target_id     TEXT,
  target_path   TEXT,
  is_attachment INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (source_id, link_text, kind, target_raw)
);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_id);

CREATE TABLE IF NOT EXISTS attachments (
  att_id       TEXT PRIMARY KEY,
  path         TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  file_size    INTEGER NOT NULL DEFAULT 0,
  tombstoned   INTEGER NOT NULL DEFAULT 0,
  mtime_ns     INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_attachments_live_path ON attachments(path) WHERE tombstoned = 0;

CREATE TABLE IF NOT EXISTS attachment_refs (
  note_id TEXT NOT NULL,
  att_id  TEXT NOT NULL,
  PRIMARY KEY (note_id, att_id)
);

CREATE TABLE IF NOT EXISTS chunks (
  chunk_id     TEXT PRIMARY KEY,
  note_id      TEXT NOT NULL,
  kind         TEXT NOT NULL,
  heading_path TEXT NOT NULL DEFAULT '',
  body         TEXT NOT NULL,
  content_hash TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_chunks_note ON chunks(note_id);

CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
  title, body, tags, note_id,
  tokenize = 'unicode61 remove_diacritics 2'
);
"#;

/// Schema version tracked via `PRAGMA user_version`, so future migrations can
/// gate on it. Bumped to 1 when the partial-unique-index migration runs.
const SCHEMA_VERSION: i64 = 1;

/// Synthetic path prefix used to detach notes/attachments during reconcile so
/// that a path swap does not trip the UNIQUE constraint. It is never written to
/// disk and is always replaced before commit. It must never collide with a real
/// `notes/markdown/...` or `attachments/...` path, which holds because every
/// real rel-path lives under one of those two roots.
const RECONCILE_TMP_PREFIX: &str = "__reconcile__/";

/// An mtime younger than this is not trusted: coarse timestamps (FUSE on
/// Android, FAT, some network shares) could hide an edit made in the same tick.
const MTIME_SETTLE_NS: i64 = 2_000_000_000;

type MtimeCache = HashMap<String, (String, String, u64, i64)>;

fn now_ns() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(i64::MAX)
}

/// The file's mtime if it is old enough to be relied on (future ones are not).
fn trusted_mtime(meta: &vault::FileMeta, now: i64) -> Option<i64> {
    meta.mtime_ns
        .filter(|&t| now.saturating_sub(t) >= MTIME_SETTLE_NS)
}

/// `(id, content_hash)` of the live row for this file if size and mtime match.
fn cache_hit(
    cache: &MtimeCache,
    meta: &vault::FileMeta,
    mtime: Option<i64>,
) -> Option<(String, String)> {
    let (id, hash, size, cached_mtime) = cache.get(&meta.rel_path)?;
    (mtime? == *cached_mtime && *size == meta.size).then(|| (id.clone(), hash.clone()))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoteRecord {
    pub note_id: String,
    pub path: String,
    pub format: String,
    pub title: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub content_hash: String,
    pub tombstoned: bool,
    pub quarantined: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHit {
    pub note_id: String,
    pub title: String,
    pub path: String,
    pub format: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Backlink {
    pub source_id: String,
    pub source_title: String,
    pub link_text: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ReconcileReport {
    pub scanned: usize,
    pub adopted: usize,
    pub resolved_by_hash: usize,
    pub new: usize,
    pub updated: usize,
    pub quarantined: usize,
    pub tombstones: Vec<String>,
    pub collisions: Vec<Collision>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Collision {
    pub id: String,
    pub paths: Vec<String>,
}

pub struct Index {
    conn: Connection,
}

/// Connection settings shared by the on-disk databases (index + sync journal):
/// WAL so readers never block the writer and a crash cannot leave a half-written
/// rollback journal, `synchronous=NORMAL` (the durable setting recommended for
/// WAL), and a busy timeout instead of failing immediately with SQLITE_BUSY.
pub fn configure_connection(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

impl Index {
    pub fn open(db_path: &Path) -> rusqlite::Result<Index> {
        let conn = Connection::open(db_path)?;
        configure_connection(&conn)?;
        conn.execute_batch(SCHEMA)?;
        Self::finish_open(conn)
    }

    pub fn open_in_memory() -> rusqlite::Result<Index> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::finish_open(conn)
    }

    /// Apply pending schema migrations and return the ready index.
    fn finish_open(conn: Connection) -> rusqlite::Result<Index> {
        let mut index = Index { conn };
        index.migrate()?;
        Ok(index)
    }

    /// Apply pending schema migrations (currently the partial-unique-index
    /// migration that replaces the column-level UNIQUE on `path`).
    fn migrate(&mut self) -> rusqlite::Result<()> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < SCHEMA_VERSION {
            if Self::has_legacy_path_unique(&self.conn)? {
                self.rebuild_path_constraints()?;
            }
            self.conn
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        // Idempotent: indexes created before the mtime cache get the column as
        // NULL, which makes the next reconcile read every file once.
        for table in ["notes", "attachments"] {
            let has: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'mtime_ns'",
                params![table],
                |r| r.get(0),
            )?;
            if has == 0 {
                self.conn
                    .execute_batch(&format!("ALTER TABLE {table} ADD COLUMN mtime_ns INTEGER"))?;
            }
        }
        Ok(())
    }

    /// True if the `notes` or `attachments` table still carries a column-level
    /// UNIQUE constraint on `path` (the pre-migration layout).
    fn has_legacy_path_unique(conn: &Connection) -> rusqlite::Result<bool> {
        let notes: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_index_list('notes') WHERE origin = 'u'",
            [],
            |r| r.get(0),
        )?;
        let attachments: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_index_list('attachments') WHERE origin = 'u'",
            [],
            |r| r.get(0),
        )?;
        Ok(notes > 0 || attachments > 0)
    }

    /// Rebuild the `notes` and `attachments` tables without the column-level
    /// UNIQUE on `path`, replacing it with a partial unique index over live
    /// rows only. SQLite cannot drop a UNIQUE constraint in place, hence the
    /// table rebuild. `aliases` and `notes_fts` are separate and untouched, and
    /// `fts_rowid` is preserved by copying every row.
    fn rebuild_path_constraints(&mut self) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE notes_migrated (
               note_id     TEXT PRIMARY KEY,
               path        TEXT NOT NULL,
               format      TEXT NOT NULL,
               title       TEXT NOT NULL,
               created_at  TEXT,
               updated_at  TEXT,
               content_hash TEXT NOT NULL,
               file_size   INTEGER NOT NULL DEFAULT 0,
               tombstoned  INTEGER NOT NULL DEFAULT 0,
               quarantined INTEGER NOT NULL DEFAULT 0,
               dirty       INTEGER NOT NULL DEFAULT 1,
               fts_rowid   INTEGER,
               mtime_ns    INTEGER
             );
             INSERT INTO notes_migrated
               (note_id, path, format, title, created_at, updated_at, content_hash,
                file_size, tombstoned, quarantined, dirty, fts_rowid)
               SELECT note_id, path, format, title, created_at, updated_at, content_hash,
                      file_size, tombstoned, quarantined, dirty, fts_rowid
               FROM notes;
             DROP TABLE notes;
             ALTER TABLE notes_migrated RENAME TO notes;
             CREATE INDEX idx_notes_path ON notes(path);
             CREATE UNIQUE INDEX uq_notes_live_path ON notes(path) WHERE tombstoned = 0;

             CREATE TABLE attachments_migrated (
               att_id       TEXT PRIMARY KEY,
               path         TEXT NOT NULL,
               content_hash TEXT NOT NULL,
               file_size    INTEGER NOT NULL DEFAULT 0,
               tombstoned   INTEGER NOT NULL DEFAULT 0,
               mtime_ns     INTEGER
             );
             INSERT INTO attachments_migrated
               (att_id, path, content_hash, file_size, tombstoned)
               SELECT att_id, path, content_hash, file_size, tombstoned
               FROM attachments;
             DROP TABLE attachments;
             ALTER TABLE attachments_migrated RENAME TO attachments;
             CREATE UNIQUE INDEX uq_attachments_live_path ON attachments(path) WHERE tombstoned = 0;",
        )?;
        tx.commit()
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Wipe all tables (used before a cold rebuild).
    pub fn clear_all(&mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "DELETE FROM notes; DELETE FROM aliases; DELETE FROM tags; DELETE FROM links;
             DELETE FROM attachments; DELETE FROM attachment_refs; DELETE FROM chunks;
             DELETE FROM notes_fts;",
        )
    }

    /// Cold rebuild: wipe and re-index the entire vault from disk.
    pub fn rebuild(&mut self, vault: &LocalVault) -> rusqlite::Result<ReconcileReport> {
        self.clear_all()?;
        self.reconcile(vault)
    }

    /// Incremental reconcile: scan the vault, resolve identity, adopt new files,
    /// re-extract changed ones, tombstone missing ones, and resolve links.
    pub fn reconcile(&mut self, vault: &LocalVault) -> rusqlite::Result<ReconcileReport> {
        // Wrap all DB writes in one transaction for performance (10k notes) and
        // atomicity (the index is always consistent or unchanged).
        self.conn.execute_batch("BEGIN")?;
        match self.reconcile_inner(vault) {
            Ok(r) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(r)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn reconcile_inner(&mut self, vault: &LocalVault) -> rusqlite::Result<ReconcileReport> {
        // Files whose live index row has the same path, size and (trusted)
        // mtime are reused as-is: their content is not read or hashed again.
        let scan_now = now_ns();
        let note_cache = self.mtime_cache("note_id", "notes")?;
        // (entry, trusted mtime, cached id)
        let mut notes: Vec<(vault::NoteEntry, Option<i64>, Option<String>)> = Vec::new();
        for (meta, editor) in vault.scan_note_meta().map_err(rusqlite_err)? {
            let mtime = trusted_mtime(&meta, scan_now);
            match cache_hit(&note_cache, &meta, mtime) {
                Some((id, hash)) => notes.push((
                    vault::NoteEntry {
                        rel_path: meta.rel_path,
                        editor,
                        bytes: Vec::new(),
                        hash,
                        size: meta.size,
                    },
                    mtime,
                    Some(id),
                )),
                None => notes.push((
                    vault
                        .read_note_entry(meta.rel_path, editor)
                        .map_err(rusqlite_err)?,
                    mtime,
                    None,
                )),
            }
        }
        let att_cache = self.mtime_cache("att_id", "attachments")?;
        let mut attachments: Vec<(vault::AttachmentEntry, Option<i64>)> = Vec::new();
        for meta in vault.scan_attachment_meta().map_err(rusqlite_err)? {
            let mtime = trusted_mtime(&meta, scan_now);
            let entry = match cache_hit(&att_cache, &meta, mtime) {
                Some((_, hash)) => vault::AttachmentEntry {
                    rel_path: meta.rel_path,
                    bytes: Vec::new(),
                    hash,
                    size: meta.size,
                },
                None => vault
                    .read_attachment_entry(meta.rel_path)
                    .map_err(rusqlite_err)?,
            };
            attachments.push((entry, mtime));
        }

        // Existing index state
        let mut existing: HashMap<String, (String, String)> = HashMap::new(); // note_id -> (path, hash)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT note_id, path, content_hash FROM notes")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (id, path, h) = row?;
                existing.insert(id, (path, h));
            }
        }
        let mut hash_to_id: HashMap<String, String> = HashMap::new();
        let mut path_to_id: HashMap<String, String> = HashMap::new();
        for (id, (p, h)) in &existing {
            hash_to_id.entry(h.clone()).or_insert_with(|| id.clone());
            path_to_id.entry(p.clone()).or_insert_with(|| id.clone());
        }

        // Identity resolution pass
        let mut report = ReconcileReport {
            scanned: notes.len(),
            ..Default::default()
        };
        let mut id_to_path: HashMap<String, String> = HashMap::new();
        // (entry, id, had_embedded_id, mtime to store, cached id)
        type Assignment = (vault::NoteEntry, String, bool, Option<i64>, Option<String>);
        let mut assignments: Vec<Assignment> = Vec::new();

        for (entry, mtime, cached) in notes {
            // A cached file was last indexed clean: its embedded id is its row id.
            let embedded = match &cached {
                Some(id) => Some(id.clone()),
                None => vault::extract_embedded_id(&entry.bytes, entry.editor),
            };
            let mut id;
            let mut collision = false;
            match &embedded {
                Some(eid) => {
                    if let Some(prev) = id_to_path.get(eid) {
                        report.collisions.push(Collision {
                            id: eid.clone(),
                            paths: vec![prev.clone(), entry.rel_path.clone()],
                        });
                        id = clock::note_id();
                        collision = true;
                    } else {
                        id = eid.clone();
                    }
                }
                None => {
                    if let Some(found) = hash_to_id.get(&entry.hash).cloned() {
                        // rename/move detection: same content, possibly new path
                        if existing.get(&found).map(|(p, _)| p.clone())
                            != Some(entry.rel_path.clone())
                        {
                            report.resolved_by_hash += 1;
                        }
                        id = found;
                    } else if let Some(found) = path_to_id.get(&entry.rel_path).cloned() {
                        // same path, content edited in place (e.g., an editor that
                        // stripped the frontmatter): keep the existing identity.
                        id = found;
                    } else if let Some(found) = self.lookup_alias(&entry.rel_path) {
                        id = found;
                    } else {
                        report.new += 1;
                        id = clock::note_id();
                    }
                }
            }
            if !collision {
                if let Some(prev) = id_to_path.get(&id) {
                    report.collisions.push(Collision {
                        id: id.clone(),
                        paths: vec![prev.clone(), entry.rel_path.clone()],
                    });
                    id = clock::note_id();
                }
            }
            id_to_path.insert(id.clone(), entry.rel_path.clone());
            // Only a file indexed under its own embedded id may skip the read
            // next time; anything else re-runs the full identity logic.
            let store_mtime = mtime.filter(|_| embedded.as_deref() == Some(id.as_str()));
            assignments.push((entry, id, embedded.is_some(), store_mtime, cached));
        }

        // Detach any live note whose path is about to change, so a later
        // re-attach cannot collide with another note's still-unmoved path
        // (e.g. a two-note swap X: A->B, Y: B->A). Tombstoned notes are not in
        // `id_to_path`, so they keep their real path and, thanks to the partial
        // unique index, no longer block a surviving note moving onto them.
        for (id, new_path) in &id_to_path {
            if let Some((old_path, _)) = existing.get(id) {
                if old_path != new_path {
                    self.conn.execute(
                        "UPDATE notes SET path = ?1 WHERE note_id = ?2",
                        params![format!("{RECONCILE_TMP_PREFIX}{id}"), id],
                    )?;
                }
            }
        }

        // Tombstone notes whose files disappeared. Run this BEFORE the upsert
        // pass so a note being tombstoned releases its path (tombstoned=1)
        // before a surviving note is upserted onto that same path; the partial
        // unique index then allows the takeover.
        for (id, (_path, _hash)) in &existing {
            if !id_to_path.contains_key(id) {
                self.conn.execute(
                    "UPDATE notes SET tombstoned = 1, dirty = 0 WHERE note_id = ?1",
                    params![id],
                )?;
                self.delete_fts_for_note(id)?;
                report.tombstones.push(id.clone());
            }
        }

        // Upsert pass (extract + write-back + DB writes)
        let mut processed: Vec<String> = Vec::new();
        for (mut entry, id, had_embedded, store_mtime, cached) in assignments {
            if cached.as_deref() == Some(id.as_str()) {
                // Unchanged file under its unchanged row: nothing to re-index.
                report.updated += 1;
                continue;
            }
            if cached.is_some() {
                // Lost its id to a collision: needs its bytes after all.
                entry = vault
                    .read_note_entry(entry.rel_path, entry.editor)
                    .map_err(rusqlite_err)?;
            }
            // Inject identity if the file lacks one, and refresh the hash so the
            // index's content_hash always matches the bytes actually on disk.
            if !had_embedded {
                if let Some(new_bytes) = vault::inject_id(&entry.bytes, entry.editor, &id) {
                    if new_bytes != entry.bytes {
                        let _ = vault.write_atomic_bulk(&entry.rel_path, &new_bytes);
                        entry.hash = hash::content_hash(&new_bytes);
                        entry.size = new_bytes.len() as u64;
                        entry.bytes = new_bytes;
                    }
                }
                report.adopted += 1;
            }
            let prior = existing.get(&id);
            let is_update = prior.is_some();
            if is_update {
                report.updated += 1;
            }
            // If path changed, record the old path/title as an alias
            if let Some((old_path, _)) = prior {
                if *old_path != entry.rel_path {
                    let old_title = vault::title_from_path(old_path);
                    self.conn.execute(
                        "INSERT OR IGNORE INTO aliases (alias, note_id) VALUES (?1, ?2)",
                        params![old_title, id],
                    )?;
                    self.conn.execute(
                        "INSERT OR IGNORE INTO aliases (alias, note_id) VALUES (?1, ?2)",
                        params![old_path, id],
                    )?;
                }
            }
            // A rewritten (id-injected) file gets a new mtime; read it next time.
            let store_mtime = if had_embedded { store_mtime } else { None };
            self.upsert_note(&id, &entry, store_mtime, &mut report)?;
            processed.push(id);
        }
        let _ = processed;

        // Attachments
        self.reconcile_attachments_with_mtime(&attachments)?;

        // Two-pass link resolution
        self.resolve_all_links()?;

        Ok(report)
    }

    /// Live rows with a stored mtime: path -> (id, content_hash, file_size, mtime_ns).
    fn mtime_cache(&self, id_col: &str, table: &str) -> rusqlite::Result<MtimeCache> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT path, {id_col}, content_hash, file_size, mtime_ns FROM {table}
             WHERE tombstoned = 0 AND mtime_ns IS NOT NULL"
        ))?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as u64, r.get(4)?),
            ))
        })?;
        rows.collect()
    }

    fn lookup_alias(&self, rel_path: &str) -> Option<String> {
        let title = vault::title_from_path(rel_path);
        let norm = normalize_title(&title);
        let mut stmt = self
            .conn
            .prepare("SELECT alias, note_id FROM aliases")
            .ok()?;
        let mut fallback = None;
        let mut rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .ok()?;
        while let Some(Ok((alias, note_id))) = rows.next() {
            if alias == title || alias == rel_path {
                return Some(note_id);
            }
            if normalize_title(&alias) == norm && fallback.is_none() {
                fallback = Some(note_id);
            }
        }
        fallback
    }

    fn upsert_note(
        &mut self,
        id: &str,
        entry: &vault::NoteEntry,
        mtime_ns: Option<i64>,
        report: &mut ReconcileReport,
    ) -> rusqlite::Result<()> {
        let format = entry.editor.as_str();
        let title = vault::title_from_path(&entry.rel_path);

        // Extract (quarantine on failure)
        let doc = match extract::extract(entry.editor, id, &entry.bytes) {
            Ok(d) => Some(d),
            Err(_) => {
                report.quarantined += 1;
                None
            }
        };

        let (created, updated) = match &doc {
            Some(d) => (d.meta.created_at.clone(), d.meta.updated_at.clone()),
            None => (None, None),
        };

        self.conn.execute(
            "INSERT INTO notes (note_id, path, format, title, created_at, updated_at, content_hash, file_size, tombstoned, quarantined, dirty, mtime_ns)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,?9,0,?10)
             ON CONFLICT(note_id) DO UPDATE SET
               path=excluded.path, format=excluded.format, title=excluded.title,
               created_at=excluded.created_at, updated_at=excluded.updated_at,
               content_hash=excluded.content_hash, file_size=excluded.file_size,
               tombstoned=0, quarantined=excluded.quarantined, dirty=0,
               mtime_ns=excluded.mtime_ns",
            params![
                id,
                entry.rel_path,
                format,
                title,
                created,
                updated,
                entry.hash,
                entry.size as i64,
                doc.is_none() as i64,
                // A quarantined note is re-read every time, as before.
                mtime_ns.filter(|_| doc.is_some()),
            ],
        )?;

        // Clear derived data for this note
        self.conn
            .execute("DELETE FROM tags WHERE note_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM links WHERE source_id = ?1", params![id])?;
        self.conn.execute(
            "DELETE FROM attachment_refs WHERE note_id = ?1",
            params![id],
        )?;
        self.conn
            .execute("DELETE FROM chunks WHERE note_id = ?1", params![id])?;
        self.delete_fts_for_note(id)?;

        let doc = match doc {
            Some(d) => d,
            None => return Ok(()),
        };

        // tags
        for tag in &doc.tags {
            self.conn.execute(
                "INSERT OR IGNORE INTO tags (tag, note_id) VALUES (?1, ?2)",
                params![tag, id],
            )?;
        }
        // links (unresolved; resolved in resolve_all_links)
        for link in &doc.out_links {
            self.conn.execute(
                "INSERT OR IGNORE INTO links (source_id, link_text, kind, target_raw, is_attachment)
                 VALUES (?1,?2,?3,?4,?5)",
                params![id, link.text, link.kind.as_str(), link.target, link.is_attachment as i64],
            )?;
        }
        // chunks
        for chunk in &doc.chunks {
            self.conn.execute(
                "INSERT OR IGNORE INTO chunks (chunk_id, note_id, kind, heading_path, body, content_hash)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    chunk.chunk_id,
                    id,
                    chunk.kind,
                    chunk.heading_path,
                    chunk.body,
                    hash::content_hash(chunk.body.as_bytes())
                ],
            )?;
        }
        // FTS
        let body = doc.plain_text.clone();
        let tags_joined = doc.tags.join(" ");
        self.conn.execute(
            "INSERT INTO notes_fts (title, body, tags, note_id) VALUES (?1,?2,?3,?4)",
            params![title, body, tags_joined, id],
        )?;
        let fts_rowid = self.conn.last_insert_rowid();
        self.conn.execute(
            "UPDATE notes SET fts_rowid = ?1 WHERE note_id = ?2",
            params![fts_rowid, id],
        )?;

        Ok(())
    }

    fn delete_fts_for_note(&mut self, id: &str) -> rusqlite::Result<()> {
        let rowid: Option<i64> = self
            .conn
            .query_row(
                "SELECT fts_rowid FROM notes WHERE note_id = ?1",
                params![id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        if let Some(rowid) = rowid {
            self.conn
                .execute("DELETE FROM notes_fts WHERE rowid = ?1", params![rowid])?;
        }
        Ok(())
    }

    /// Tags for a single note (sorted).
    pub fn tags_for(&self, note_id: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM tags WHERE note_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map(params![note_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Distinct tags with counts, ordered by count desc then tag asc.
    pub fn list_tags(&self) -> rusqlite::Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY COUNT(*) DESC, tag ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Note title suggestions for the `[[` autocomplete: `(note_id, title, format)`
    /// for non-tombstoned notes whose title contains `prefix` (case-insensitive).
    pub fn suggest_titles(
        &self,
        prefix: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT note_id, title, format FROM notes WHERE tombstoned = 0 ORDER BY title LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let p = prefix.to_lowercase();
        let mut out = Vec::new();
        for row in rows {
            let (id, title, format) = row?;
            if p.is_empty() || title.to_lowercase().contains(&p) {
                out.push((id, title, format));
            }
        }
        Ok(out)
    }

    /// Re-index a single note (targeted save path) and re-resolve all links.
    pub fn reindex_note(
        &mut self,
        id: &str,
        rel_path: &str,
        editor: crate::extract::EditorKind,
        bytes: &[u8],
    ) -> rusqlite::Result<()> {
        let entry = vault::NoteEntry {
            rel_path: rel_path.to_string(),
            editor,
            bytes: bytes.to_vec(),
            hash: hash::content_hash(bytes),
            size: bytes.len() as u64,
        };
        let mut report = ReconcileReport::default();
        self.upsert_note(id, &entry, None, &mut report)?;
        self.resolve_all_links()?;
        Ok(())
    }

    /// Mark a note as tombstoned without a full scan (delete command).
    pub fn mark_tombstoned(&mut self, note_id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE notes SET tombstoned = 1, dirty = 0 WHERE note_id = ?1",
            params![note_id],
        )?;
        self.delete_fts_for_note(note_id)?;
        Ok(())
    }

    /// Check if a relative path is already occupied by an active note in the index.
    pub fn is_path_taken(
        &self,
        path: &str,
        excluding_note_id: Option<&str>,
    ) -> rusqlite::Result<bool> {
        if let Some(id) = excluding_note_id {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM notes WHERE path = ?1 AND note_id != ?2 AND tombstoned = 0",
                params![path, id],
                |r| r.get(0),
            )?;
            Ok(count > 0)
        } else {
            let count: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM notes WHERE path = ?1 AND tombstoned = 0",
                params![path],
                |r| r.get(0),
            )?;
            Ok(count > 0)
        }
    }

    /// Incremental reconcile_attachments helper (public so `vault_import_attachment` can register a new file).
    pub fn reconcile_attachments(
        &mut self,
        attachments: &[vault::AttachmentEntry],
    ) -> rusqlite::Result<()> {
        let with_mtime: Vec<_> = attachments.iter().map(|a| (a.clone(), None)).collect();
        self.reconcile_attachments_with_mtime(&with_mtime)
    }

    fn reconcile_attachments_with_mtime(
        &mut self,
        attachments: &[(vault::AttachmentEntry, Option<i64>)],
    ) -> rusqlite::Result<()> {
        let mut hash_to_att: HashMap<String, String> = HashMap::new();
        let mut path_of_att: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT att_id, path, content_hash FROM attachments")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (aid, path, h) = row?;
                hash_to_att.insert(h, aid.clone());
                path_of_att.insert(aid, path);
            }
        }

        // Detach any existing attachment whose path is about to change, so a
        // later re-attach cannot collide with another attachment's still-unmoved
        // path (two-attachment swap).
        for (att, _) in attachments {
            if let Some(aid) = hash_to_att.get(&att.hash) {
                if path_of_att
                    .get(aid)
                    .map(|old| old != &att.rel_path)
                    .unwrap_or(false)
                {
                    self.conn.execute(
                        "UPDATE attachments SET path = ?1 WHERE att_id = ?2",
                        params![format!("{RECONCILE_TMP_PREFIX}{aid}"), aid],
                    )?;
                }
            }
        }

        let mut seen: HashMap<String, String> = HashMap::new(); // path -> att_id
        for (att, mtime) in attachments {
            let aid = match hash_to_att.get(&att.hash) {
                Some(a) => {
                    // rename detection: same content, update path
                    self.conn.execute(
                        "UPDATE attachments SET path = ?1, file_size = ?2, mtime_ns = ?3 WHERE att_id = ?4",
                        params![att.rel_path, att.size as i64, mtime, a],
                    )?;
                    a.clone()
                }
                None => {
                    let aid = clock::att_id();
                    self.conn.execute(
                        "INSERT OR IGNORE INTO attachments (att_id, path, content_hash, file_size, tombstoned, mtime_ns)
                         VALUES (?1,?2,?3,?4,0,?5)",
                        params![aid, att.rel_path, att.hash, att.size as i64, mtime],
                    )?;
                    hash_to_att.insert(att.hash.clone(), aid.clone());
                    aid
                }
            };
            seen.insert(att.rel_path.clone(), aid);
        }
        // Tombstone attachments that disappeared
        let mut gone: Vec<String> = Vec::new();
        {
            let mut stmt = self.conn.prepare("SELECT att_id, path FROM attachments")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (aid, path) = row?;
                if !seen.contains_key(&path) {
                    gone.push(aid);
                }
            }
        }
        for aid in gone {
            self.conn.execute(
                "UPDATE attachments SET tombstoned = 1 WHERE att_id = ?1",
                params![aid],
            )?;
        }
        Ok(())
    }

    /// Resolve all links (two-pass) after every note/attachment is indexed.
    fn resolve_all_links(&mut self) -> rusqlite::Result<()> {
        // Build resolution maps
        let mut id_map: HashMap<String, (String, String)> = HashMap::new();
        let mut title_map: HashMap<String, (String, String)> = HashMap::new();
        let mut norm_title_map: HashMap<String, (String, String)> = HashMap::new();
        let mut path_map: HashMap<String, (String, String)> = HashMap::new();
        let mut alias_map: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT note_id, path, title FROM notes WHERE tombstoned = 0")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (id, path, title) = row?;
                id_map
                    .entry(id.clone())
                    .or_insert((id.clone(), path.clone()));
                title_map
                    .entry(title.clone())
                    .or_insert((id.clone(), path.clone()));
                norm_title_map
                    .entry(normalize_title(&title))
                    .or_insert((id.clone(), path.clone()));
                path_map
                    .entry(path.clone())
                    .or_insert((id.clone(), path.clone()));
            }
        }
        {
            let mut stmt = self.conn.prepare("SELECT alias, note_id FROM aliases")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (alias, id) = row?;
                alias_map.entry(normalize_title(&alias)).or_insert(id);
            }
        }
        let mut att_path_map: HashMap<String, String> = HashMap::new();
        let mut att_name_map: HashMap<String, String> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT att_id, path FROM attachments WHERE tombstoned = 0")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (aid, path) = row?;
                att_path_map
                    .entry(path.clone())
                    .or_insert_with(|| aid.clone());
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                att_name_map.entry(name).or_insert(aid);
            }
        }

        // Load all link rows
        let mut links: Vec<(String, String, String, String, bool)> = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT source_id, target_raw, link_text, kind, is_attachment FROM links",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            })?;
            for row in rows {
                links.push(row?);
            }
        }

        self.conn.execute("DELETE FROM attachment_refs", [])?;
        for (source_id, target, link_text, kind, is_attachment) in links {
            let resolved = if is_attachment {
                resolve_attachment(&target, &att_path_map, &att_name_map)
            } else {
                resolve_note_target(
                    &target,
                    &id_map,
                    &path_map,
                    &title_map,
                    &norm_title_map,
                    &alias_map,
                )
            };
            match resolved {
                Some((target_id, target_path)) => {
                    self.conn.execute(
                        "UPDATE links SET target_id = ?1, target_path = ?2
                         WHERE source_id = ?3 AND target_raw = ?4 AND link_text = ?5 AND kind = ?6",
                        params![target_id, target_path, source_id, target, link_text, kind],
                    )?;
                    if is_attachment {
                        self.conn.execute(
                            "INSERT OR IGNORE INTO attachment_refs (note_id, att_id) VALUES (?1, ?2)",
                            params![source_id, target_id],
                        )?;
                    }
                }
                None => {
                    // leave unresolved (target_id stays NULL)
                    let _ = (source_id, target, link_text, kind);
                }
            }
        }
        Ok(())
    }

    // ---------- queries ----------

    pub fn search(
        &self,
        query: &str,
        tag: Option<&str>,
        format: Option<&str>,
    ) -> rusqlite::Result<Vec<SearchHit>> {
        let mut ids: Vec<String> = Vec::new();
        let trimmed = query.trim();
        if trimmed.is_empty() {
            if let Some(t) = tag {
                let mut stmt = self.conn.prepare(
                    "SELECT n.note_id FROM tags t JOIN notes n ON n.note_id = t.note_id
                     WHERE t.tag = ?1 AND n.tombstoned = 0 ORDER BY n.title LIMIT 500",
                )?;
                let rows = stmt.query_map(params![t], |r| r.get::<_, String>(0))?;
                for row in rows {
                    ids.push(row?);
                }
            } else {
                return Ok(Vec::new());
            }
        } else {
            // FTS match (single param), then filter in Rust to avoid dynamic-SQL param juggling.
            let run_fts = |q: &str| -> rusqlite::Result<Vec<String>> {
                let mut stmt = self.conn.prepare(
                    "SELECT note_id FROM notes_fts WHERE notes_fts MATCH ?1 ORDER BY bm25(notes_fts) LIMIT 500",
                )?;
                let rows = stmt.query_map(params![q], |r| r.get::<_, String>(0))?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            };
            match run_fts(trimmed) {
                Ok(found) => ids = found,
                Err(_) => {
                    // Fallback when raw query contains FTS5 punctuation/syntax characters
                    // (e.g. `lorem-ipsum`, `#tag`, unmatched quotes, parentheses).
                    let safe_q = sanitize_fts_query(trimmed);
                    if !safe_q.is_empty() {
                        if let Ok(found) = run_fts(&safe_q) {
                            ids = found;
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        for id in ids {
            let Some(rec) = self.get_note(&id)? else {
                continue;
            };
            if rec.tombstoned || rec.quarantined {
                continue;
            }
            if let Some(f) = format {
                if rec.format != f {
                    continue;
                }
            }
            if let Some(t) = tag {
                let has = self.has_tag(&id, t)?;
                if !has {
                    continue;
                }
            }
            out.push(SearchHit {
                note_id: rec.note_id,
                title: rec.title,
                path: rec.path,
                format: rec.format,
            });
        }
        Ok(out)
    }

    fn has_tag(&self, note_id: &str, tag: &str) -> rusqlite::Result<bool> {
        let n = self.conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE note_id = ?1 AND tag = ?2",
            params![note_id, tag],
            |r| r.get::<_, i64>(0),
        )?;
        Ok(n > 0)
    }

    pub fn get_backlinks(&self, note_id: &str) -> rusqlite::Result<Vec<Backlink>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.source_id, n.title, l.link_text, l.kind
             FROM links l JOIN notes n ON n.note_id = l.source_id
             WHERE l.target_id = ?1 AND n.tombstoned = 0
             ORDER BY n.title, l.link_text",
        )?;
        let rows = stmt.query_map(params![note_id], |r| {
            Ok(Backlink {
                source_id: r.get(0)?,
                source_title: r.get(1)?,
                link_text: r.get(2)?,
                kind: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Plain-text body of a note as indexed in FTS (used for backlink snippets).
    pub fn body_of(&self, note_id: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT body FROM notes_fts WHERE note_id = ?1",
                params![note_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
    }

    pub fn get_outlinks(
        &self,
        note_id: &str,
    ) -> rusqlite::Result<Vec<(String, String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT target_raw, kind, target_id FROM links WHERE source_id = ?1 ORDER BY target_raw",
        )?;
        let rows = stmt.query_map(params![note_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_notes(&self) -> rusqlite::Result<Vec<NoteRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT note_id, path, format, title, created_at, updated_at, content_hash, tombstoned, quarantined
             FROM notes ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(NoteRecord {
                note_id: r.get(0)?,
                path: r.get(1)?,
                format: r.get(2)?,
                title: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
                content_hash: r.get(6)?,
                tombstoned: r.get::<_, i64>(7)? != 0,
                quarantined: r.get::<_, i64>(8)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_note(&self, note_id: &str) -> rusqlite::Result<Option<NoteRecord>> {
        self.conn
            .query_row(
                "SELECT note_id, path, format, title, created_at, updated_at, content_hash, tombstoned, quarantined
                 FROM notes WHERE note_id = ?1",
                params![note_id],
                |r| {
                    Ok(NoteRecord {
                        note_id: r.get(0)?,
                        path: r.get(1)?,
                        format: r.get(2)?,
                        title: r.get(3)?,
                        created_at: r.get(4)?,
                        updated_at: r.get(5)?,
                        content_hash: r.get(6)?,
                        tombstoned: r.get::<_, i64>(7)? != 0,
                        quarantined: r.get::<_, i64>(8)? != 0,
                    })
                },
            )
            .optional()
    }

    /// Resolve a link text to a note id (exposed for tests).
    pub fn resolve_link(&self, target: &str) -> rusqlite::Result<Option<String>> {
        let mut id_map = HashMap::new();
        let mut title_map = HashMap::new();
        let mut norm_map = HashMap::new();
        let mut path_map = HashMap::new();
        let mut alias_map = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT note_id, path, title FROM notes WHERE tombstoned = 0")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (id, path, title) = row?;
                id_map
                    .entry(id.clone())
                    .or_insert((id.clone(), path.clone()));
                title_map
                    .entry(title.clone())
                    .or_insert((id.clone(), path.clone()));
                norm_map
                    .entry(normalize_title(&title))
                    .or_insert((id.clone(), path.clone()));
                path_map
                    .entry(path.clone())
                    .or_insert((id.clone(), path.clone()));
            }
        }
        {
            let mut stmt = self.conn.prepare("SELECT alias, note_id FROM aliases")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (alias, id) = row?;
                alias_map.entry(normalize_title(&alias)).or_insert(id);
            }
        }
        Ok(resolve_note_target(
            target, &id_map, &path_map, &title_map, &norm_map, &alias_map,
        )
        .map(|(id, _)| id))
    }
}

fn resolve_note_target(
    target: &str,
    id_map: &HashMap<String, (String, String)>,
    path_map: &HashMap<String, (String, String)>,
    title_map: &HashMap<String, (String, String)>,
    norm_title_map: &HashMap<String, (String, String)>,
    alias_map: &HashMap<String, String>,
) -> Option<(String, String)> {
    let t = target.split('#').next().unwrap_or(target).trim();
    if t.is_empty() {
        return None;
    }
    if let Some(v) = id_map.get(t) {
        return Some(v.clone());
    }
    if let Some(v) = title_map.get(t) {
        return Some(v.clone());
    }
    if let Some(v) = path_map.get(t) {
        return Some(v.clone());
    }
    if let Some(v) = norm_title_map.get(&normalize_title(t)) {
        return Some(v.clone());
    }
    if let Some(id) = alias_map.get(&normalize_title(t)) {
        // resolve alias to path via title/path maps is not available; return id with empty path
        return Some((id.clone(), String::new()));
    }
    None
}

fn resolve_attachment(
    target: &str,
    path_map: &HashMap<String, String>,
    name_map: &HashMap<String, String>,
) -> Option<(String, String)> {
    let t = target.trim();
    if let Some(aid) = path_map.get(t) {
        return Some((aid.clone(), t.to_string()));
    }
    if let Some(aid) = name_map.get(t) {
        return Some((aid.clone(), t.to_string()));
    }
    let name = t.rsplit('/').next().unwrap_or(t);
    if let Some(aid) = name_map.get(name) {
        return Some((aid.clone(), t.to_string()));
    }
    None
}

fn sanitize_fts_query(raw: &str) -> String {
    let tokens: Vec<String> = raw
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|tok| !tok.is_empty())
        .map(|tok| format!("\"{}\"*", tok))
        .collect();
    tokens.join(" ")
}

fn rusqlite_err<E: std::fmt::Display>(e: E) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e.to_string())))
}

#[cfg(test)]
mod mtime_cache_tests {
    use super::*;
    use crate::vault::SCAN_READS;
    use std::fs;
    use std::time::{Duration, SystemTime};

    /// Scan reads since the last call (thread-local, so parallel tests don't mix).
    fn reads() -> usize {
        SCAN_READS.with(|c| c.replace(0))
    }

    fn set_mtime(p: &Path, t: SystemTime) {
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    fn mtime(p: &Path) -> SystemTime {
        fs::metadata(p).unwrap().modified().unwrap()
    }

    /// Backdate every vault file so its mtime is past the settle window.
    fn age_all(v: &LocalVault) {
        let old = SystemTime::now() - Duration::from_secs(3600);
        let mut stack = vec![v.root.join("notes/markdown"), v.root.join("attachments")];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    set_mtime(&p, old);
                }
            }
        }
    }

    /// A vault with two notes and one attachment, indexed until warm.
    fn warm() -> (tempfile::TempDir, LocalVault, Index) {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        v.write_atomic("notes/markdown/A.md", b"# A\n[[B]]\n")
            .unwrap();
        v.write_atomic("notes/markdown/B.md", b"# B\n#tag\n")
            .unwrap();
        v.write_atomic("attachments/x.bin", &[7u8; 4096]).unwrap();
        let mut idx = Index::open_in_memory().unwrap();
        idx.reconcile(&v).unwrap(); // injects ids (rewrites the notes)
        age_all(&v);
        idx.reconcile(&v).unwrap(); // reads everything once, stores mtimes
        reads();
        (dir, v, idx)
    }

    fn id_at(idx: &Index, table: &str, id_col: &str, path: &str) -> String {
        idx.conn()
            .query_row(
                &format!("SELECT {id_col} FROM {table} WHERE path = ?1 AND tombstoned = 0"),
                params![path],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn hash_at(idx: &Index, path: &str) -> String {
        idx.conn()
            .query_row(
                "SELECT content_hash FROM notes WHERE path = ?1",
                params![path],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn unchanged_vault_reads_nothing_and_reports_the_same() {
        let (_d, v, mut idx) = warm();
        let first = format!("{:?}", idx.reconcile(&v).unwrap());
        assert_eq!(reads(), 0);
        let second = format!("{:?}", idx.reconcile(&v).unwrap());
        assert_eq!(reads(), 0);
        assert_eq!(first, second);
        let b = id_at(&idx, "notes", "note_id", "notes/markdown/B.md");
        assert_eq!(idx.tags_for(&b).unwrap(), vec!["tag"]);

        // Same size and mtime but different bytes: trusted, so not re-read.
        let p = v.root.join("notes/markdown/A.md");
        let (t, before) = (mtime(&p), hash_at(&idx, "notes/markdown/A.md"));
        let mut bytes = fs::read(&p).unwrap();
        *bytes.last_mut().unwrap() = b'\t';
        fs::write(&p, &bytes).unwrap();
        set_mtime(&p, t);
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 0);
        assert_eq!(hash_at(&idx, "notes/markdown/A.md"), before);
    }

    #[test]
    fn edited_note_is_reindexed() {
        let (_d, v, mut idx) = warm();
        let p = v.root.join("notes/markdown/B.md");
        let before = hash_at(&idx, "notes/markdown/B.md");
        let id = id_at(&idx, "notes", "note_id", "notes/markdown/B.md");
        let mut bytes = fs::read(&p).unwrap();
        bytes.extend_from_slice(b"\nmore #other\n");
        fs::write(&p, &bytes).unwrap();
        set_mtime(&p, SystemTime::now() - Duration::from_secs(60));
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 1);
        assert_ne!(hash_at(&idx, "notes/markdown/B.md"), before);
        assert_eq!(id_at(&idx, "notes", "note_id", "notes/markdown/B.md"), id);
        assert_eq!(idx.tags_for(&id).unwrap(), vec!["other", "tag"]);
    }

    #[test]
    fn recently_modified_file_is_read_until_it_settles() {
        let (_d, v, mut idx) = warm();
        let p = v.root.join("notes/markdown/A.md");
        set_mtime(&p, SystemTime::now());
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 1);
        idx.reconcile(&v).unwrap();
        assert_eq!(
            reads(),
            1,
            "an mtime inside the settle window is never trusted"
        );
        set_mtime(&p, SystemTime::now() - Duration::from_secs(60));
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 1);
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 0);
    }

    #[test]
    fn moved_note_and_attachment_keep_identity() {
        let (_d, v, mut idx) = warm();
        let note = id_at(&idx, "notes", "note_id", "notes/markdown/A.md");
        let att = id_at(&idx, "attachments", "att_id", "attachments/x.bin");
        fs::create_dir_all(v.root.join("notes/markdown/sub")).unwrap();
        fs::rename(
            v.root.join("notes/markdown/A.md"),
            v.root.join("notes/markdown/sub/A2.md"),
        )
        .unwrap();
        fs::rename(
            v.root.join("attachments/x.bin"),
            v.root.join("attachments/y.bin"),
        )
        .unwrap();
        let report = idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 2);
        assert!(report.tombstones.is_empty());
        assert_eq!(
            id_at(&idx, "notes", "note_id", "notes/markdown/sub/A2.md"),
            note
        );
        assert_eq!(
            id_at(&idx, "attachments", "att_id", "attachments/y.bin"),
            att
        );
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 0);
    }

    #[test]
    fn deleted_note_is_tombstoned() {
        let (_d, v, mut idx) = warm();
        let id = id_at(&idx, "notes", "note_id", "notes/markdown/B.md");
        fs::remove_file(v.root.join("notes/markdown/B.md")).unwrap();
        let report = idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 0);
        assert_eq!(report.tombstones, vec![id.clone()]);
        assert!(idx.get_note(&id).unwrap().unwrap().tombstoned);
    }

    #[test]
    fn index_without_mtime_column_migrates_and_reads_once() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        v.write_atomic("notes/markdown/A.md", b"# A\n").unwrap();
        v.write_atomic("attachments/x.bin", b"x").unwrap();
        let db = dir.path().join("old.db");
        {
            let mut idx = Index::open(&db).unwrap();
            idx.reconcile(&v).unwrap();
            idx.conn()
                .execute_batch(
                    "ALTER TABLE notes DROP COLUMN mtime_ns;
                     ALTER TABLE attachments DROP COLUMN mtime_ns;",
                )
                .unwrap();
        }
        age_all(&v);
        drop(Index::open(&db).unwrap());
        let mut idx = Index::open(&db).unwrap(); // reopening is idempotent
        reads();
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 2);
        idx.reconcile(&v).unwrap();
        assert_eq!(reads(), 0);
    }

    /// `cargo test -p notium-core --release --lib reconcile_benchmark -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reconcile_benchmark() {
        let dir = tempfile::tempdir().unwrap();
        let v = LocalVault::new(dir.path());
        v.init().unwrap();
        let filler = "lorem ipsum ".repeat(80);
        for i in 0..2000 {
            let body = format!(
                "# Note {i}\n[[Note {}]] #t{}\n{filler}\n",
                (i + 1) % 2000,
                i % 50
            );
            v.write_atomic_bulk(&format!("notes/markdown/Note {i}.md"), body.as_bytes())
                .unwrap();
        }
        for i in 0..40u8 {
            v.write_atomic_bulk(&format!("attachments/photo{i}.jpg"), &vec![i; 4 << 20])
                .unwrap();
        }
        let mut idx = Index::open(&dir.path().join("bench.db")).unwrap();
        idx.reconcile(&v).unwrap();
        age_all(&v);
        let t = std::time::Instant::now();
        idx.reconcile(&v).unwrap();
        let full = t.elapsed();
        reads();
        let t = std::time::Instant::now();
        idx.reconcile(&v).unwrap();
        let cached = t.elapsed();
        assert_eq!(reads(), 0);
        println!("2000 notes + 160 MB attachments: full read {full:?}, mtime cache {cached:?}");
    }
}
