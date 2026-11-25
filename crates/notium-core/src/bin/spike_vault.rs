//! `spike-vault <dir>` — initialize/reconcile a vault and print an identity report.
//!
//! Manual verification companion to the acceptance tests V1–V8.

use notium_core::index::Index;
use notium_core::vault::LocalVault;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let vault = LocalVault::new(&dir);
    vault.init().expect("init vault");
    let mut index = Index::open(&vault.index_db_path()).expect("open index");
    let report = index.reconcile(&vault).expect("reconcile");

    println!("vault: {}", vault.root.display());
    println!("notes scanned: {}", report.scanned);
    println!("  adopted (id injected): {}", report.adopted);
    println!("  new:                  {}", report.new);
    println!("  updated:              {}", report.updated);
    println!("  resolved by hash:     {}", report.resolved_by_hash);
    println!("  quarantined:          {}", report.quarantined);
    println!("  collisions:           {}", report.collisions.len());
    for c in &report.collisions {
        println!("    ! collision id={} paths={:?}", c.id, c.paths);
    }
    println!("indexed notes:");
    for n in index.list_notes().unwrap() {
        println!(
            "  - [{}] {} ({}){}{}",
            n.format,
            n.title,
            n.path,
            if n.tombstoned { " [tombstoned]" } else { "" },
            if n.quarantined { " [quarantined]" } else { "" },
        );
    }
}
