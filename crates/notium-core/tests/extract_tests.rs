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
    let dir = fixtures_dir().join("md");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "md").unwrap_or(false))
        .collect();
    files.sort();
    for path in files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let doc = extract::extract(EditorKind::Markdown, &format!("note_{stem}"), &bytes).unwrap();
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
    let bytes = std::fs::read(fixture("md", "basic.md")).unwrap();
    let a = extract::extract(EditorKind::Markdown, "note_x", &bytes).unwrap();
    let b = extract::extract(EditorKind::Markdown, "note_x", &bytes).unwrap();
    assert_eq!(a.chunks, b.chunks);
    let c = extract::extract(EditorKind::Markdown, "note_x", &bytes).unwrap();
    assert_eq!(a.chunks, c.chunks);
}

#[test]
fn x3_bidirectional_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic(
            "notes/markdown/A.md",
            b"---\nnotium_id: note_a\n---\n# A\n\nSee [[Proyecto]].\n",
        )
        .unwrap();
    vault
        .write_atomic(
            "notes/markdown/Proyecto.md",
            b"---\nnotium_id: note_proj\n---\n# Proyecto\n\nSee [[A]].\n",
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
        .write_atomic("notes/markdown/Link.md", b"# Link\n\n[[Idea]]\n")
        .unwrap();
    vault
        .write_atomic("notes/markdown/Idea.md", b"# Idea\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();

    std::fs::rename(
        vault.root.join("notes/markdown/Idea.md"),
        vault.root.join("notes/markdown/Concept.md"),
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
        .write_atomic("notes/markdown/A.md", b"# A\n\n[[DoesNotExist]]\n")
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
        .write_atomic("notes/markdown/Bad.md", &[0xff, 0xfe, 0x00])
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
    let start = std::time::Instant::now();
    let doc = extract::extract(EditorKind::Markdown, "note_big", s.as_bytes()).unwrap();
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
