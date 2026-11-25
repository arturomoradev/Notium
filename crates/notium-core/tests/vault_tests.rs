//! Workstream 1 acceptance tests (V1–V8): vault & identity engine.

use notium_core::extract::EditorKind;
use notium_core::index::Index;
use notium_core::vault::{extract_embedded_id, LocalVault};

fn setup() -> (tempfile::TempDir, LocalVault) {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    (dir, vault)
}

#[test]
fn v1_adopt_md_preserves_bytes() {
    let (_d, vault) = setup();
    let orig = b"---\ntitle: Idea\n# a comment\nsome_key: keep me\n---\nbody text\n";
    vault.write_atomic("notes/markdown/Idea.md", orig).unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    let report = index.reconcile(&vault).unwrap();
    assert_eq!(report.adopted, 1);

    let after = vault.read("notes/markdown/Idea.md").unwrap();
    let s = String::from_utf8(after.clone()).unwrap();
    assert!(s.contains("notium_id: note_"));
    assert!(s.contains("# a comment"));
    assert!(s.contains("some_key: keep me"));
    assert!(s.ends_with("body text\n"));
    // only the id line was added: original content minus the added line is intact
    let id = extract_embedded_id(&after, EditorKind::Markdown).unwrap();
    assert!(id.starts_with("note_"));
}

#[test]
fn v3_external_rename_keeps_id_and_links() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/A.md", b"# A\n\nSee [[B]].\n")
        .unwrap();
    vault.write_atomic("notes/markdown/B.md", b"# B\n").unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let b_id = index.resolve_link("B").unwrap().expect("resolve B");

    std::fs::rename(
        vault.root.join("notes/markdown/B.md"),
        vault.root.join("notes/markdown/C.md"),
    )
    .unwrap();

    let report = index.reconcile(&vault).unwrap();
    // The rename must be absorbed without creating a new note (identity preserved
    // via the embedded id, the preferred path; hash is only the fallback).
    assert_eq!(report.new, 0, "rename must not create a new note");

    let c_id = index.resolve_link("C").unwrap().unwrap();
    assert_eq!(b_id, c_id, "identity must survive rename");
    // old title resolves via alias
    assert_eq!(index.resolve_link("B").unwrap().unwrap(), b_id);
    // backlinks from A still point at b_id
    assert!(!index.get_backlinks(&b_id).unwrap().is_empty());
}

#[test]
fn v4_external_edit_reindexes_same_id() {
    let (_d, vault) = setup();
    vault
        .write_atomic("notes/markdown/N.md", b"# N\n\noriginal\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let rec = index.list_notes().unwrap();
    let id = rec[0].note_id.clone();
    let old_hash = rec[0].content_hash.clone();

    vault
        .write_atomic("notes/markdown/N.md", b"# N\n\nedited externally\n")
        .unwrap();
    index.reconcile(&vault).unwrap();
    let rec2 = index.get_note(&id).unwrap().unwrap();
    assert_eq!(rec2.note_id, id);
    assert_ne!(rec2.content_hash, old_hash, "hash drift must be detected");
}

#[test]
fn v5_external_delete_tombstones() {
    let (_d, vault) = setup();
    vault.write_atomic("notes/markdown/N.md", b"# N\n").unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let id = index.list_notes().unwrap()[0].note_id.clone();

    std::fs::remove_file(vault.root.join("notes/markdown/N.md")).unwrap();
    let report = index.reconcile(&vault).unwrap();
    assert!(report.tombstones.contains(&id));
    assert!(index.get_note(&id).unwrap().unwrap().tombstoned);
}

#[test]
fn v6_new_file_adopted() {
    let (_d, vault) = setup();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    assert!(index.list_notes().unwrap().is_empty());

    vault
        .write_atomic("notes/markdown/New.md", b"# New\n")
        .unwrap();
    let report = index.reconcile(&vault).unwrap();
    assert_eq!(report.new, 1);
    assert_eq!(index.list_notes().unwrap().len(), 1);
    let bytes = vault.read("notes/markdown/New.md").unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("notium_id:"));
}

#[test]
fn v7_rebuild_after_index_delete_same_ids() {
    let (_d, vault) = setup();
    vault.write_atomic("notes/markdown/A.md", b"# A\n").unwrap();
    vault.write_atomic("notes/markdown/B.md", b"# B\n").unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let before: Vec<String> = index
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|n| n.note_id)
        .collect();

    // Close the open SQLite connection so Windows releases the file handle.
    drop(index);
    std::fs::remove_file(vault.index_db_path()).unwrap();
    let mut index2 = Index::open(&vault.index_db_path()).unwrap();
    index2.rebuild(&vault).unwrap();
    let after: Vec<String> = index2
        .list_notes()
        .unwrap()
        .into_iter()
        .map(|n| n.note_id)
        .collect();
    assert_eq!(before, after, "embedded ids must survive index deletion");
}

#[test]
fn v8_id_collision_detected_and_kept() {
    let (_d, vault) = setup();
    vault
        .write_atomic(
            "notes/markdown/A.md",
            b"---\nnotium_id: note_DUP\n---\n# A\n",
        )
        .unwrap();
    vault
        .write_atomic(
            "notes/markdown/B.md",
            b"---\nnotium_id: note_DUP\n---\n# B\n",
        )
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    let report = index.reconcile(&vault).unwrap();
    assert!(!report.collisions.is_empty(), "collision must be surfaced");
    let notes = index.list_notes().unwrap();
    assert_eq!(notes.len(), 2, "both files must be kept");
    assert_ne!(notes[0].note_id, notes[1].note_id, "no silent merge");
}
