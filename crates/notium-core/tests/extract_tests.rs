//! Workstream 2 acceptance tests (X1–X7): headless dual extractor.

use std::path::{Path, PathBuf};

use notium_core::extract::{self, EditorKind};
use notium_core::index::Index;
use notium_core::vault::LocalVault;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(sub: &str, name: &str) -> PathBuf {
    fixtures_dir().join(sub).join(name)
}

#[test]
fn x1_golden_snapshots() {
    let expected_dir = fixtures_dir().join("expected");
    let dir = fixtures_dir().join("notes");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    files.sort();
    for path in files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let doc = extract::extract(EditorKind::Tiptap, &format!("note_{stem}"), &bytes).unwrap();
        let actual = serde_json::to_value(&doc).unwrap();
        let expected: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(expected_dir.join(format!("{stem}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(actual, expected, "golden snapshot mismatch for {stem}");
    }
}

#[test]
fn x2_chunk_ids_deterministic() {
    let bytes = std::fs::read(fixture("notes", "basic.json")).unwrap();
    let a = extract::extract(EditorKind::Tiptap, "note_x", &bytes).unwrap();
    let b = extract::extract(EditorKind::Tiptap, "note_x", &bytes).unwrap();
    assert_eq!(a.chunks, b.chunks);
    let c = extract::extract(EditorKind::Tiptap, "note_x", &bytes).unwrap();
    assert_eq!(a.chunks, c.chunks);
}

#[test]
fn x3_bidirectional_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic(
            "notes/tiptap/A.json",
            notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                b"---\nnotium_id: note_a\n---\n# A\n\nSee [[Proyecto]].\n".as_ref(),
            ))
            .as_bytes(),
        )
        .unwrap();
    vault
        .write_atomic(
            "notes/tiptap/Proyecto.json",
            notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                b"---\nnotium_id: note_proj\n---\n# Proyecto\n\nSee [[A]].\n".as_ref(),
            ))
            .as_bytes(),
        )
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();

    let backlinks_of_proj = index.get_backlinks("note_proj").unwrap();
    assert_eq!(backlinks_of_proj.len(), 1);
    assert_eq!(backlinks_of_proj[0].source_id, "note_a");

    let backlinks_of_a = index.get_backlinks("note_a").unwrap();
    assert_eq!(backlinks_of_a.len(), 1);
    assert_eq!(backlinks_of_a[0].source_id, "note_proj");
}

#[test]
fn x4_rename_alias_resolves_old_link() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic(
            "notes/tiptap/Link.json",
            notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                b"# Link\n\n[[Idea]]\n".as_ref(),
            ))
            .as_bytes(),
        )
        .unwrap();
    vault
        .write_atomic(
            "notes/tiptap/Idea.json",
            notium_core::envelope::note_from_text(&String::from_utf8_lossy(b"# Idea\n".as_ref()))
                .as_bytes(),
        )
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();

    std::fs::rename(
        vault.root.join("notes/tiptap/Idea.json"),
        vault.root.join("notes/tiptap/Concept.json"),
    )
    .unwrap();
    index.reconcile(&vault).unwrap();
    // old wikilink target still resolves via alias
    let resolved = index.resolve_link("Idea").unwrap().unwrap();
    assert!(index
        .get_backlinks(&resolved)
        .unwrap()
        .iter()
        .any(|b| b.link_text == "Idea"));
}

#[test]
fn x5_unresolved_links() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic(
            "notes/tiptap/A.json",
            notium_core::envelope::note_from_text(&String::from_utf8_lossy(
                b"# A\n\n[[DoesNotExist]]\n".as_ref(),
            ))
            .as_bytes(),
        )
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let a_id = index.list_notes().unwrap()[0].note_id.clone();
    let outlinks = index.get_outlinks(&a_id).unwrap();
    assert_eq!(outlinks.len(), 1);
    assert!(
        outlinks[0].2.is_none(),
        "unresolved link must have NULL target"
    );
}

#[test]
fn x6_malformed_input_quarantined() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic("notes/tiptap/Bad.json", &[0xff, 0xfe, 0x00])
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    let report = index.reconcile(&vault).unwrap();
    assert_eq!(report.quarantined, 1);
    let notes = index.list_notes().unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].quarantined);
}

#[test]
fn x7_big_note_extract_perf() {
    let mut s = String::from("# Big\n\n");
    for i in 0..2000 {
        s.push_str(&format!(
            "Paragraph {i} with [[link{i}]] and #tag{i} text.\n\n"
        ));
    }
    let s = notium_core::envelope::note_from_text(&s);
    let start = std::time::Instant::now();
    let doc = extract::extract(EditorKind::Tiptap, "note_big", s.as_bytes()).unwrap();
    let elapsed = start.elapsed();
    assert!(doc.chunks.len() > 1000);
    eprintln!(
        "[metric] big-note extract ({n} lines): {elapsed:?}",
        n = 2000
    );
    // 50 ms is the release target; debug builds are allowed more slack.
    if cfg!(debug_assertions) {
        assert!(
            elapsed.as_millis() < 2000,
            "extract too slow (debug): {elapsed:?}"
        );
    } else {
        assert!(
            elapsed.as_millis() < 50,
            "extract too slow (release): {elapsed:?}"
        );
    }
}
