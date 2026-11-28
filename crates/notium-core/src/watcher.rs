//! Filesystem watcher primitive over the vault root (desktop only).
//!
//! Uses `notify`'s `RecommendedWatcher` with a simple 200 ms quiet-period debounce
//! implemented on an `mpsc` channel (deterministic, no extra debouncer dependency).
//! `.app/**` is filtered out to avoid reconcile feedback loops.
//!
//! On Android and iOS there is no `inotify`/`FSEvents`/ReadDirectoryChangesW; the shell
//! reconciles on lifecycle resume instead, so this module compiles to a no-op there.

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod imp {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    pub struct VaultWatcher {
        _watcher: RecommendedWatcher,
    }

    /// Watch `root` recursively. `on_change` receives vault-relative, `/`-separated
    /// paths (excluding `.app/**`) after ~200 ms of quiet.
    pub fn watch(
        root: &Path,
        on_change: impl Fn(Vec<String>) + Send + 'static,
    ) -> std::io::Result<VaultWatcher> {
        // macOS FSEvents reports canonical paths (`/var` -> `/private/var`), so a
        // root reached through a symlink would never match `strip_prefix` below.
        #[cfg(target_os = "macos")]
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        #[cfg(not(target_os = "macos"))]
        let root = root.to_path_buf();
        let (tx, rx) = channel::<notify::Event>();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        )
        .map_err(std::io::Error::other)?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(std::io::Error::other)?;

        std::thread::spawn(move || {
            let mut pending: Vec<PathBuf> = Vec::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(event) => pending.extend(event.paths),
                    Err(RecvTimeoutError::Timeout) => {
                        if !pending.is_empty() {
                            let mut paths = Vec::new();
                            for p in pending.drain(..) {
                                if let Ok(rel) = p.strip_prefix(&root) {
                                    let s = crate::paths::nfc(
                                        &rel.to_string_lossy().replace('\\', "/"),
                                    )
                                    .into_owned();
                                    if s.is_empty() || s.starts_with(".app/") {
                                        continue;
                                    }
                                    paths.push(s);
                                }
                            }
                            paths.sort();
                            paths.dedup();
                            if !paths.is_empty() {
                                on_change(paths);
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Ok(VaultWatcher { _watcher: watcher })
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use imp::{watch, VaultWatcher};

#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct VaultWatcher;

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn watch(
    _root: &std::path::Path,
    _on_change: impl Fn(Vec<String>) + Send + 'static,
) -> std::io::Result<VaultWatcher> {
    Ok(VaultWatcher)
}

#[cfg(test)]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;

    #[test]
    fn watcher_reports_changes_and_ignores_app_dir() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel::<Vec<String>>();
        let _w = watch(dir.path(), move |paths| {
            let _ = tx.send(paths);
        })
        .unwrap();

        // create a note (should be reported)
        fs::create_dir_all(dir.path().join("notes/tiptap")).unwrap();
        fs::write(
            dir.path().join("notes/tiptap/A.json"),
            crate::envelope::note_from_text(&String::from_utf8_lossy(b"# A\n".as_ref())).as_bytes(),
        )
        .unwrap();
        // create an .app file (should be filtered)
        fs::create_dir_all(dir.path().join(".app")).unwrap();
        fs::write(dir.path().join(".app/index.db"), b"x").unwrap();

        let paths = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(paths.iter().any(|p| p == "notes/tiptap/A.json"));
        assert!(!paths.iter().any(|p| p.starts_with(".app/")));
    }
}
