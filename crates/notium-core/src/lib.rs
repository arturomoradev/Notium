//! Notium headless Rust core.
//!
//! Pure Rust (no Tauri, no WebView). Modules:
//! - `clock`: UUIDv7 identity + Hybrid Logical Clock (total ordering).
//! - `hash`: BLAKE3 content hashing + deterministic chunk ids.
//! - `frontmatter`: surgical YAML frontmatter utilities for `.md` notes.
//! - `vault`: filesystem vault layout + stable identity resolution.
//! - `extract`: headless Markdown extractor (`.md`).
//! - `index`: derived SQLite index (notes, links, backlinks, FTS5, chunks).
//! - `links`: link resolution + backlink queries.
//! - `sync`: op journal, revision log, 3-way merge, conflict resolution.
//! - `sim`: deterministic multi-device sync/conflict simulator.

//!
//! The default `native` feature enables `index`, `vault`, `watcher`, `sim` and
//! the full `sync`; without it only the pure modules build (for WebAssembly).

pub mod clock;
pub mod extract;
pub mod frontmatter;
pub mod hash;
#[cfg(feature = "native")]
pub mod index;
pub mod links;
pub mod paths;
#[cfg(feature = "native")]
pub mod sim;
#[cfg(feature = "native")]
pub mod sync;
/// Without `native`, `sync` only carries the pure 3-way Markdown merge.
#[cfg(not(feature = "native"))]
pub mod sync {
    pub mod merge;
}
#[cfg(feature = "native")]
pub mod vault;
#[cfg(feature = "native")]
pub mod watcher;
