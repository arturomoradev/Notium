-- SQLite schema for Cloudflare D1 (notium-db)
CREATE TABLE IF NOT EXISTS journal (
  vault_id TEXT NOT NULL DEFAULT 'default',
  op_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  note_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_rev INTEGER NOT NULL,
  new_rev INTEGER NOT NULL,
  content BLOB,
  hlc_ts INTEGER NOT NULL,
  hlc_cnt INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  path TEXT,
  purge INTEGER NOT NULL DEFAULT 0,
  base_hash TEXT,
  -- Arrival order shared with blobs: the /sync/pull cursor.
  seq INTEGER,
  PRIMARY KEY (vault_id, op_id)
);

CREATE TABLE IF NOT EXISTS blobs (
  vault_id TEXT NOT NULL DEFAULT 'default',
  hash TEXT NOT NULL,
  path TEXT NOT NULL,
  size INTEGER NOT NULL DEFAULT 0,
  data BLOB NOT NULL,
  ts INTEGER NOT NULL,
  seq INTEGER,
  PRIMARY KEY (vault_id, hash, path)
);

CREATE INDEX IF NOT EXISTS idx_journal_vault_ts ON journal(vault_id, ts);
CREATE INDEX IF NOT EXISTS idx_blobs_vault_ts ON blobs(vault_id, ts);
CREATE INDEX IF NOT EXISTS idx_journal_seq ON journal(seq);
CREATE INDEX IF NOT EXISTS idx_blobs_seq ON blobs(seq);
CREATE INDEX IF NOT EXISTS idx_journal_vault_seq ON journal(vault_id, seq);
CREATE INDEX IF NOT EXISTS idx_blobs_vault_seq ON blobs(vault_id, seq);
