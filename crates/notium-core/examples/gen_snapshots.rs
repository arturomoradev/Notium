//! One-shot generator: extract every fixture under `tests/fixtures/{md,json}` and
//! write the canonical golden snapshot to `tests/fixtures/expected/<name>.json`.
//!
//! Run with: `cargo run --example gen_snapshots` (from the crate directory).

use std::fs;
use std::path::Path;

use notium_core::extract::{self, EditorKind};

fn main() {
    let root = Path::new("tests/fixtures");
    let out_dir = root.join("expected");
    fs::create_dir_all(&out_dir).expect("create expected dir");

    let dir = root.join("md");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("read fixtures")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "md").unwrap_or(false))
        .collect();
    entries.sort();
    for path in entries {
        let bytes = fs::read(&path).expect("read fixture");
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let note_id = format!("note_{stem}");
        let doc = match extract::extract(EditorKind::Markdown, &note_id, &bytes) {
            Ok(d) => d,
            Err(_) => {
                println!("skip (unparseable): {}", path.display());
                continue;
            }
        };
        let json = serde_json::to_string_pretty(&doc).expect("serialize");
        let out = out_dir.join(format!("{stem}.json"));
        fs::write(&out, json).expect("write snapshot");
        println!("wrote {}", out.display());
    }
    println!("done");
}
