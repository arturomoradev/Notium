//! Workstreams 3 & 4 acceptance tests (S1–S12): sync engine, conflict
//! resolution, and the deterministic chaos simulator.

use notium_core::clock::Hlc;
use notium_core::index::Index;
use notium_core::sim::{self, NoteSpec, SimConfig};
use notium_core::sync::merge;
use notium_core::sync::{self, apply_remote, local_update_op, ApplyOutcome, SyncDb};
use notium_core::vault::LocalVault;

fn device() -> (tempfile::TempDir, LocalVault, SyncDb) {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    let db = SyncDb::open(&vault.sync_db_path()).unwrap();
    (dir, vault, db)
}

fn sample_notes() -> Vec<NoteSpec> {
    vec![
        NoteSpec {
            note_id: "note_md_1".into(),
            format: "markdown".into(),
            path: "notes/markdown/Doc.md".into(),
            initial: b"# Doc\n\nline one\nline two\nline three\n".to_vec(),
        },
        NoteSpec {
            note_id: "note_md_2".into(),
            format: "markdown".into(),
            path: "notes/markdown/Proyecto.md".into(),
            initial: b"# Proyecto\n\nhola\n".to_vec(),
        },
    ]
}

#[test]
fn s1_fast_forward() {
    let (_d, vault, mut db) = device();
    vault.write_atomic("notes/markdown/A.md", b"v1").unwrap();
    db.set_current("note_a", 0, Some(b"v1")).unwrap();

    let op = local_update_op(
        "devB",
        "note_a",
        0,
        b"v2".to_vec(),
        Hlc::origin().tick(1),
        1,
    );
    let out = apply_remote(&mut db, &vault, &op, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(out, ApplyOutcome::FastForward);
    assert_eq!(vault.read("notes/markdown/A.md").unwrap(), b"v2");
    assert_eq!(db.get_current("note_a").unwrap().unwrap().0, 1);
}

#[test]
fn s2_md_disjoint_merge() {
    let (_d, vault, mut db) = device();
    let base = "line1\nline2\nline3\n";
    vault
        .write_atomic("notes/markdown/A.md", base.as_bytes())
        .unwrap();
    db.set_current("note_a", 0, Some(base.as_bytes())).unwrap();

    // local edit (rev 1)
    let mine = "MINE\nline2\nline3\n";
    vault
        .write_atomic("notes/markdown/A.md", mine.as_bytes())
        .unwrap();
    db.set_current("note_a", 1, Some(mine.as_bytes())).unwrap();

    // remote op based on rev 0, editing a disjoint line
    let op = local_update_op(
        "devB",
        "note_a",
        0,
        "line1\nline2\nTHEIRS\n".as_bytes().to_vec(),
        Hlc::origin().tick(1),
        1,
    );
    let out = apply_remote(&mut db, &vault, &op, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(out, ApplyOutcome::Merged { conflict: false });
    assert_eq!(
        String::from_utf8(vault.read("notes/markdown/A.md").unwrap()).unwrap(),
        "MINE\nline2\nTHEIRS\n"
    );
}

#[test]
fn s3_md_conflict_markers_and_copy() {
    let (_d, vault, mut db) = device();
    let base = "line1\nline2\n";
    vault
        .write_atomic("notes/markdown/A.md", base.as_bytes())
        .unwrap();
    db.set_current("note_a", 0, Some(base.as_bytes())).unwrap();

    let mine = "mine edit\nline2\n";
    vault
        .write_atomic("notes/markdown/A.md", mine.as_bytes())
        .unwrap();
    db.set_current("note_a", 1, Some(mine.as_bytes())).unwrap();

    let op = local_update_op(
        "devB",
        "note_a",
        0,
        "theirs edit\nline2\n".as_bytes().to_vec(),
        Hlc::origin().tick(1),
        1,
    );
    let (out, copy) =
        sync::apply_remote_ex(&mut db, &vault, &op, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(out, ApplyOutcome::Merged { conflict: true });

    // The local edit (never journaled: a fresh edit on disk) stays in the note,
    // untouched and without markers; the remote one goes to a copy with its own id.
    let content = String::from_utf8(vault.read("notes/markdown/A.md").unwrap()).unwrap();
    assert_eq!(content, mine);

    let copy = copy.expect("conflict copy");
    assert_eq!(
        copy.path,
        merge::conflict_copy_path("notes/markdown/A.md", "devB", 1)
    );
    let text = String::from_utf8(vault.read(&copy.path).unwrap()).unwrap();
    assert!(text.contains("theirs edit\nline2\n"));
    assert!(text.contains(&format!("notium_id: {}", copy.note_id)));
    assert_ne!(copy.note_id, "note_a");
}

#[test]
fn s3b_newer_remote_version_wins_a_conflict() {
    let (_d, vault, mut db) = device();
    let base = "line1\nline2\n";
    vault
        .write_atomic("notes/markdown/A.md", base.as_bytes())
        .unwrap();
    db.set_current("note_a", 0, Some(base.as_bytes())).unwrap();
    // A journaled local edit stamped at t=5.
    let mine = local_update_op(
        "devA",
        "note_a",
        0,
        b"mine edit\nline2\n".to_vec(),
        Hlc { ts: 5, cnt: 0 },
        5,
    );
    db.append_journal(&mine).unwrap();
    vault
        .write_atomic("notes/markdown/A.md", b"mine edit\nline2\n")
        .unwrap();
    db.set_current("note_a", 1, Some(b"mine edit\nline2\n"))
        .unwrap();

    let theirs = local_update_op(
        "devB",
        "note_a",
        0,
        b"theirs edit\nline2\n".to_vec(),
        Hlc { ts: 9, cnt: 0 },
        9,
    );
    let (_, copy) =
        sync::apply_remote_ex(&mut db, &vault, &theirs, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(
        vault.read("notes/markdown/A.md").unwrap(),
        b"theirs edit\nline2\n"
    );
    let copy = copy.unwrap();
    assert_eq!(
        copy.path,
        merge::conflict_copy_path("notes/markdown/A.md", "devA", 5)
    );
    assert!(String::from_utf8(copy.content)
        .unwrap()
        .contains("mine edit"));
}

#[test]
fn s6_rename_with_concurrent_edit_keeps_identity() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic("notes/markdown/Idea.md", b"# Idea\n\ncontent v1\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let id = index.list_notes().unwrap()[0].note_id.clone();

    // external rename (preserves the embedded id in the file)
    std::fs::rename(
        vault.root.join("notes/markdown/Idea.md"),
        vault.root.join("notes/markdown/Viaje.md"),
    )
    .unwrap();
    index.reconcile(&vault).unwrap();

    // concurrent content edit that preserves the embedded id (as the app would)
    let content = format!("---\nnotium_id: {id}\n---\n# Viaje\n\ncontent v2\n");
    vault
        .write_atomic("notes/markdown/Viaje.md", content.as_bytes())
        .unwrap();
    index.reconcile(&vault).unwrap();

    let rec = index.get_note(&id).unwrap().unwrap();
    assert_eq!(rec.note_id, id);
    assert_eq!(rec.path, "notes/markdown/Viaje.md");
    assert_eq!(rec.title, "Viaje");
    assert_eq!(
        index.resolve_link("Idea").unwrap().unwrap(),
        id,
        "alias must survive"
    );
}

#[test]
fn s7_rename_vs_rename_accumulates_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    vault
        .write_atomic("notes/markdown/Idea.md", b"# Idea\n")
        .unwrap();
    let mut index = Index::open(&vault.index_db_path()).unwrap();
    index.reconcile(&vault).unwrap();
    let id = index.list_notes().unwrap()[0].note_id.clone();

    std::fs::rename(
        vault.root.join("notes/markdown/Idea.md"),
        vault.root.join("notes/markdown/Viaje.md"),
    )
    .unwrap();
    index.reconcile(&vault).unwrap();
    std::fs::rename(
        vault.root.join("notes/markdown/Viaje.md"),
        vault.root.join("notes/markdown/Trabajo.md"),
    )
    .unwrap();
    index.reconcile(&vault).unwrap();

    let rec = index.get_note(&id).unwrap().unwrap();
    assert_eq!(rec.path, "notes/markdown/Trabajo.md");
    // every historical title still resolves
    assert_eq!(index.resolve_link("Idea").unwrap().unwrap(), id);
    assert_eq!(index.resolve_link("Viaje").unwrap().unwrap(), id);
}

#[test]
fn s8_delete_vs_update_keeps_file() {
    let (_d, vault, mut db) = device();
    vault.write_atomic("notes/markdown/A.md", b"v1").unwrap();
    db.set_current("note_a", 0, Some(b"v1")).unwrap();
    // local edit -> rev 1
    vault
        .write_atomic("notes/markdown/A.md", b"local edit")
        .unwrap();
    db.set_current("note_a", 1, Some(b"local edit")).unwrap();

    // remote delete based on stale rev 0
    let op = sync::Op {
        op_id: "op_del".into(),
        device_id: "devB".into(),
        note_id: "note_a".into(),
        kind: sync::OpKind::Delete,
        base_rev: 0,
        new_rev: 2,
        content: None,
        hlc: Hlc::origin().tick(1),
        ts: 1,
        path: None,
        purge: false,
        base_hash: None,
    };
    let out = apply_remote(&mut db, &vault, &op, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(out, ApplyOutcome::DeleteConflict);
    assert!(
        vault.exists("notes/markdown/A.md"),
        "file must be kept for user resolution"
    );
}

#[test]
fn s9_no_resurrection_from_stale_push() {
    let (_d, vault, mut db) = device();
    vault.write_atomic("notes/markdown/A.md", b"v1").unwrap();
    db.set_current("note_a", 0, Some(b"v1")).unwrap();

    // delete at rev 1
    let del = sync::Op {
        op_id: "op_del".into(),
        device_id: "devA".into(),
        note_id: "note_a".into(),
        kind: sync::OpKind::Delete,
        base_rev: 0,
        new_rev: 1,
        content: None,
        hlc: Hlc::origin().tick(1),
        ts: 1,
        path: None,
        purge: false,
        base_hash: None,
    };
    assert_eq!(
        apply_remote(&mut db, &vault, &del, "markdown", "notes/markdown/A.md").unwrap(),
        ApplyOutcome::Deleted
    );
    assert!(!vault.exists("notes/markdown/A.md"));

    // stale update from a device that was offline before the delete
    let stale = local_update_op(
        "devB",
        "note_a",
        0,
        b"resurrect".to_vec(),
        Hlc::origin().tick(2),
        2,
    );
    let out = apply_remote(&mut db, &vault, &stale, "markdown", "notes/markdown/A.md").unwrap();
    assert_eq!(out, ApplyOutcome::RejectedTombstone);
    assert!(
        !vault.exists("notes/markdown/A.md"),
        "file must not be resurrected"
    );
}

#[test]
fn s10_external_edit_not_overwritten() {
    let (_d, vault, mut db) = device();
    vault.write_atomic("notes/markdown/A.md", b"v1").unwrap();
    db.set_current("note_a", 0, Some(b"v1")).unwrap();
    // local edit -> rev 1, sync.db knows "local v1"
    vault
        .write_atomic("notes/markdown/A.md", b"local v1")
        .unwrap();
    db.set_current("note_a", 1, Some(b"local v1")).unwrap();

    // external disk edit (not recorded in sync.db)
    vault
        .write_atomic("notes/markdown/A.md", b"external edit")
        .unwrap();

    // remote op that would fast-forward if we ignored the external edit
    let op = local_update_op(
        "devB",
        "note_a",
        1,
        b"remote v2".to_vec(),
        Hlc::origin().tick(1),
        1,
    );
    let out = apply_remote(&mut db, &vault, &op, "markdown", "notes/markdown/A.md").unwrap();
    assert_ne!(
        out,
        ApplyOutcome::FastForward,
        "must not silently overwrite external edit"
    );
    let content = String::from_utf8(vault.read("notes/markdown/A.md").unwrap()).unwrap();
    assert!(
        content.contains("external"),
        "external edit must be preserved: {content}"
    );
}

// ---- Chaos simulator (S11) and determinism (S12) ----

#[test]
fn s12_deterministic_replay() {
    let config = SimConfig {
        seed: 42,
        devices: 4,
        notes: sample_notes(),
        rounds: 15,
        edit_probability: 0.7,
        drop_rate: 0.3,
        duplicate_rate: 0.2,
    };
    let d1 = tempfile::tempdir().unwrap();
    let r1 = sim::run(&config, d1.path());
    let d2 = tempfile::tempdir().unwrap();
    let r2 = sim::run(&config, d2.path());
    assert_eq!(r1.state_hash, r2.state_hash);
    assert_eq!(r1.conflicts, r2.conflicts);
}

#[test]
fn s11_chaos_proptest_invariants() {
    use proptest::prelude::*;
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        cases: 128,
        ..Default::default()
    });
    let strategy = (0u64..10_000, 2usize..5, 1usize..12);
    let result = runner.run(&strategy, |(seed, devices, rounds)| {
        let config = SimConfig {
            seed,
            devices,
            notes: sample_notes(),
            rounds,
            edit_probability: 0.65,
            drop_rate: 0.35,
            duplicate_rate: 0.25,
        };
        let dir = tempfile::tempdir().unwrap();
        let r = sim::run(&config, dir.path());
        prop_assert!(r.converged, "devices must converge");
        prop_assert!(r.md_nonempty, "md notes must stay non-empty");
        Ok(())
    });
    if let Err(e) = result {
        panic!("chaos sim invariant violated: {e}");
    }
}
