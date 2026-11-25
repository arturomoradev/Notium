//! Deterministic multi-device sync/conflict simulator (Workstream 4).
//!
//! Device 0 acts as the sync authority (the "cloud"); clients 1..N make offline
//! edits and push ops with injected network faults (drop / duplicate / reorder).
//! The authority applies ops through the real merge engine, then a final flush
//! converges every client. A seeded xorshift PRNG makes runs fully reproducible.

use std::path::Path;

use crate::clock::Hlc;
use crate::hash;
use crate::sync::{self, ApplyOutcome, Op, SyncDb};
use crate::vault::LocalVault;

/// A seeded xorshift64* PRNG (deterministic, no external dependency).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    // An infinite PRNG stream, not an iterator; `next` is the conventional name.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() as f64) / (u64::MAX as f64) < p
    }
    pub fn range(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

#[derive(Clone, Debug)]
pub struct NoteSpec {
    pub note_id: String,
    pub format: String,
    pub path: String,
    pub initial: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SimConfig {
    pub seed: u64,
    pub devices: usize,
    pub notes: Vec<NoteSpec>,
    pub rounds: usize,
    pub edit_probability: f64,
    pub drop_rate: f64,
    pub duplicate_rate: f64,
}

#[derive(Clone, Debug)]
pub struct SimResult {
    pub ops_sent: usize,
    pub ops_applied: usize,
    pub conflicts: usize,
    pub converged: bool,
    pub md_nonempty: bool,
    pub state_hash: String,
}

struct Device {
    vault: LocalVault,
    db: SyncDb,
}

/// Run the simulation inside `root` (each device gets a subdirectory).
pub fn run(config: &SimConfig, root: &Path) -> SimResult {
    let mut rng = Rng::new(config.seed);
    let n = config.devices.max(2);

    let mut devices: Vec<Device> = Vec::new();
    for i in 0..n {
        let dir = root.join(format!("dev{i}"));
        let vault = LocalVault::new(&dir);
        vault.init().expect("vault init");
        let db = SyncDb::open(&vault.sync_db_path()).expect("sync db open");
        devices.push(Device { vault, db });
    }

    // Seed all devices with identical notes at rev 0.
    for note in &config.notes {
        for d in &mut devices {
            d.vault
                .write_atomic(&note.path, &note.initial)
                .expect("seed write");
            d.db.set_current(&note.note_id, 0, Some(&note.initial))
                .expect("seed rev");
        }
    }

    let mut result = SimResult {
        ops_sent: 0,
        ops_applied: 0,
        conflicts: 0,
        converged: false,
        md_nonempty: true,
        state_hash: String::new(),
    };

    for round in 0..config.rounds {
        // Phase 1: clients produce ops.
        let mut inbox: Vec<Op> = Vec::new();
        // `di` also addresses other per-device state below, so index explicitly.
        #[allow(clippy::needless_range_loop)]
        for di in 1..n {
            if !rng.chance(config.edit_probability) {
                continue;
            }
            let ni = rng.range(config.notes.len());
            let note = config.notes[ni].clone();
            let device = &mut devices[di];

            let cur = device.db.get_current(&note.note_id).ok().flatten();
            let cur_rev = cur.map(|(r, _)| r).unwrap_or(0);
            let old = device
                .vault
                .read(&note.path)
                .unwrap_or_else(|_| note.initial.clone());
            let new_content = mutate(&old, &mut rng, &format!("d{di}"));
            let op = sync::local_update_op(
                &format!("dev{di}"),
                &note.note_id,
                cur_rev,
                new_content.clone(),
                Hlc {
                    ts: round as u64,
                    cnt: di as u32,
                },
                round as u64,
            );
            device.vault.write_atomic(&note.path, &new_content).ok();
            device
                .db
                .set_current(&note.note_id, cur_rev + 1, Some(&new_content))
                .ok();

            if !rng.chance(config.drop_rate) {
                inbox.push(op.clone());
                if rng.chance(config.duplicate_rate) {
                    inbox.push(op);
                }
            }
            result.ops_sent += 1;
        }

        // Fault: reorder.
        if rng.chance(0.5) {
            inbox.reverse();
        }

        // Phase 2: authority (device 0) applies ops.
        for op in inbox {
            let Some(note) = config.notes.iter().find(|n| n.note_id == op.note_id) else {
                continue;
            };
            let authority = &mut devices[0];
            if let Ok(outcome) = sync::apply_remote(
                &mut authority.db,
                &authority.vault,
                &op,
                &note.format,
                &note.path,
            ) {
                result.ops_applied += 1;
                if let ApplyOutcome::Merged { conflict: true } = outcome {
                    result.conflicts += 1;
                }
            }
        }
    }

    // Final flush: converge clients to the authority.
    for note in &config.notes {
        let authority_content = devices[0].vault.read(&note.path).unwrap_or_default();
        for d in devices.iter_mut().skip(1) {
            d.vault.write_atomic(&note.path, &authority_content).ok();
        }
    }

    // Invariants.
    result.converged = {
        let mut ok = true;
        for note in &config.notes {
            let a = devices[0].vault.read(&note.path).unwrap_or_default();
            for d in devices.iter().skip(1) {
                let c = d.vault.read(&note.path).unwrap_or_default();
                if c != a {
                    ok = false;
                }
            }
        }
        ok
    };
    for note in &config.notes {
        let content = devices[0].vault.read(&note.path).unwrap_or_default();
        if content.is_empty() {
            result.md_nonempty = false;
        }
    }

    // Deterministic state hash over the authority's files.
    result.state_hash = state_hash(&devices[0], &config.notes);

    result
}

fn mutate(content: &[u8], rng: &mut Rng, tag: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(content).to_string();
    mutate_md(&text, rng, tag).into_bytes()
}

fn mutate_md(content: &str, rng: &mut Rng, tag: &str) -> String {
    let mut lines: Vec<String> = content.split('\n').map(|l| l.to_string()).collect();
    if lines.is_empty() || (lines.len() == 1 && lines[0].is_empty()) {
        return format!("{} {}\n", tag, rng.next() % 1000);
    }
    if rng.chance(0.5) {
        let i = rng.range(lines.len());
        lines[i] = format!("{} {}", tag, rng.next() % 1000);
    } else {
        lines.push(format!("{} {}", tag, rng.next() % 1000));
    }
    lines.join("\n")
}

fn state_hash(authority: &Device, notes: &[NoteSpec]) -> String {
    let mut hasher = blake3::Hasher::new();
    for note in notes {
        hasher.update(note.path.as_bytes());
        hasher.update(&[0]);
        hasher.update(&authority.vault.read(&note.path).unwrap_or_default());
        hasher.update(&[0xff]);
    }
    hash::hex(&hasher.finalize().as_bytes()[..16])
}

/// List conflict-copy files written for a note (for inspection in tests).
pub fn list_conflict_copies(vault: &LocalVault, note_path: &str) -> Vec<String> {
    let dir = Path::new(note_path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let notes = vault.scan_notes().unwrap_or_default();
    notes
        .into_iter()
        .filter(|n| n.rel_path.starts_with(&dir) && n.rel_path.contains("(conflict"))
        .map(|n| n.rel_path)
        .collect()
}
