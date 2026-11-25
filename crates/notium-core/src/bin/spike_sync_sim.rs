//! `spike-sync-sim` — run a seeded multi-device sync/conflict simulation and print
//! the invariants. Companion to acceptance tests S11/S12.

use notium_core::sim::{self, NoteSpec, SimConfig};

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let notes = vec![
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
    ];
    let config = SimConfig {
        seed,
        devices: 4,
        notes,
        rounds: 30,
        edit_probability: 0.7,
        drop_rate: 0.3,
        duplicate_rate: 0.2,
    };
    let dir = std::env::temp_dir().join(format!("notium-sim-{}", std::process::id()));
    let r = sim::run(&config, &dir);

    println!("seed: {seed}");
    println!("ops sent:     {}", r.ops_sent);
    println!("ops applied:  {}", r.ops_applied);
    println!("conflicts:    {}", r.conflicts);
    println!("converged:    {}", r.converged);
    println!("md non-empty: {}", r.md_nonempty);
    println!("state hash:   {}", r.state_hash);
}
