//! Workstream 3 acceptance tests (F1–F6): FTS5 derived index.

use notium_core::index::Index;
use notium_core::vault::LocalVault;

fn setup() -> (tempfile::TempDir, LocalVault) {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    (dir, vault)
}

#[test]
fn f1_build_and_fts() {
    let (_d, vault) = setup();
    for i in 0..20 {
        let content = format!("# Note {i}\n\nUnique keyword{i} body.\n");
        vault
            .write_atomic(&format!("notes/markdown/N{i}.md"), content.as_bytes())
            .unwrap();
    }
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();
    assert_eq!(index.list_notes().unwrap().len(), 20);
    let hits = index.search("keyword7", None, None).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "N7");
}

#[test]
fn f2_accent_insensitive_search() {
    let (_d, vault) = setup();
    let content = "# Cancion\n\nLa canci\u{f3}n de cuna en ingl\u{e9}s.\n";
    vault
        .write_atomic("notes/markdown/Cancion.md", content.as_bytes())
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();
    assert!(!index.search("cancion", None, None).unwrap().is_empty());
    assert!(!index.search("ingles", None, None).unwrap().is_empty());
}

#[test]
fn f3_tag_and_format_filters() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\n#work stuff\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\nother stuff #personal\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let md = index.search("stuff", None, Some("markdown")).unwrap();
    assert_eq!(md.len(), 2);
    assert_eq!(md[0].format, "markdown");

    let tagged = index.search("stuff", Some("work"), None).unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].title, "A");
}

#[test]
fn f4_incremental_update() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\nold\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\nunchanged\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\nnew content\n")
        .unwrap();
    index.reconcile(&vault).unwrap();

    assert!(!index.search("new", None, None).unwrap().is_empty());
    assert!(index.search("old", None, None).unwrap().is_empty());
    assert!(!index.search("unchanged", None, None).unwrap().is_empty());
}

#[test]
fn f5_rebuild_equals_incremental() {
    let (_d, vault) = setup();
    for i in 0..10 {
        vault
            .write_atomic(
                &format!("notes/markdown/N{i}.md"),
                format!("# N{i}\n\nbody {i}\n").as_bytes(),
            )
            .unwrap();
    }
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();
    let full = index.list_notes().unwrap();

    index.reconcile(&vault).unwrap();
    let inc = index.list_notes().unwrap();
    assert_eq!(full, inc);
}

#[test]
fn f6_rebuild_10k_perf() {
    let (_d, vault) = setup();
    for i in 0..10_000 {
        vault
            .write_atomic(
                &format!("notes/markdown/N{i:05}.md"),
                format!("# N{i}\n\nbody text for note {i}\n").as_bytes(),
            )
            .unwrap();
    }
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    let start = std::time::Instant::now();
    index.rebuild(&vault).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(index.list_notes().unwrap().len(), 10_000);
    eprintln!("[metric] 10k rebuild: {elapsed:?}");
    assert!(elapsed.as_secs() < 30, "10k rebuild too slow: {elapsed:?}");
}

#[test]
fn f7_attachment_refs_populated_on_reconcile() {
    let (_d, vault) = setup();
    vault
        .write_atomic("attachments/photo.jpg", b"jpeg-bytes")
        .unwrap();
    vault
        .write_atomic(
            "notes/markdown/A.md",
            b"# A\n\nEmbed: ![[attachments/photo.jpg]]\n",
        )
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let count: i64 = index
        .conn()
        .query_row("SELECT COUNT(*) FROM attachment_refs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        count, 1,
        "attachment_refs table must record resolved attachment embeds"
    );
}

// ---------------------------------------------------------------------------
// Path-swap / collision regressions (notes.path and attachments.path).
// These used to fail with `UNIQUE constraint failed: notes.path` (or
// attachments.path) and abort `open_vault`, leaving the vault closed.
// ---------------------------------------------------------------------------

#[test]
fn reconcile_handles_path_swap() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\ncontent A\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\ncontent B\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let before = index.list_notes().unwrap();
    let id_a = before
        .iter()
        .find(|n| n.path == "notes/markdown/A.md")
        .unwrap()
        .note_id
        .clone();
    let id_b = before
        .iter()
        .find(|n| n.path == "notes/markdown/B.md")
        .unwrap()
        .note_id
        .clone();

    // Swap the file bytes: each file now carries the other note's embedded id.
    let bytes_a = vault.read("notes/markdown/A.md").unwrap();
    let bytes_b = vault.read("notes/markdown/B.md").unwrap();
    vault.write_atomic("notes/markdown/A.md", &bytes_b).unwrap();
    vault.write_atomic("notes/markdown/B.md", &bytes_a).unwrap();

    index.reconcile(&vault).unwrap();

    assert_eq!(
        index.get_note(&id_a).unwrap().unwrap().path,
        "notes/markdown/B.md"
    );
    assert_eq!(
        index.get_note(&id_b).unwrap().unwrap().path,
        "notes/markdown/A.md"
    );
    // No synthetic sentinel path may remain.
    assert!(index
        .list_notes()
        .unwrap()
        .iter()
        .all(|n| !n.path.contains("__reconcile__")));
}

#[test]
fn reconcile_handles_three_way_cycle() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\ncontent A\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\ncontent B\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/C.md", b"# C\n\ncontent C\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let before = index.list_notes().unwrap();
    let id_a = before
        .iter()
        .find(|n| n.path == "notes/markdown/A.md")
        .unwrap()
        .note_id
        .clone();
    let id_b = before
        .iter()
        .find(|n| n.path == "notes/markdown/B.md")
        .unwrap()
        .note_id
        .clone();
    let id_c = before
        .iter()
        .find(|n| n.path == "notes/markdown/C.md")
        .unwrap()
        .note_id
        .clone();

    // Rotate A->B, B->C, C->A.
    let (ba, bb, bc) = (
        vault.read("notes/markdown/A.md").unwrap(),
        vault.read("notes/markdown/B.md").unwrap(),
        vault.read("notes/markdown/C.md").unwrap(),
    );
    vault.write_atomic("notes/markdown/A.md", &bc).unwrap();
    vault.write_atomic("notes/markdown/B.md", &ba).unwrap();
    vault.write_atomic("notes/markdown/C.md", &bb).unwrap();

    index.reconcile(&vault).unwrap();

    assert_eq!(
        index.get_note(&id_a).unwrap().unwrap().path,
        "notes/markdown/B.md"
    );
    assert_eq!(
        index.get_note(&id_b).unwrap().unwrap().path,
        "notes/markdown/C.md"
    );
    assert_eq!(
        index.get_note(&id_c).unwrap().unwrap().path,
        "notes/markdown/A.md"
    );
}

#[test]
fn reconcile_moved_note_takes_over_tombstoned_path() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\ncontent A\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\ncontent B\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let id_a = index
        .list_notes()
        .unwrap()
        .iter()
        .find(|n| n.path == "notes/markdown/A.md")
        .unwrap()
        .note_id
        .clone();

    // Delete A: file moves to trash, the index keeps a tombstone at its path.
    vault.delete_to_trash("notes/markdown/A.md").unwrap();
    index.mark_tombstoned(&id_a).unwrap();

    // Rename B onto A's now-free path (what rename_impl does).
    let new_path = vault.rename_note("notes/markdown/B.md", "A").unwrap();
    assert_eq!(new_path, "notes/markdown/A.md");

    index.reconcile(&vault).unwrap();

    // The tombstoned note keeps its real path and does not block the mover.
    let a = index.get_note(&id_a).unwrap().unwrap();
    assert!(a.tombstoned);
    assert_eq!(a.path, "notes/markdown/A.md");
}

#[test]
fn reconcile_note_takes_over_path_tombstoned_in_same_pass() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\ncontent A\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\ncontent B\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let id_a = index
        .list_notes()
        .unwrap()
        .iter()
        .find(|n| n.path == "notes/markdown/A.md")
        .unwrap()
        .note_id
        .clone();

    // Delete A's file and rename B onto A's path within the same window, with
    // no intervening reconcile: A is tombstoned and B takes over A.md in a
    // single reconcile pass.
    vault.delete_to_trash("notes/markdown/A.md").unwrap();
    let new_path = vault.rename_note("notes/markdown/B.md", "A").unwrap();
    assert_eq!(new_path, "notes/markdown/A.md");

    // Previously failed: the note being tombstoned still held A.md while B was
    // upserted onto it.
    index.reconcile(&vault).unwrap();

    let a = index.get_note(&id_a).unwrap().unwrap();
    assert!(a.tombstoned);

    let live_at_a = index
        .list_notes()
        .unwrap()
        .into_iter()
        .find(|n| n.path == "notes/markdown/A.md" && !n.tombstoned)
        .expect("a live note should occupy A.md");
    assert_ne!(live_at_a.note_id, id_a);
}

#[test]
fn delete_then_create_same_title_does_not_collide() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/Foo.md", b"# Foo\n\nold\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let id_old = index
        .list_notes()
        .unwrap()
        .iter()
        .find(|n| n.path == "notes/markdown/Foo.md")
        .unwrap()
        .note_id
        .clone();

    // Delete (file to trash, tombstone kept).
    vault.delete_to_trash("notes/markdown/Foo.md").unwrap();
    index.mark_tombstoned(&id_old).unwrap();

    // Recreate the same title, as create_impl does (create_note_in + reindex_note).
    let id_new = notium_core::clock::note_id();
    let rel_path = vault
        .create_note_in(
            notium_core::extract::EditorKind::Markdown,
            "Foo",
            &id_new,
            "",
        )
        .unwrap();
    assert_eq!(rel_path, "notes/markdown/Foo.md");
    let bytes = vault.read(&rel_path).unwrap();
    index
        .reindex_note(
            &id_new,
            &rel_path,
            notium_core::extract::EditorKind::Markdown,
            &bytes,
        )
        .unwrap();

    assert_eq!(
        index.get_note(&id_new).unwrap().unwrap().path,
        "notes/markdown/Foo.md"
    );
}

#[test]
fn reconcile_is_idempotent_after_swap() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\ncontent A\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/B.md", b"# B\n\ncontent B\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    let bytes_a = vault.read("notes/markdown/A.md").unwrap();
    let bytes_b = vault.read("notes/markdown/B.md").unwrap();
    vault.write_atomic("notes/markdown/A.md", &bytes_b).unwrap();
    vault.write_atomic("notes/markdown/B.md", &bytes_a).unwrap();

    index.reconcile(&vault).unwrap();
    let first = index.list_notes().unwrap();
    assert!(first.iter().all(|n| !n.path.contains("__reconcile__")));

    // A second, no-op reconcile must not churn the index.
    index.reconcile(&vault).unwrap();
    let second = index.list_notes().unwrap();
    assert_eq!(first, second);
}

#[test]
fn reconcile_handles_attachment_swap() {
    let (_d, vault) = setup();
    vault
        .write_atomic("attachments/a.png", b"png-bytes-AAA")
        .unwrap();
    vault
        .write_atomic("attachments/b.png", b"png-bytes-BBB")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.rebuild(&vault).unwrap();

    // Swap the attachment bytes.
    let bytes_a = vault.read("attachments/a.png").unwrap();
    let bytes_b = vault.read("attachments/b.png").unwrap();
    vault.write_atomic("attachments/a.png", &bytes_b).unwrap();
    vault.write_atomic("attachments/b.png", &bytes_a).unwrap();

    index.reconcile(&vault).unwrap();

    let live: i64 = index
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE tombstoned = 0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(live, 2);
    let sentinel_count: i64 = index
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE path LIKE '__reconcile__/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sentinel_count, 0);
}

#[test]
fn migration_rebuilds_legacy_unique_and_format_check() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("index.db");

    // Build a legacy database as it existed before the richtext removal: a
    // column-level UNIQUE on `path` and a format CHECK allowing 'richtext'.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE notes (
               note_id     TEXT PRIMARY KEY,
               path        TEXT NOT NULL UNIQUE,
               format      TEXT NOT NULL CHECK (format IN ('markdown','richtext')),
               title       TEXT NOT NULL,
               created_at  TEXT,
               updated_at  TEXT,
               content_hash TEXT NOT NULL,
               file_size   INTEGER NOT NULL DEFAULT 0,
               tombstoned  INTEGER NOT NULL DEFAULT 0,
               quarantined INTEGER NOT NULL DEFAULT 0,
               dirty       INTEGER NOT NULL DEFAULT 1,
               fts_rowid   INTEGER
             );
             CREATE TABLE attachments (
               att_id       TEXT PRIMARY KEY,
               path         TEXT NOT NULL UNIQUE,
               content_hash TEXT NOT NULL,
               file_size    INTEGER NOT NULL DEFAULT 0,
               tombstoned   INTEGER NOT NULL DEFAULT 0
             );
             INSERT INTO notes (note_id, path, format, title, content_hash)
               VALUES ('n_md', 'notes/markdown/Foo.md', 'markdown', 'Foo', 'h1');
             INSERT INTO notes (note_id, path, format, title, content_hash)
               VALUES ('n_rt', 'notes/markdown/Bar.json', 'richtext', 'Bar', 'h2');
             INSERT INTO attachments (att_id, path, content_hash, file_size)
               VALUES ('a1', 'attachments/x.png', 'h3', 4);",
        )
        .unwrap();
    }

    // Opening migrates the legacy schema in place, preserving rows.
    let index = Index::open(&db_path).unwrap();

    // The column-level UNIQUE is gone (no origin='u' autoindexes remain).
    let legacy: i64 = index
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pragma_index_list('notes') WHERE origin = 'u'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(legacy, 0);

    // Both rows survived, including the legacy 'richtext' note.
    assert_eq!(
        index.get_note("n_md").unwrap().unwrap().path,
        "notes/markdown/Foo.md"
    );
    assert_eq!(
        index.get_note("n_rt").unwrap().unwrap().path,
        "notes/markdown/Bar.json"
    );

    // The partial unique index now exists.
    let partial: i64 = index
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='uq_notes_live_path'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(partial, 1);

    // The attachment survived too.
    let atts: i64 = index
        .conn()
        .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(atts, 1);

    // user_version is bumped so the migration does not re-run.
    let version: i64 = index
        .conn()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 1);
}
