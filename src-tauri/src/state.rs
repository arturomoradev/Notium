use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::Manager;

use notium_core::index::Index;
use notium_core::sync::SyncDb;
use notium_core::vault::LocalVault;

use crate::error::CommandError;

/// A single open vault: filesystem + derived index + sync journal. The `watcher`
/// field exists only on desktop.
pub struct VaultHandle {
    pub vault: LocalVault,
    pub index: Index,
    pub sync: SyncDb,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub watcher: Option<notium_core::watcher::VaultWatcher>,
}

/// Application state. The vault is behind a single-writer `Mutex` — every command
/// serializes its access to keep the index/journal consistent.
pub struct AppState {
    pub vault: Arc<Mutex<Option<VaultHandle>>>,
    pub self_writes: Arc<Mutex<HashMap<String, (Instant, String)>>>,
    pub sync_manager: Arc<Mutex<Option<Arc<crate::sync::SyncManager>>>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            vault: Arc::new(Mutex::new(None)),
            self_writes: Arc::new(Mutex::new(HashMap::new())),
            sync_manager: Arc::new(Mutex::new(None)),
        }
    }
}

/// Open (or initialize) a vault at `root`: create the layout, open the derived
/// index + sync journal, and reconcile the index against files already on disk.
pub fn open_vault(root: &Path) -> Result<VaultHandle, CommandError> {
    let vault = LocalVault::new(root);
    vault.init()?;
    let mut index = Index::open(&vault.index_db_path())
        .map_err(|e| CommandError::Sql(format!("open_index: {e}")))?;
    index
        .reconcile(&vault)
        .map_err(|e| CommandError::Sql(format!("reconcile: {e}")))?;
    let sync = SyncDb::open(&vault.sync_db_path())
        .map_err(|e| CommandError::Sql(format!("open_sync: {e}")))?;
    Ok(VaultHandle {
        vault,
        index,
        sync,
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        watcher: None,
    })
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VaultRegistry {
    pub active_vault: Option<String>,
    pub recent_vaults: Vec<String>,
    #[serde(default)]
    pub initialized_name: bool,
}

/// Resolve the base application data directory.
pub fn app_base_dir(app: &tauri::AppHandle) -> Result<PathBuf, CommandError> {
    app.path()
        .app_data_dir()
        .map_err(|e| CommandError::Io(e.to_string()))
}

/// Resolve the default vault directory (app-data "MyNotes" on both desktop and mobile).
pub fn default_vault_dir(app: &tauri::AppHandle) -> Result<PathBuf, CommandError> {
    let base = app_base_dir(app)?;
    let default_dir = base.join("MyNotes");
    let reg = read_registry(&base);
    if let Some(active) = reg.active_vault {
        let p = PathBuf::from(&active);
        if p.exists() || p.parent().map(|pr| pr.exists()).unwrap_or(false) {
            return Ok(p);
        }
    }
    if let Some(first) = reg.recent_vaults.first() {
        let p = PathBuf::from(first);
        if p.exists() {
            return Ok(p);
        }
    }
    Ok(default_dir)
}

pub fn read_registry(base: &Path) -> VaultRegistry {
    let p = base.join("vaults.json");
    if let Ok(s) = std::fs::read_to_string(p) {
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        VaultRegistry::default()
    }
}

pub fn write_registry(base: &Path, reg: &VaultRegistry) -> Result<(), CommandError> {
    std::fs::create_dir_all(base)?;
    let bytes = serde_json::to_vec_pretty(reg).map_err(|e| CommandError::Io(e.to_string()))?;
    notium_core::vault::atomic_write(&base.join("vaults.json"), &bytes)?;
    Ok(())
}

pub fn record_vault_in_registry(
    base: &Path,
    vault_root: &Path,
) -> Result<VaultRegistry, CommandError> {
    let mut reg = read_registry(base);
    let root_str = vault_root.to_string_lossy().to_string();
    reg.active_vault = Some(root_str.clone());
    if !reg.recent_vaults.iter().any(|v| v == &root_str) {
        reg.recent_vaults.insert(0, root_str);
    }
    write_registry(base, &reg)?;
    Ok(reg)
}
