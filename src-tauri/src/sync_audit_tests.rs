//! Two-device scenarios from the sync audit: each test names the failure it guards against.

use super::*;
use notium_core::clock;
use notium_core::index::Index;
use notium_core::sync::SyncDb;
use notium_core::vault::LocalVault;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

fn device() -> (tempfile::TempDir, VaultHandle) {
    let dir = tempfile::tempdir().unwrap();
    let vault = LocalVault::new(dir.path());
    vault.init().unwrap();
    let index = Index::open(&vault.index_db_path()).unwrap();
    let sync = SyncDb::open(&vault.sync_db_path()).unwrap();
    let h = VaultHandle {
        vault,
        index,
        sync,
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        watcher: None,
    };
    (dir, h)
}

fn dev(h: &VaultHandle) -> String {
    load_sync_config(&h.vault.root).device_id
}

/// Deliver `from`'s unpushed ops to `to`, as a push then a pull would.
fn deliver(from: &mut VaultHandle, to: &mut VaultHandle) -> usize {
    let ops = from.sync.get_unpushed_ops().unwrap();
    let ids: Vec<&str> = ops.iter().map(|o| o.op_id.as_str()).collect();
    from.sync.mark_ops_synced(&ids).unwrap();
    let to_dev = dev(to);
    apply_pulled_ops(to, &ops, &to_dev).unwrap();
    let _ = to.index.reconcile(&to.vault);
    ops.len()
}

/// Exchange ops both ways until neither device has anything left to send.
fn settle(a: &mut VaultHandle, b: &mut VaultHandle) {
    for _ in 0..6 {
        let (da, db) = (dev(a), dev(b));
        journal_external_edits(a, &da).unwrap();
        journal_external_edits(b, &db).unwrap();
        heal_tombstoned_deletes(a).unwrap();
        heal_tombstoned_deletes(b).unwrap();
        if deliver(a, b) + deliver(b, a) == 0 {
            return;
        }
    }
    panic!("devices never stopped sending ops (sync loop)");
}

fn doc_of(h: &VaultHandle, id: &str) -> String {
    let rec = h.index.get_note(id).unwrap().unwrap();
    String::from_utf8(h.vault.read(&rec.path).unwrap()).unwrap()
}

fn rev_of(h: &VaultHandle, id: &str) -> u64 {
    h.sync.get_current(id).unwrap().unwrap().0
}

fn save(h: &mut VaultHandle, id: &str, body: &str) {
    let rev = rev_of(h, id);
    crate::commands::save_impl(h, id, body, rev).unwrap();
}

/// Live notes as (id, file contents), sorted by id.
fn live_notes(h: &VaultHandle) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = h
        .index
        .list_notes()
        .unwrap()
        .into_iter()
        .filter(|r| !r.tombstoned)
        .map(|r| {
            let bytes = h.vault.read(&r.path).unwrap();
            (r.note_id, String::from_utf8(bytes).unwrap())
        })
        .collect();
    out.sort();
    out
}

type Pair = (
    tempfile::TempDir,
    VaultHandle,
    tempfile::TempDir,
    VaultHandle,
    String,
);

/// A shared note on two devices, both at the same synced rev.
fn shared_note(body: &str) -> Pair {
    let (da, mut a) = device();
    let (db, mut b) = device();
    let id = crate::commands::create_impl(&mut a, "markdown", Some("Plan"), None)
        .unwrap()
        .note
        .note_id;
    save(&mut a, &id, body);
    settle(&mut a, &mut b);
    assert_eq!(doc_of(&a, &id), doc_of(&b, &id));
    (da, a, db, b, id)
}

// --- #3: a conflict must never break the note's frontmatter or clone its id ---
#[test]
fn conflict_keeps_the_note_id_and_gives_the_copy_its_own() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\ndos\n");
    save(&mut a, &id, "uno desde A\ndos\n");
    save(&mut b, &id, "uno desde B\ndos\n");
    settle(&mut a, &mut b);

    for h in [&a, &b] {
        let doc = doc_of(h, &id);
        assert_eq!(
            vault::extract_embedded_id(doc.as_bytes(), EditorKind::Markdown).as_deref(),
            Some(id.as_str()),
            "the note lost its id:\n{doc}"
        );
        assert!(
            !doc.contains("<<<<<<<"),
            "conflict markers inside the note:\n{doc}"
        );
    }
    // Both edits survive: one in the note, the other in a conflict copy with its own id.
    let all = live_notes(&a);
    assert_eq!(all.len(), 2, "note + one conflict copy: {all:#?}");
    let text: String = all.iter().map(|(_, d)| d.as_str()).collect();
    assert!(text.contains("uno desde A") && text.contains("uno desde B"));
}

// --- #2: after a conflict both devices must hold the same content ---
#[test]
fn devices_converge_after_a_conflict() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\ndos\n");
    save(&mut a, &id, "uno desde A\ndos\n");
    save(&mut b, &id, "uno desde B\ndos\n");
    settle(&mut a, &mut b);
    assert_eq!(live_notes(&a), live_notes(&b));
}

// --- #2 + `updated:` field: disjoint concurrent edits merge cleanly everywhere ---
#[test]
fn disjoint_concurrent_edits_merge_and_converge() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\ndos\ntres\n");
    // Saves in later seconds, so each side rewrites `updated:` with its own value.
    std::thread::sleep(Duration::from_millis(1100));
    save(&mut a, &id, "UNO\ndos\ntres\n");
    std::thread::sleep(Duration::from_millis(1100));
    save(&mut b, &id, "uno\ndos\nTRES\n");
    settle(&mut a, &mut b);

    let doc = doc_of(&a, &id);
    assert!(doc.contains("UNO\ndos\nTRES\n"), "not merged:\n{doc}");
    assert_eq!(live_notes(&a), live_notes(&b));
    assert_eq!(
        live_notes(&a).len(),
        1,
        "a clean merge leaves no conflict copy"
    );
}

// --- #2: the merge result reaches peers that do not merge (the web client) ---
#[test]
fn merge_result_is_journaled_for_other_peers() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\ndos\ntres\n");
    save(&mut a, &id, "UNO\ndos\ntres\n");
    save(&mut b, &id, "uno\ndos\nTRES\n");
    deliver(&mut b, &mut a);
    let merged = doc_of(&a, &id);
    let last = a.sync.get_unpushed_ops().unwrap().pop().unwrap();
    assert_eq!(last.content.as_deref(), Some(merged.as_bytes()));
}

// --- #4: delete on one device + edit on the other: the edit wins on both ---
#[test]
fn concurrent_edit_beats_delete_on_every_device() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\n");
    crate::commands::delete_impl(&mut b, &id).unwrap();
    save(&mut a, &id, "uno editado\n");
    settle(&mut a, &mut b);

    for h in [&a, &b] {
        let rec = h.index.get_note(&id).unwrap();
        assert!(
            rec.as_ref().is_some_and(|r| !r.tombstoned),
            "note gone on a device"
        );
        assert!(doc_of(h, &id).contains("uno editado"));
    }
}

// --- Re-delivered ops (repeated pull pages) change nothing the second time ---
#[test]
fn redelivered_ops_are_idempotent() {
    let (_da, mut a, _db, mut b, id) = shared_note("uno\ndos\ntres\n");
    save(&mut a, &id, "UNO\ndos\ntres\n");
    save(&mut b, &id, "uno\ndos\nTRES\n");
    let ops = b.sync.get_unpushed_ops().unwrap();
    let dev_a = dev(&a);
    apply_pulled_ops(&mut a, &ops, &dev_a).unwrap();
    let once = live_notes(&a);
    apply_pulled_ops(&mut a, &ops, &dev_a).unwrap();
    assert_eq!(live_notes(&a), once);
}

// --- #8: a vault folder copied to another place must not share the device id ---
#[test]
fn copied_vault_gets_its_own_device_id() {
    let d1 = tempfile::tempdir().unwrap();
    let d2 = tempfile::tempdir().unwrap();
    let first = load_sync_config(d1.path());
    fs::create_dir_all(d2.path().join(".app")).unwrap();
    fs::copy(
        d1.path().join(".app/sync_config.json"),
        d2.path().join(".app/sync_config.json"),
    )
    .unwrap();
    let copy = load_sync_config(d2.path());
    assert_ne!(first.device_id, copy.device_id);
    // Reopening the same folder keeps its id.
    assert_eq!(load_sync_config(d1.path()).device_id, first.device_id);
}

// --- #11: a case-only rename from another device is applied ---
#[test]
fn case_only_rename_is_applied() {
    let (_d, mut h) = device();
    let mut op = Op {
        op_id: "op_c".into(),
        device_id: "dev_remote".into(),
        note_id: "n1".into(),
        kind: sync::OpKind::Create,
        base_rev: 0,
        new_rev: 0,
        content: Some(b"---\nnotium_id: n1\n---\n\nhola\n".to_vec()),
        hlc: clock::Hlc { ts: 10, cnt: 0 },
        ts: 10,
        path: Some("notes/markdown/plan.md".into()),
        purge: false,
        base_hash: None,
        content_ref: None,
    };
    apply_pulled_ops(&mut h, &[op.clone()], "dev_local").unwrap();
    op.op_id = "op_r".into();
    op.kind = sync::OpKind::Update;
    op.new_rev = 1;
    op.hlc.ts = 20;
    op.path = Some("notes/markdown/Plan.md".into());
    apply_pulled_ops(&mut h, &[op], "dev_local").unwrap();
    assert_eq!(
        h.index.get_note("n1").unwrap().unwrap().path,
        "notes/markdown/Plan.md"
    );
}

/// A path sent in NFD by an older peer lands under its NFC name.
#[test]
fn remote_nfd_path_is_stored_as_nfc() {
    let (_d, mut h) = device();
    let op = Op {
        op_id: "op_nfd".into(),
        device_id: "dev_remote".into(),
        note_id: "n1".into(),
        kind: sync::OpKind::Create,
        base_rev: 0,
        new_rev: 0,
        content: Some(b"---\nnotium_id: n1\n---\n\nhola\n".to_vec()),
        hlc: clock::Hlc { ts: 10, cnt: 0 },
        ts: 10,
        path: Some("notes/markdown/Cafe\u{301}.md".into()),
        purge: false,
        base_hash: None,
        content_ref: None,
    };
    apply_pulled_ops(&mut h, &[op], "dev_local").unwrap();
    assert_eq!(
        h.index.get_note("n1").unwrap().unwrap().path,
        "notes/markdown/Caf\u{e9}.md"
    );
}

// --- Mock HTTP server for full cycles ---

struct Req {
    line: String,
    body: Vec<u8>,
}

/// Serve every request with `handler` (status, JSON body) until the test ends.
fn mock_server(handler: impl Fn(&Req) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut len = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let (status, json) = handler(&Req { line, body });
            let resp = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                json.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    url
}

fn cfg_for(url: String) -> SyncConfig {
    SyncConfig {
        server_url: url,
        auth_token: String::new(),
        device_id: "dev_local".into(),
        vault_id: "default".into(),
        enabled: true,
        auto_sync: true,
        interval_secs: 10,
        bound_to: String::new(),
    }
}

fn accept_all(req: &Req) -> String {
    let push: PushRequest = serde_json::from_slice(&req.body).unwrap();
    let ids: Vec<String> = push.ops.into_iter().map(|o| o.op_id).collect();
    serde_json::json!({ "accepted_op_ids": ids, "rejected_op_ids": [] }).to_string()
}

fn pull_json(cursor: &str, ops: &[Op], blobs: serde_json::Value) -> String {
    serde_json::json!({ "cursor": cursor, "ops": ops, "blobs": blobs }).to_string()
}

fn cloud_note(id: &str, rev: u64, body: &str) -> Op {
    Op {
        op_id: format!("op_{id}_{rev}"),
        device_id: "dev_remote".into(),
        note_id: id.into(),
        kind: if rev == 0 {
            sync::OpKind::Create
        } else {
            sync::OpKind::Update
        },
        base_rev: rev.saturating_sub(1),
        new_rev: rev,
        content: Some(format!("---\nnotium_id: {id}\n---\n\n{body}\n").into_bytes()),
        hlc: clock::Hlc {
            ts: 10 + rev,
            cnt: 0,
        },
        ts: 10 + rev,
        path: Some(format!("notes/markdown/{id}.md")),
        purge: false,
        base_hash: None,
        content_ref: None,
    }
}

fn shared(d: &tempfile::TempDir, h: VaultHandle) -> (SharedVault, PathBuf) {
    (Arc::new(Mutex::new(Some(h))), d.path().to_path_buf())
}

// --- #6a: a failing push must not stop this device from receiving changes ---
#[test]
fn failed_push_still_pulls() {
    let url = mock_server(|req| {
        if req.line.starts_with("POST /sync/push") {
            (500, r#"{"error":"boom"}"#.into())
        } else {
            let ops = [cloud_note("n7", 0, "remota")];
            (200, pull_json("s:1", &ops, serde_json::json!([])))
        }
    });
    let (d, mut h) = device();
    crate::commands::create_impl(&mut h, "markdown", Some("Local"), None).unwrap();
    let (shared, root) = shared(&d, h);
    assert!(
        run_sync_cycle(&shared, &root, &cfg_for(url)).is_err(),
        "the push error is reported"
    );
    let g = shared.lock().unwrap();
    assert!(
        g.as_ref().unwrap().index.get_note("n7").unwrap().is_some(),
        "pull was skipped"
    );
}

// --- #6a: ops the server rejects stay queued instead of being marked synced ---
#[test]
fn rejected_ops_stay_pending() {
    let url = mock_server(|req| {
        if req.line.starts_with("POST /sync/push") {
            let push: PushRequest = serde_json::from_slice(&req.body).unwrap();
            let ids: Vec<String> = push.ops.into_iter().map(|o| o.op_id).collect();
            let json = serde_json::json!({ "accepted_op_ids": [], "rejected_op_ids": ids });
            (200, json.to_string())
        } else {
            (200, pull_json("s:1", &[], serde_json::json!([])))
        }
    });
    let (d, mut h) = device();
    crate::commands::create_impl(&mut h, "markdown", Some("Local"), None).unwrap();
    let before = h.sync.unpushed_count().unwrap();
    assert!(before > 0);
    let (shared, root) = shared(&d, h);
    let _ = run_sync_cycle(&shared, &root, &cfg_for(url));
    let g = shared.lock().unwrap();
    assert_eq!(g.as_ref().unwrap().sync.unpushed_count().unwrap(), before);
}

// --- #6b: a blob the server no longer has must not freeze the pull cursor ---
#[test]
fn missing_blob_does_not_stall_the_cursor() {
    let url = mock_server(|req| {
        if req.line.starts_with("POST /sync/push") {
            (200, accept_all(req))
        } else if req.line.starts_with("GET /sync/blob") {
            (404, "\"Blob no encontrado\"".into())
        } else if req.line.contains("since=s%3A1") || req.line.contains("since=s:1") {
            let ops = [cloud_note("n9", 0, "tras el blob")];
            (200, pull_json("s:2", &ops, serde_json::json!([])))
        } else if req.line.contains("since=") {
            (200, pull_json("s:2", &[], serde_json::json!([])))
        } else {
            let blobs = serde_json::json!([{ "hash": "abc", "path": "attachments/x.png" }]);
            (200, pull_json("s:1", &[], blobs))
        }
    });
    let (d, h) = device();
    let (shared, root) = shared(&d, h);
    let cfg = cfg_for(url);
    let _ = run_sync_cycle(&shared, &root, &cfg);
    let _ = run_sync_cycle(&shared, &root, &cfg);
    let g = shared.lock().unwrap();
    assert!(
        g.as_ref().unwrap().index.get_note("n9").unwrap().is_some(),
        "cursor never advanced"
    );
}

/// Hold a file so reads and writes to it fail, like an antivirus or a
/// cloud-sync client does.
#[cfg(windows)]
fn block_file(path: &Path) -> Box<dyn Send> {
    use std::os::windows::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap();
    Box::new(file)
}

#[cfg(unix)]
fn block_file(path: &Path) -> Box<dyn Send> {
    use std::os::unix::fs::PermissionsExt;
    struct Guard(PathBuf);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o644));
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    Box::new(Guard(path.to_path_buf()))
}

// --- #5: an op that fails to apply is retried, not skipped forever ---
#[test]
fn op_that_fails_to_apply_is_retried() {
    let (d, mut h) = device();
    apply_pulled_ops(&mut h, &[cloud_note("n1", 0, "original")], "dev_local").unwrap();
    h.index.reconcile(&h.vault).unwrap();
    let file = d.path().join("notes/markdown/n1.md");
    // The server locks the note's file while it answers the first pull, so
    // applying that pull fails; the test unlocks it after that cycle.
    let lock: Arc<Mutex<Option<Box<dyn Send>>>> = Arc::default();
    let server_lock = lock.clone();
    let pulls = Mutex::new(0);
    let locked_file = file.clone();
    let url = mock_server(move |req| {
        if req.line.starts_with("POST /sync/push") {
            return (200, accept_all(req));
        }
        let mut n = pulls.lock().unwrap();
        *n += 1;
        if *n == 1 {
            *server_lock.lock().unwrap() = Some(block_file(&locked_file));
            let ops = [cloud_note("n1", 1, "cambiada")];
            (200, pull_json("s:1", &ops, serde_json::json!([])))
        } else {
            (200, pull_json("s:1", &[], serde_json::json!([])))
        }
    });
    let (shared, root) = shared(&d, h);
    let cfg = cfg_for(url);
    let _ = run_sync_cycle(&shared, &root, &cfg);
    assert!(
        lock.lock().unwrap().take().is_some(),
        "the first pull happened"
    );
    let _ = run_sync_cycle(&shared, &root, &cfg);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("cambiada"),
        "the remote edit was lost:\n{text}"
    );
}

// --- R2: notes over INLINE_CONTENT_MAX travel by reference, both ways ---
#[test]
fn large_note_travels_through_content_endpoint() {
    let stored: Arc<Mutex<Vec<u8>>> = Default::default();
    let pushed: Arc<Mutex<Vec<Op>>> = Default::default();
    let remote = cloud_note("big", 0, &"r".repeat(1_200_000));
    let remote_body = remote.content.clone().unwrap();
    let remote_hash = notium_core::hash::content_hash(&remote_body);
    let expected = remote_body.clone();
    let (s2, p2) = (stored.clone(), pushed.clone());
    let url = mock_server(move |req| {
        if req.line.starts_with("PUT /sync/content/") {
            *s2.lock().unwrap() = req.body.clone();
            (201, "{}".into())
        } else if req.line.starts_with("POST /sync/push") {
            let push: PushRequest = serde_json::from_slice(&req.body).unwrap();
            p2.lock().unwrap().extend(push.ops);
            (200, accept_all(req))
        } else if req.line.starts_with("GET /sync/content/big/") {
            (200, String::from_utf8(remote_body.clone()).unwrap())
        } else {
            let mut op = cloud_note("big", 0, "");
            op.content = None;
            op.content_ref = Some(remote_hash.clone());
            (200, pull_json("s:1", &[op], serde_json::json!([])))
        }
    });
    let (d, mut h) = device();
    let n = crate::commands::create_impl(&mut h, "markdown", Some("Grande"), None).unwrap();
    let body = format!("# Grande\n\n{}", "x".repeat(1_100_000));
    crate::commands::save_impl(&mut h, &n.note.note_id, &body, 0).unwrap();
    let (shared, root) = shared(&d, h);
    run_sync_cycle(&shared, &root, &cfg_for(url)).unwrap();

    let ops = pushed.lock().unwrap();
    let big = ops
        .iter()
        .find(|o| o.content_ref.is_some())
        .expect("no op by reference");
    assert!(big.content.is_none(), "the large content also went inline");
    let up = stored.lock().unwrap();
    assert_eq!(
        big.content_ref.as_deref(),
        Some(&*notium_core::hash::content_hash(&up))
    );
    assert!(ops
        .iter()
        .all(|o| o.content.as_ref().map_or(0, Vec::len) <= INLINE_CONTENT_MAX));

    let g = shared.lock().unwrap();
    let h = g.as_ref().unwrap();
    let rec = h
        .index
        .get_note("big")
        .unwrap()
        .expect("remote large note missing");
    assert_eq!(h.vault.read(&rec.path).unwrap(), expected);
}

// --- Attachment deletes reach the server, and the server's reach this device ---
#[test]
fn attachment_deletes_sync_both_ways() {
    let deleted: Arc<Mutex<Vec<String>>> = Default::default();
    let d2 = deleted.clone();
    let theirs = notium_core::hash::content_hash(b"theirs");
    let theirs2 = theirs.clone();
    let url = mock_server(move |req| {
        if req.line.starts_with("DELETE /sync/blob/") {
            d2.lock().unwrap().push(req.line.clone());
            (200, "{}".into())
        } else if req.line.starts_with("POST /sync/push") {
            (200, accept_all(req))
        } else {
            let blobs = serde_json::json!([
                { "hash": theirs2, "path": "attachments/b.png", "deleted": true }
            ]);
            (200, pull_json("s:1", &[], blobs))
        }
    });
    let (d, mut h) = device();
    let mine = notium_core::hash::content_hash(b"mine");
    h.sync.mark_blob_synced(&mine, "attachments/a.png").unwrap();
    h.vault
        .write_atomic("attachments/b.png", b"theirs")
        .unwrap();
    h.sync
        .mark_blob_synced(&theirs, "attachments/b.png")
        .unwrap();
    h.vault
        .write_atomic("attachments/keep.png", b"keep")
        .unwrap();
    let (shared, root) = shared(&d, h);
    run_sync_cycle(&shared, &root, &cfg_for(url)).unwrap();

    let sent = deleted.lock().unwrap();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        sent[0].contains(&mine) && sent[0].contains("a.png"),
        "{sent:?}"
    );
    let g = shared.lock().unwrap();
    let h = g.as_ref().unwrap();
    assert!(
        !h.vault.exists("attachments/b.png"),
        "the remote delete was not applied"
    );
    assert!(h.vault.exists("attachments/keep.png"));
    assert!(h
        .sync
        .synced_blobs()
        .unwrap()
        .iter()
        .all(|(_, p)| p != "attachments/a.png"));
}
