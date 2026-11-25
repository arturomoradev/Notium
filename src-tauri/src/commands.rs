//! Typed Tauri IPC commands (least-privilege: no raw FS/SQL/shell exposed).
//!
//! Every command serializes access through the single-writer `Mutex<VaultHandle>`
//! and runs on a blocking thread (the proven 0A `spawn_blocking` pattern).
//! Pure `*_impl` functions are unit-tested directly against a temp vault.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::time::Duration;
use std::time::Instant;

use tauri::{AppHandle, Emitter, State};

use notium_core::clock;
use notium_core::extract::EditorKind;
use notium_core::sync;
use notium_core::vault;

use crate::dto::{
    AttachmentItemDto, AttachmentNoteRefDto, BacklinkDto, FolderDto, LinkReferenceDto,
    LinkRepairResultDto, NoteContentDto, NoteSummaryDto, OutlinkDto, RebuildReportDto,
    SaveResultDto, SearchHitDto, SettingsDto, TitleSuggestionDto, TrashItemDto,
    VaultBackupBundleDto, VaultBackupEntryDto, VaultInfoDto, VaultProfileDto,
};
use crate::error::CommandError;
use crate::state::{self, AppState, VaultHandle};

// ---------------------------------------------------------------------------
// Async helpers
// ---------------------------------------------------------------------------

async fn blocking<T, F>(f: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(r) => r,
        Err(e) => Err(CommandError::Io(e.to_string())),
    }
}

fn lock<'a>(
    arc: &'a Arc<Mutex<Option<VaultHandle>>>,
) -> Result<std::sync::MutexGuard<'a, Option<VaultHandle>>, CommandError> {
    arc.lock()
        .map_err(|_| CommandError::Io("lock poisoned".into()))
}

pub fn ensure_sync_manager(state: &AppState, app: &AppHandle) -> Arc<crate::sync::SyncManager> {
    // A poisoned lock only means another thread panicked mid-access; the
    // Option<Arc<..>> inside is still valid, so recover instead of panicking.
    let mut guard = state.sync_manager.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mgr) = guard.as_ref() {
        mgr.clone()
    } else {
        let mgr = crate::sync::SyncManager::start(state.vault.clone(), app.clone());
        *guard = Some(mgr.clone());
        mgr
    }
}

// ---------------------------------------------------------------------------
// Vault commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn vault_init(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<VaultInfoDto, CommandError> {
    let base = state::app_base_dir(&app)?;
    let prior_reg = state::read_registry(&base);
    let is_first_run = !prior_reg.initialized_name && prior_reg.recent_vaults.len() <= 1;
    let dir = state::default_vault_dir(&app)?;
    let arc = state.vault.clone();
    let info = blocking({
        let arc = arc.clone();
        move || {
            let mut info = open_at(&arc, &dir)?;
            info.is_first_run = is_first_run && info.name == "MyNotes";
            let _ = state::record_vault_in_registry(&base, &dir);
            Ok(info)
        }
    })
    .await?;
    start_watcher(app.clone(), &arc, state.self_writes.clone());
    let mgr = ensure_sync_manager(&state, &app);
    mgr.trigger_sync();
    Ok(info)
}

#[tauri::command]
pub async fn vault_open(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<VaultInfoDto, CommandError> {
    let base = state::app_base_dir(&app)?;
    let dir = PathBuf::from(path);
    let arc = state.vault.clone();
    let info = blocking({
        let arc = arc.clone();
        move || {
            let info = open_at(&arc, &dir)?;
            let mut reg = state::record_vault_in_registry(&base, &dir)?;
            reg.initialized_name = true;
            let _ = state::write_registry(&base, &reg);
            Ok(info)
        }
    })
    .await?;
    start_watcher(app.clone(), &arc, state.self_writes.clone());
    let mgr = ensure_sync_manager(&state, &app);
    mgr.trigger_sync();
    Ok(info)
}

#[tauri::command]
pub async fn vault_create_named(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<VaultInfoDto, CommandError> {
    let base = state::app_base_dir(&app)?;
    let clean = vault::sanitize_title(&name);
    let dir = base.join(&clean);
    let arc = state.vault.clone();
    let info = blocking({
        let arc = arc.clone();
        move || {
            let info = open_at(&arc, &dir)?;
            let mut reg = state::record_vault_in_registry(&base, &dir)?;
            reg.initialized_name = true;
            let _ = state::write_registry(&base, &reg);
            Ok(info)
        }
    })
    .await?;
    start_watcher(app.clone(), &arc, state.self_writes.clone());
    let mgr = ensure_sync_manager(&state, &app);
    mgr.trigger_sync();
    Ok(info)
}

#[tauri::command]
pub async fn vault_rename_current(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<VaultInfoDto, CommandError> {
    let base = state::app_base_dir(&app)?;
    let clean = vault::sanitize_title(&name);
    let arc = state.vault.clone();
    let info = blocking({
        let arc = arc.clone();
        move || {
            let old_root = {
                let mut guard = lock(&arc)?;
                let root = guard.as_ref().map(|h| h.vault.root.clone());
                *guard = None;
                root
            };
            let old_dir = old_root.unwrap_or_else(|| base.join("MyNotes"));
            let parent = old_dir.parent().unwrap_or(&base);
            let new_dir = parent.join(&clean);
            if new_dir != old_dir && old_dir.exists() && !new_dir.exists() {
                let _ = std::fs::rename(&old_dir, &new_dir);
            }
            let target_dir = if new_dir.exists() || !old_dir.exists() {
                new_dir
            } else {
                old_dir.clone()
            };
            let info = open_at(&arc, &target_dir)?;
            let mut reg = state::read_registry(&base);
            let old_str = old_dir.to_string_lossy().to_string();
            let new_str = target_dir.to_string_lossy().to_string();
            reg.recent_vaults.retain(|p| p != &old_str && p != &new_str);
            reg.recent_vaults.insert(0, new_str.clone());
            reg.active_vault = Some(new_str);
            reg.initialized_name = true;
            state::write_registry(&base, &reg)?;
            Ok(info)
        }
    })
    .await?;
    start_watcher(app.clone(), &arc, state.self_writes.clone());
    let mgr = ensure_sync_manager(&state, &app);
    mgr.trigger_sync();
    Ok(info)
}

#[tauri::command]
pub async fn vault_delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<VaultInfoDto, CommandError> {
    let base = state::app_base_dir(&app)?;
    let target_dir = PathBuf::from(&path);
    let arc = state.vault.clone();
    let info = blocking({
        let arc = arc.clone();
        move || {
            let active_root = {
                let mut guard = lock(&arc)?;
                let current = guard.as_ref().map(|h| h.vault.root.clone());
                if current.as_deref() == Some(target_dir.as_path()) {
                    *guard = None;
                }
                current
            };
            if target_dir.exists() {
                std::fs::remove_dir_all(&target_dir)?;
            }
            let mut reg = state::read_registry(&base);
            reg.recent_vaults
                .retain(|p| p != &path && PathBuf::from(p).exists());
            let was_active = active_root.as_deref() == Some(target_dir.as_path());
            if was_active {
                if let Some(next_path) = reg.recent_vaults.first().cloned() {
                    reg.active_vault = Some(next_path.clone());
                    state::write_registry(&base, &reg)?;
                    return open_at(&arc, &PathBuf::from(next_path));
                } else {
                    let fresh_dir = base.join("MyNotes");
                    reg.active_vault = Some(fresh_dir.to_string_lossy().to_string());
                    reg.recent_vaults = vec![fresh_dir.to_string_lossy().to_string()];
                    reg.initialized_name = false;
                    state::write_registry(&base, &reg)?;
                    let mut info = open_at(&arc, &fresh_dir)?;
                    info.is_first_run = true;
                    return Ok(info);
                }
            }
            state::write_registry(&base, &reg)?;
            let guard = lock(&arc)?;
            let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
            vault_info(h)
        }
    })
    .await?;
    start_watcher(app.clone(), &arc, state.self_writes.clone());
    let mgr = ensure_sync_manager(&state, &app);
    mgr.trigger_sync();
    Ok(info)
}

#[tauri::command]
pub async fn vault_list_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<VaultProfileDto>, CommandError> {
    let base = state::app_base_dir(&app)?;
    let arc = state.vault.clone();
    blocking(move || {
        let active_root = lock(&arc)?
            .as_ref()
            .map(|h| h.vault.root.to_string_lossy().to_string())
            .unwrap_or_default();
        let reg = state::read_registry(&base);
        let mut paths: Vec<String> = reg
            .recent_vaults
            .into_iter()
            .filter(|p| PathBuf::from(p).exists())
            .collect();
        if !active_root.is_empty() && !paths.iter().any(|p| p == &active_root) {
            paths.insert(0, active_root.clone());
        }
        let mut out = Vec::new();
        for p in paths {
            let pb = PathBuf::from(&p);
            let name = pb
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone());
            out.push(VaultProfileDto {
                name,
                active: p == active_root,
                path: p,
            });
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn vault_export_bundle(
    state: State<'_, AppState>,
) -> Result<VaultBackupBundleDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        let vault_name = h
            .vault
            .root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Notium".to_string());
        let mut entries = Vec::new();
        for n in h.vault.scan_notes()? {
            entries.push(VaultBackupEntryDto {
                rel_path: n.rel_path,
                bytes: n.bytes,
            });
        }
        for a in h.vault.scan_attachments()? {
            entries.push(VaultBackupEntryDto {
                rel_path: a.rel_path,
                bytes: a.bytes,
            });
        }
        Ok(VaultBackupBundleDto {
            vault_name,
            exported_at: vault::now_ts(),
            entries,
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_import_bundle(
    state: State<'_, AppState>,
    bundle: VaultBackupBundleDto,
) -> Result<VaultInfoDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        for entry in bundle.entries {
            let rel = entry.rel_path.replace('\\', "/");
            if rel.contains("..")
                || (!rel.starts_with("notes/markdown/") && !rel.starts_with("attachments/"))
            {
                continue;
            }
            h.vault.write_atomic(&rel, &entry.bytes)?;
        }
        h.index
            .reconcile(&h.vault)
            .map_err(|e| CommandError::Sql(e.to_string()))?;
        vault_info(h)
    })
    .await
}

#[tauri::command]
pub async fn vault_get_settings(state: State<'_, AppState>) -> Result<SettingsDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        let s = h.vault.read_settings()?;
        Ok(SettingsDto {
            source_mode: s.source_mode,
            theme: s.theme,
            font_size: s.font_size,
            image_quality: s.image_quality,
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_update_settings(
    state: State<'_, AppState>,
    settings: SettingsDto,
) -> Result<(), CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        h.vault.write_settings(&vault::VaultSettings {
            source_mode: settings.source_mode,
            theme: match settings.theme.as_str() {
                "light" | "system" => settings.theme,
                _ => "dark".into(),
            },
            font_size: settings.font_size,
            image_quality: match settings.image_quality.as_str() {
                "720p" | "1440p" => settings.image_quality,
                _ => vault::default_image_quality(),
            },
        })?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_rebuild_index(
    state: State<'_, AppState>,
) -> Result<RebuildReportDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let report = h
            .index
            .rebuild(&h.vault)
            .map_err(|e| CommandError::Sql(e.to_string()))?;
        Ok(RebuildReportDto {
            scanned: report.scanned,
            adopted: report.adopted,
            updated: report.updated,
            tombstoned: report.tombstones.len(),
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Cloud Sync commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_sync_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::sync::SyncStatusDto, CommandError> {
    let mgr = ensure_sync_manager(&state, &app);
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        Ok(mgr.get_status(h))
    })
    .await
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::sync::SyncStatusDto, CommandError> {
    let mgr = ensure_sync_manager(&state, &app);
    let arc = state.vault.clone();
    blocking(move || {
        // Network I/O runs without the vault lock; see `run_sync_cycle`.
        let (root, config) = crate::sync::snapshot_config(&arc).ok_or(CommandError::VaultClosed)?;
        let res = crate::sync::run_sync_cycle(&arc, &root, &config);
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        match res {
            Ok(report) => {
                if report.ops_pulled > 0 || report.blobs_synced > 0 {
                    let _ = app.emit("vault://reconciled", ());
                }
                Ok(mgr.get_status(h))
            }
            Err(e) => {
                let mut status = mgr.get_status(h);
                status.state = "error".to_string();
                status.last_error = Some(e.to_string());
                Ok(status)
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn configure_sync_server(
    app: AppHandle,
    state: State<'_, AppState>,
    mut config: crate::sync::SyncConfig,
) -> Result<crate::sync::SyncStatusDto, CommandError> {
    config.vault_id = crate::sync::normalize_vault_id(&config.vault_id);
    if !config.server_url.trim().is_empty()
        && !crate::sync::is_allowed_server_url(&config.server_url)
    {
        return Err(CommandError::InvalidFormat(
            "la URL del servidor debe empezar por https://".into(),
        ));
    }
    let mgr = ensure_sync_manager(&state, &app);
    let arc = state.vault.clone();
    let config_clone = config.clone();
    let mgr_for_blocking = mgr.clone();
    let status = blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let prev = crate::sync::load_sync_config(&h.vault.root);
        let prev_vault_id = crate::sync::normalize_vault_id(&prev.vault_id);
        let server_changed = prev.server_url.trim() != config_clone.server_url.trim();
        let vault_changed = prev_vault_id != config_clone.vault_id;
        crate::sync::save_sync_config(&h.vault.root, &config_clone)?;
        if server_changed || vault_changed {
            let _ = h.sync.set_meta("last_pull_cursor", "0");
            let _ = h.sync.clear_synced_blobs();
        }
        Ok(mgr_for_blocking.get_status(h))
    })
    .await?;
    mgr.trigger_config_changed();
    if config.enabled {
        mgr.trigger_sync();
    }
    Ok(status)
}

#[tauri::command]
pub async fn pause_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::sync::SyncStatusDto, CommandError> {
    let mgr = ensure_sync_manager(&state, &app);
    mgr.pause();
    get_sync_status(app, state).await
}

#[tauri::command]
pub async fn resume_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::sync::SyncStatusDto, CommandError> {
    let mgr = ensure_sync_manager(&state, &app);
    mgr.resume();
    mgr.trigger_sync();
    get_sync_status(app, state).await
}

#[tauri::command]
pub async fn vault_pick_folder() -> Result<Option<String>, CommandError> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let folder = rfd::AsyncFileDialog::new()
            .set_title("Seleccionar carpeta del Vault")
            .pick_folder()
            .await;
        Ok(folder.map(|f| f.path().to_string_lossy().to_string()))
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub async fn vault_create_folder(
    state: State<'_, AppState>,
    name: String,
) -> Result<String, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        h.vault.create_folder(&name).map_err(CommandError::from)
    })
    .await
}

#[tauri::command]
pub async fn vault_rename_folder(
    state: State<'_, AppState>,
    folder: String,
    new_name: String,
) -> Result<String, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        rename_folder_impl(h, &folder, &new_name)
    })
    .await
}

#[tauri::command]
pub async fn vault_delete_folder(
    state: State<'_, AppState>,
    folder: String,
    keep_content: bool,
) -> Result<usize, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        delete_folder_impl(h, &folder, keep_content)
    })
    .await
}

#[tauri::command]
pub async fn vault_move_folder_to_vault(
    state: State<'_, AppState>,
    folder: String,
    target_vault_path: String,
) -> Result<usize, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        move_folder_to_vault_impl(h, &folder, PathBuf::from(target_vault_path).as_path())
    })
    .await
}

#[tauri::command]
pub async fn vault_list_folders(
    state: State<'_, AppState>,
) -> Result<Vec<FolderDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        list_folders_impl(h)
    })
    .await
}

pub(crate) fn list_folders_impl(h: &VaultHandle) -> Result<Vec<FolderDto>, CommandError> {
    let mut folders = Vec::new();
    let mut pending = vec![h.vault.root.join(vault::NOTES_MD)];
    while let Some(dir) = pending.pop() {
        if !dir.exists() {
            continue;
        }
        let read_res = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read_res {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                pending.push(entry.path());
                if let Ok(rel) = entry.path().strip_prefix(&h.vault.root) {
                    folders.push(FolderDto {
                        path: rel.to_string_lossy().replace('\\', "/"),
                    });
                }
            }
        }
    }
    folders.sort_by(|a, b| a.path.cmp(&b.path));
    folders.dedup_by(|a, b| a.path == b.path);
    Ok(folders)
}

fn rename_folder_impl(
    h: &mut VaultHandle,
    folder: &str,
    new_name: &str,
) -> Result<String, CommandError> {
    let before = note_paths(h)?;
    let new_folder = h.vault.rename_folder(folder, new_name)?;
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    // Every note under the folder got a new path: sync each move.
    for (note_id, path) in note_paths(h)? {
        if before.get(&note_id).is_some_and(|old| *old != path) {
            crate::sync::journal_local_path(h, &note_id, &path)?;
        }
    }
    Ok(new_folder)
}

/// `note_id -> path` of every live note.
fn note_paths(h: &VaultHandle) -> Result<HashMap<String, String>, CommandError> {
    let conn = h.index.conn();
    let mut stmt = conn
        .prepare("SELECT note_id, path FROM notes WHERE tombstoned = 0")
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| CommandError::Sql(e.to_string()))
}

fn delete_folder_impl(
    h: &mut VaultHandle,
    folder: &str,
    keep_content: bool,
) -> Result<usize, CommandError> {
    let clean = vault::sanitize_folder_path(folder)?;
    let prefix = format!("{}/{}/", vault::NOTES_MD, clean);
    let existing_notes = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let affected = h.vault.delete_folder(&clean, keep_content)?;
    if !keep_content {
        let now = vault::now_ts().parse::<u64>().unwrap_or(0);
        for rec in existing_notes {
            if !rec.tombstoned && rec.path.starts_with(&prefix) {
                let _ = h.index.mark_tombstoned(&rec.note_id);
                let _ = crate::sync::journal_local_delete(h, &rec.note_id, now);
            }
        }
    }
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    Ok(affected.len())
}

fn move_folder_to_vault_impl(
    h: &mut VaultHandle,
    folder: &str,
    target_vault_root: &std::path::Path,
) -> Result<usize, CommandError> {
    let clean = vault::sanitize_folder_path(folder)?;
    if clean.is_empty() {
        return Err(CommandError::Io("cannot move root folder".into()));
    }
    if target_vault_root == h.vault.root.as_path() {
        return Err(CommandError::Io(
            "source and destination vault are the same".into(),
        ));
    }
    let src_dir = h.vault.root.join(vault::NOTES_MD).join(&clean);
    if !src_dir.exists() {
        return Err(CommandError::NotFound(clean));
    }

    let target_vault = vault::LocalVault::new(target_vault_root);
    target_vault.init()?;
    let _ = target_vault.create_folder(&clean);

    let prefix = format!("{}/{}/", vault::NOTES_MD, clean);
    let notes_in_folder: Vec<notium_core::index::NoteRecord> = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .into_iter()
        .filter(|n| !n.tombstoned && n.path.starts_with(&prefix))
        .collect();

    // Also copy empty subdirectories inside `folder`
    let mut dir_stack = vec![src_dir.clone()];
    while let Some(d) = dir_stack.pop() {
        for entry in std::fs::read_dir(d)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                dir_stack.push(p.clone());
                if let Ok(rel_sub) = p.strip_prefix(h.vault.root.join(vault::NOTES_MD)) {
                    let rel_sub_str = rel_sub.to_string_lossy().replace('\\', "/");
                    let _ = target_vault.create_folder(&rel_sub_str);
                }
            }
        }
    }

    let mut moved_count = 0usize;
    let now = vault::now_ts().parse::<u64>().unwrap_or(0);
    for rec in &notes_in_folder {
        let bytes = h.vault.read(&rec.path)?;
        // Copy any attachments referenced by this note
        if let Ok(extracted) =
            notium_core::extract::extract(EditorKind::Markdown, &rec.note_id, &bytes)
        {
            for att_ref in extracted.attachment_refs {
                let att_rel = if att_ref.starts_with("attachments/") {
                    att_ref
                } else {
                    format!("attachments/{att_ref}")
                };
                if h.vault.exists(&att_rel) && !target_vault.exists(&att_rel) {
                    if let Ok(att_bytes) = h.vault.read(&att_rel) {
                        let _ = target_vault.write_atomic(&att_rel, &att_bytes);
                    }
                }
            }
        }
        // Write the note into target vault (deduplicating if a file at the same path exists)
        let dst_rel = if target_vault.exists(&rec.path) {
            let stem = vault::title_from_path(&rec.path);
            let parent = std::path::Path::new(&rec.path)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| vault::NOTES_MD.to_string());
            let mut n = 2usize;
            let mut candidate = format!("{parent}/{stem} {n}.md");
            while target_vault.exists(&candidate) {
                n += 1;
                candidate = format!("{parent}/{stem} {n}.md");
            }
            candidate
        } else {
            rec.path.clone()
        };
        target_vault.write_atomic(&dst_rel, &bytes)?;
        h.vault.remove_file(&rec.path)?;
        let _ = h.index.mark_tombstoned(&rec.note_id);
        let _ = crate::sync::journal_local_delete(h, &rec.note_id, now);
        moved_count += 1;
    }

    let _ = std::fs::remove_dir_all(&src_dir);
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let mut target_index = notium_core::index::Index::open(&target_vault.index_db_path())
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    target_index
        .reconcile(&target_vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    Ok(moved_count)
}

/// Import an attachment. The file bytes travel as the raw IPC request body
/// (no JSON number-array encoding) and the percent-encoded original filename
/// in the `x-filename` header.
#[tauri::command]
pub async fn vault_import_attachment(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, CommandError> {
    let bytes: Vec<u8> = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        // Fallback transport (postMessage) delivers the payload as a JSON array.
        tauri::ipc::InvokeBody::Json(value) => serde_json::from_value(value.clone())
            .map_err(|_| CommandError::InvalidFormat("expected a binary request body".into()))?,
    };
    let filename = request
        .headers()
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            percent_encoding::percent_decode_str(v)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_default();
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        let raw_name = std::path::Path::new(&filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment");
        let (stem, ext) = raw_name.rsplit_once('.').unwrap_or((raw_name, "bin"));
        let safe = |s: &str| -> String {
            s.chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .take(64)
                .collect()
        };
        let stem = safe(stem);
        let ext = safe(ext);
        let unique = format!(
            "{}-{}.{}",
            if stem.is_empty() { "attachment" } else { &stem },
            uuid::Uuid::now_v7().simple(),
            if ext.is_empty() { "bin" } else { &ext }
        );
        let rel = format!("attachments/{unique}");
        h.vault.write_atomic(&rel, &bytes)?;
        drop(guard);
        let mut guard = lock(&arc)?;
        if let Some(h) = guard.as_mut() {
            let atts = h.vault.scan_attachments()?;
            let _ = h.index.reconcile_attachments(&atts);
        }
        Ok(rel)
    })
    .await
}

/// Read an attachment. Returned as a raw IPC response (an `ArrayBuffer` in the
/// frontend) rather than a JSON number array.
#[tauri::command]
pub async fn vault_read_attachment(
    state: State<'_, AppState>,
    path: String,
) -> Result<tauri::ipc::Response, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        if !path.starts_with("attachments/") || path.contains("..") || path.contains('\\') {
            return Err(CommandError::Io("invalid attachment path".into()));
        }
        // Only resolve the path under the vault lock; reading a large image must not block other commands.
        let abs = {
            let guard = lock(&arc)?;
            let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
            h.vault.root.join(&path)
        };
        Ok(tauri::ipc::Response::new(std::fs::read(abs)?))
    })
    .await
}

#[tauri::command]
pub async fn attachment_list(
    state: State<'_, AppState>,
) -> Result<Vec<AttachmentItemDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        attachment_list_impl(h)
    })
    .await
}

#[tauri::command]
pub async fn attachment_delete(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        if !path.starts_with("attachments/") || path.contains("..") || path.contains('\\') {
            return Err(CommandError::Io("invalid attachment path".into()));
        }
        let abs = h.vault.root.join(&path);
        if abs.exists() {
            let _ = std::fs::remove_file(&abs);
        }
        let conn = h.index.conn();
        let _ = conn.execute(
            "UPDATE attachments SET tombstoned = 1 WHERE path = ?1",
            [&path],
        );
        let _ = conn.execute("DELETE FROM attachments WHERE path = ?1", [&path]);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn attachment_delete_orphans(state: State<'_, AppState>) -> Result<usize, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let items = attachment_list_impl(h)?;
        let mut deleted_count = 0;
        let conn = h.index.conn();
        for item in items {
            if item.is_orphan {
                let abs = h.vault.root.join(&item.path);
                if abs.exists() {
                    let _ = std::fs::remove_file(&abs);
                }
                let _ = conn.execute("DELETE FROM attachments WHERE path = ?1", [&item.path]);
                deleted_count += 1;
            }
        }
        Ok(deleted_count)
    })
    .await
}

fn attachment_list_impl(h: &VaultHandle) -> Result<Vec<AttachmentItemDto>, CommandError> {
    let conn = h.index.conn();
    let mut stmt = conn
        .prepare("SELECT att_id, path, content_hash, file_size FROM attachments WHERE tombstoned = 0 ORDER BY path ASC")
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let att_rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? as u64,
            ))
        })
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let mut out = Vec::new();
    for att in att_rows {
        let (att_id, path, content_hash, file_size) =
            att.map_err(|e| CommandError::Sql(e.to_string()))?;
        let filename = path
            .strip_prefix("attachments/")
            .unwrap_or(&path)
            .to_string();

        let mut ref_stmt = conn
            .prepare(
                "SELECT r.note_id, n.title
                 FROM attachment_refs r
                 JOIN notes n ON n.note_id = r.note_id
                 WHERE r.att_id = ?1 AND n.tombstoned = 0",
            )
            .map_err(|e| CommandError::Sql(e.to_string()))?;

        let referencing_notes: Vec<AttachmentNoteRefDto> = ref_stmt
            .query_map([&att_id], |row| {
                Ok(AttachmentNoteRefDto {
                    note_id: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .map_err(|e| CommandError::Sql(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        let is_orphan = referencing_notes.is_empty();

        out.push(AttachmentItemDto {
            att_id,
            path,
            filename,
            file_size,
            content_hash,
            is_orphan,
            referencing_notes,
        });
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Note commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn note_list(
    state: State<'_, AppState>,
    tag: Option<String>,
    format: Option<String>,
) -> Result<Vec<NoteSummaryDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        list_impl(h, tag.as_deref(), format.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn note_create(
    state: State<'_, AppState>,
    kind: String,
    title: Option<String>,
    folder: Option<String>,
) -> Result<NoteContentDto, CommandError> {
    let arc = state.vault.clone();
    let res = blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        create_impl(h, &kind, title.as_deref(), folder.as_deref())
    })
    .await?;
    if let Ok(guard) = state.sync_manager.lock() {
        if let Some(mgr) = guard.as_ref() {
            mgr.trigger_sync();
        }
    }
    Ok(res)
}

#[tauri::command]
pub async fn note_read(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<NoteContentDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        read_impl(h, &note_id)
    })
    .await
}

#[tauri::command]
pub async fn note_save(
    state: State<'_, AppState>,
    note_id: String,
    content: String,
    base_rev: u64,
) -> Result<SaveResultDto, CommandError> {
    let arc = state.vault.clone();
    let writes = state.self_writes.clone();
    let res = blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let result = save_impl(h, &note_id, &content, base_rev)?;
        if let Ok(mut writes) = writes.lock() {
            writes.insert(
                result.note.path.clone(),
                (
                    Instant::now(),
                    blake3::hash(&h.vault.read(&result.note.path)?)
                        .to_hex()
                        .to_string(),
                ),
            );
        }
        Ok(result)
    })
    .await?;
    if let Ok(guard) = state.sync_manager.lock() {
        if let Some(mgr) = guard.as_ref() {
            mgr.trigger_sync();
        }
    }
    Ok(res)
}

#[tauri::command]
pub async fn note_rename(
    state: State<'_, AppState>,
    note_id: String,
    new_title: String,
) -> Result<NoteSummaryDto, CommandError> {
    let arc = state.vault.clone();
    let res = blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        rename_impl(h, &note_id, &new_title)
    })
    .await?;
    if let Ok(guard) = state.sync_manager.lock() {
        if let Some(mgr) = guard.as_ref() {
            mgr.trigger_sync();
        }
    }
    Ok(res)
}

#[tauri::command]
pub async fn note_move(
    state: State<'_, AppState>,
    note_id: String,
    folder: String,
) -> Result<NoteSummaryDto, CommandError> {
    let arc = state.vault.clone();
    let res = blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        move_impl(h, &note_id, &folder)
    })
    .await?;
    if let Ok(guard) = state.sync_manager.lock() {
        if let Some(mgr) = guard.as_ref() {
            mgr.trigger_sync();
        }
    }
    Ok(res)
}

#[tauri::command]
pub async fn note_delete(state: State<'_, AppState>, note_id: String) -> Result<(), CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        delete_impl(h, &note_id)
    })
    .await?;
    if let Ok(guard) = state.sync_manager.lock() {
        if let Some(mgr) = guard.as_ref() {
            mgr.trigger_sync();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn trash_list(state: State<'_, AppState>) -> Result<Vec<TrashItemDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        let items = h.vault.list_trash()?;
        Ok(items
            .into_iter()
            .map(|t| TrashItemDto {
                trash_id: t.trash_id,
                note_id: t.note_id,
                title: t.title,
                deleted_at: t.deleted_at,
                days_remaining: t.days_remaining,
                size: t.size,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn trash_restore(
    state: State<'_, AppState>,
    trash_id: String,
) -> Result<NoteSummaryDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        restore_trash_impl(h, &trash_id)
    })
    .await
}

#[tauri::command]
pub async fn trash_delete_item(
    state: State<'_, AppState>,
    trash_id: String,
) -> Result<(), CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let note_id = h
            .vault
            .list_trash()?
            .into_iter()
            .find(|t| t.trash_id == trash_id)
            .and_then(|t| t.note_id);
        h.vault.delete_trash_item(&trash_id)?;
        if let Some(id) = note_id {
            purge_if_deleted(h, &id)?;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn trash_empty(state: State<'_, AppState>) -> Result<(), CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        let note_ids: HashSet<String> = h
            .vault
            .list_trash()?
            .into_iter()
            .filter_map(|t| t.note_id)
            .collect();
        h.vault.empty_trash()?;
        for id in note_ids {
            purge_if_deleted(h, &id)?;
        }
        Ok(())
    })
    .await
}

/// Sync a permanent delete, unless the note is live here (a stale trash copy
/// of a note that was restored or recreated must not erase it elsewhere).
fn purge_if_deleted(h: &mut VaultHandle, note_id: &str) -> Result<(), CommandError> {
    let live = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .is_some_and(|rec| !rec.tombstoned);
    if live {
        return Ok(());
    }
    crate::sync::journal_local_purge(h, note_id)
}

#[tauri::command]
pub async fn note_search_fts(
    state: State<'_, AppState>,
    query: String,
    tag: Option<String>,
    format: Option<String>,
) -> Result<Vec<SearchHitDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        search_impl(h, &query, tag.as_deref(), format.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn note_get_backlinks(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<BacklinkDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        backlinks_impl(h, &note_id)
    })
    .await
}

#[tauri::command]
pub async fn note_suggest_titles(
    state: State<'_, AppState>,
    prefix: String,
) -> Result<Vec<TitleSuggestionDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        let items = h
            .index
            .suggest_titles(&prefix, 500)
            .map_err(|e| CommandError::Sql(e.to_string()))?;
        Ok(items
            .into_iter()
            .map(|(note_id, title, format)| TitleSuggestionDto {
                note_id,
                title,
                format,
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn note_unlinked_mentions(
    state: State<'_, AppState>,
    note_id: String,
    title: String,
) -> Result<Vec<BacklinkDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        unlinked_mentions_impl(h, &note_id, &title)
    })
    .await
}

#[tauri::command]
pub async fn note_get_outlinks(
    state: State<'_, AppState>,
    note_id: String,
) -> Result<Vec<OutlinkDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        outlinks_impl(h, &note_id)
    })
    .await
}

#[tauri::command]
pub async fn note_check_link_references(
    state: State<'_, AppState>,
    old_title: String,
) -> Result<Vec<LinkReferenceDto>, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let guard = lock(&arc)?;
        let h = guard.as_ref().ok_or(CommandError::VaultClosed)?;
        check_link_references_impl(h, &old_title)
    })
    .await
}

#[tauri::command]
pub async fn note_repair_links(
    state: State<'_, AppState>,
    old_title: String,
    new_title: String,
) -> Result<LinkRepairResultDto, CommandError> {
    let arc = state.vault.clone();
    blocking(move || {
        let mut guard = lock(&arc)?;
        let h = guard.as_mut().ok_or(CommandError::VaultClosed)?;
        repair_links_impl(h, &old_title, &new_title)
    })
    .await
}

// ---------------------------------------------------------------------------
// Mobile resume-reconcile hook (called from lib.rs on RunEvent::Resumed)
// ---------------------------------------------------------------------------

#[cfg(mobile)]
pub fn reconcile_on_resume(app: &AppHandle) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<AppState>() {
        let arc = state.vault.clone();
        let app2 = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            if let Ok(mut guard) = arc.lock() {
                if let Some(h) = guard.as_mut() {
                    if let Err(e) = h.index.reconcile(&h.vault) {
                        log::error!("resume reconcile failed: {e}");
                    }
                }
            }
            let _ = app2.emit("vault-changed", Vec::<String>::new());
        });
    }
}

// ---------------------------------------------------------------------------
// Impl functions (unit-tested directly)
// ---------------------------------------------------------------------------

fn open_at(
    arc: &Arc<Mutex<Option<VaultHandle>>>,
    dir: &std::path::Path,
) -> Result<VaultInfoDto, CommandError> {
    let h = state::open_vault(dir)?;
    let info = vault_info(&h)?;
    *lock(arc)? = Some(h);
    Ok(info)
}

fn vault_info(h: &VaultHandle) -> Result<VaultInfoDto, CommandError> {
    let recs = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let notes = recs.iter().filter(|n| !n.tombstoned).count();
    let mut folders = HashSet::new();
    let mut pending = vec![h.vault.root.join(vault::NOTES_MD)];
    while let Some(dir) = pending.pop() {
        if !dir.exists() {
            continue;
        }
        if let Ok(read_res) = std::fs::read_dir(dir) {
            for entry in read_res.flatten() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    folders.insert(entry.path());
                    pending.push(entry.path());
                }
            }
        }
    }
    let attachments = h.vault.scan_attachment_meta()?.len();
    let name = h
        .vault
        .root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Notium".to_string());
    Ok(VaultInfoDto {
        root: h.vault.root.to_string_lossy().to_string(),
        name,
        notes,
        attachments,
        folders: folders.len(),
        opened: true,
        is_first_run: false,
    })
}

fn parse_format(kind: &str) -> Result<EditorKind, CommandError> {
    match kind {
        "" | "markdown" | "md" => Ok(EditorKind::Markdown),
        other => Err(CommandError::InvalidFormat(other.to_string())),
    }
}

fn to_summary(
    h: &VaultHandle,
    rec: &notium_core::index::NoteRecord,
) -> Result<NoteSummaryDto, CommandError> {
    let tags = h
        .index
        .tags_for(&rec.note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    Ok(NoteSummaryDto {
        note_id: rec.note_id.clone(),
        title: rec.title.clone(),
        format: rec.format.clone(),
        path: rec.path.clone(),
        updated_at: rec.updated_at.clone(),
        tags,
    })
}

fn list_impl(
    h: &VaultHandle,
    tag: Option<&str>,
    format: Option<&str>,
) -> Result<Vec<NoteSummaryDto>, CommandError> {
    let notes = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let mut out = Vec::new();
    for rec in notes {
        if rec.tombstoned {
            continue;
        }
        if let Some(f) = format {
            if rec.format != f {
                continue;
            }
        }
        let summary = to_summary(h, &rec)?;
        if let Some(t) = tag {
            if !summary.tags.iter().any(|x| x == t) {
                continue;
            }
        }
        out.push(summary);
    }
    Ok(out)
}

pub(crate) fn create_impl(
    h: &mut VaultHandle,
    kind: &str,
    title: Option<&str>,
    folder: Option<&str>,
) -> Result<NoteContentDto, CommandError> {
    let editor = parse_format(kind)?;
    let id = clock::note_id();
    let title = title
        .map(|t| t.to_string())
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("Sin título {}", vault::now_ts()));

    let rel_path = h
        .vault
        .create_note_in(editor, &title, &id, folder.unwrap_or(""))?;
    let bytes = h.vault.read(&rel_path)?;
    h.index
        .reindex_note(&id, &rel_path, editor, &bytes)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    h.sync
        .set_current(&id, 0, Some(&bytes))
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let sync_config = crate::sync::load_sync_config(&h.vault.root);
    let now = vault::now_ts().parse::<u64>().unwrap_or(0);
    let op = sync::local_create_op(
        &sync_config.device_id,
        &id,
        bytes.clone(),
        clock::Hlc { ts: now, cnt: 0 },
        now,
    )
    .with_path(&rel_path);
    let _ = h.sync.claim_path(&op);
    let _ = h.sync.append_journal(&op);

    let rec = h
        .index
        .get_note(&id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(id.clone()))?;
    let summary = to_summary(h, &rec)?;
    Ok(NoteContentDto {
        note: summary,
        content: editor_content(&bytes, editor),
        rev: 0,
    })
}

fn read_impl(h: &VaultHandle, note_id: &str) -> Result<NoteContentDto, CommandError> {
    let rec = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec.tombstoned {
        return Err(CommandError::NotFound(note_id.to_string()));
    }
    let bytes = h.vault.read(&rec.path)?;
    let rev = h
        .sync
        .get_current(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .map(|(r, _)| r)
        .unwrap_or(0);
    let summary = to_summary(h, &rec)?;
    let editor = parse_format(&rec.format)?;
    Ok(NoteContentDto {
        note: summary,
        content: editor_content(&bytes, editor),
        rev,
    })
}

fn save_impl(
    h: &mut VaultHandle,
    note_id: &str,
    content: &str,
    base_rev: u64,
) -> Result<SaveResultDto, CommandError> {
    let rec = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec.tombstoned {
        return Err(CommandError::NotFound(note_id.to_string()));
    }
    let current_rev = h
        .sync
        .get_current(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .map(|(r, _)| r)
        .unwrap_or(0);
    if base_rev != current_rev {
        return Err(CommandError::Conflict);
    }

    let editor = parse_format(&rec.format)?;
    // Identity is never lost by a save: re-inject the id if frontmatter was stripped,
    // or stitch the existing frontmatter onto the edited body, and update `updated`.
    let ts = vault::now_ts();
    let bytes = match editor {
        EditorKind::Markdown => {
            let previous = h.vault.read(&rec.path)?;
            let previous = String::from_utf8_lossy(&previous);
            let (fm, old_body) = notium_core::frontmatter::split(&previous);
            let full_doc = if fm.is_some() {
                let prefix_len = previous.len() - old_body.len();
                let mut stitched = String::with_capacity(prefix_len + content.len());
                stitched.push_str(&previous[..prefix_len]);
                stitched.push_str(content);
                notium_core::frontmatter::inject_notium_id(&stitched, note_id)
            } else {
                notium_core::frontmatter::inject_notium_id(content, note_id)
            };
            notium_core::frontmatter::upsert_field(&full_doc, "updated", &ts).into_bytes()
        }
    };

    h.vault.write_atomic(&rec.path, &bytes)?;
    h.index
        .reindex_note(note_id, &rec.path, editor, &bytes)
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let new_rev = current_rev + 1;
    let now = vault::now_ts().parse::<u64>().unwrap_or(0);
    h.sync
        .set_current(note_id, new_rev, Some(&bytes))
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let sync_config = crate::sync::load_sync_config(&h.vault.root);
    let op = sync::local_update_op(
        &sync_config.device_id,
        note_id,
        current_rev,
        bytes,
        clock::Hlc { ts: now, cnt: 0 },
        now,
    );
    h.sync
        .append_journal(&op)
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let rec2 = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    Ok(SaveResultDto {
        rev: new_rev,
        note: to_summary(h, &rec2)?,
    })
}

fn editor_content(bytes: &[u8], editor: EditorKind) -> String {
    let raw = String::from_utf8_lossy(bytes);
    match editor {
        EditorKind::Markdown => notium_core::frontmatter::split(&raw).1.to_string(),
    }
}

fn rename_impl(
    h: &mut VaultHandle,
    note_id: &str,
    new_title: &str,
) -> Result<NoteSummaryDto, CommandError> {
    let rec = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec.tombstoned {
        return Err(CommandError::NotFound(note_id.to_string()));
    }
    h.vault.rename_note(&rec.path, new_title)?;
    // Reconcile records the path change + alias (identity is preserved via the
    // embedded id). Rename is infrequent, so a full reconcile is acceptable here.
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let rec2 = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec2.path != rec.path {
        crate::sync::journal_local_path(h, note_id, &rec2.path)?;
    }
    to_summary(h, &rec2)
}

fn move_impl(
    h: &mut VaultHandle,
    note_id: &str,
    folder: &str,
) -> Result<NoteSummaryDto, CommandError> {
    let rec = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec.tombstoned {
        return Err(CommandError::NotFound(note_id.to_string()));
    }
    h.vault.move_note(&rec.path, folder)?;
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let rec2 = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec2.path != rec.path {
        crate::sync::journal_local_path(h, note_id, &rec2.path)?;
    }
    to_summary(h, &rec2)
}

fn delete_impl(h: &mut VaultHandle, note_id: &str) -> Result<(), CommandError> {
    let rec = h
        .index
        .get_note(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .ok_or_else(|| CommandError::NotFound(note_id.to_string()))?;
    if rec.tombstoned {
        return Err(CommandError::NotFound(note_id.to_string()));
    }
    h.vault.delete_to_trash(&rec.path)?;
    h.index
        .mark_tombstoned(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let now = vault::now_ts().parse::<u64>().unwrap_or(0);
    crate::sync::journal_local_delete(h, note_id, now)?;
    Ok(())
}

fn restore_trash_impl(h: &mut VaultHandle, trash_id: &str) -> Result<NoteSummaryDto, CommandError> {
    let (rel_path, embedded_id) = h.vault.restore_from_trash(trash_id)?;
    if let Some(ref id) = embedded_id {
        let _ = h.sync.clear_tombstone(id);
    }
    h.index
        .reconcile(&h.vault)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    if let Some(id) = embedded_id {
        if let Some(rec) = h
            .index
            .get_note(&id)
            .map_err(|e| CommandError::Sql(e.to_string()))?
        {
            // Sync the restore: content + path on top of the tombstone rev.
            crate::sync::journal_local_path(h, &id, &rec.path)?;
            return to_summary(h, &rec);
        }
    }
    let notes = h
        .index
        .list_notes()
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let rec = notes
        .into_iter()
        .find(|n| n.path == rel_path && !n.tombstoned)
        .ok_or(CommandError::NotFound(rel_path))?;
    to_summary(h, &rec)
}

fn search_impl(
    h: &VaultHandle,
    query: &str,
    tag: Option<&str>,
    format: Option<&str>,
) -> Result<Vec<SearchHitDto>, CommandError> {
    let hits = h
        .index
        .search(query, tag, format)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    Ok(hits
        .into_iter()
        .map(|x| SearchHitDto {
            note_id: x.note_id,
            title: x.title,
            path: x.path,
            format: x.format,
        })
        .collect())
}

fn backlinks_impl(h: &VaultHandle, note_id: &str) -> Result<Vec<BacklinkDto>, CommandError> {
    let bl = h
        .index
        .get_backlinks(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    bl.into_iter()
        .map(|b| {
            let snippet = snippet_for(h, &b.source_id, &b.link_text);
            Ok(BacklinkDto {
                source_id: b.source_id,
                source_title: b.source_title,
                link_text: b.link_text,
                kind: b.kind,
                snippet,
            })
        })
        .collect()
}

fn unlinked_mentions_impl(
    h: &VaultHandle,
    note_id: &str,
    title: &str,
) -> Result<Vec<BacklinkDto>, CommandError> {
    // Phrase-match the title across the FTS index, then drop the note itself and
    // any source that already links to it explicitly (those are "linked mentions").
    let phrase = format!("\"{}\"", title.replace('"', " "));
    let hits = h
        .index
        .search(&phrase, None, None)
        .map_err(|e| CommandError::Sql(e.to_string()))?;
    let linked: HashSet<String> = h
        .index
        .get_backlinks(note_id)
        .map_err(|e| CommandError::Sql(e.to_string()))?
        .into_iter()
        .map(|b| b.source_id)
        .collect();
    let mut out = Vec::new();
    for hit in hits {
        if hit.note_id == note_id || linked.contains(&hit.note_id) {
            continue;
        }
        let snippet = snippet_for(h, &hit.note_id, title);
        out.push(BacklinkDto {
            source_id: hit.note_id,
            source_title: hit.title,
            link_text: title.to_string(),
            kind: "mention".to_string(),
            snippet,
        });
    }
    Ok(out)
}

fn snippet_for(h: &VaultHandle, note_id: &str, needle: &str) -> String {
    match h.index.body_of(note_id) {
        Ok(Some(body)) => snippet_around(&body, needle),
        _ => String::new(),
    }
}

/// Return a short window of `body` around the first occurrence of `needle`.
fn snippet_around(body: &str, needle: &str) -> String {
    let needle = needle.trim();
    if needle.is_empty() {
        return body.chars().take(120).collect();
    }
    let body_chars: Vec<char> = body.chars().collect();
    let lower_body: Vec<char> = body_chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let lower_needle: Vec<char> = needle
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let n = lower_needle.len();
    let total = body_chars.len();
    if n > 0 && n <= total {
        if let Some(char_idx) = lower_body
            .windows(n)
            .position(|w| w == lower_needle.as_slice())
        {
            let start = char_idx.saturating_sub(40);
            let end = (char_idx + n + 60).min(total);
            let mut out = String::new();
            if start > 0 {
                out.push('…');
            }
            out.extend(&body_chars[start..end]);
            if end < total {
                out.push('…');
            }
            return out;
        }
    }
    body_chars.into_iter().take(120).collect()
}

fn outlinks_impl(h: &VaultHandle, note_id: &str) -> Result<Vec<OutlinkDto>, CommandError> {
    let conn = h.index.conn();
    let mut stmt = conn
        .prepare(
            "SELECT l.target_raw, l.link_text, l.kind, l.is_attachment, l.target_id, l.target_path, n.title
             FROM links l
             LEFT JOIN notes n ON n.note_id = l.target_id AND n.tombstoned = 0
             WHERE l.source_id = ?1
             ORDER BY l.target_raw ASC",
        )
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let rows = stmt
        .query_map([note_id], |row| {
            let target_raw: String = row.get(0)?;
            let link_text: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let is_attachment: bool = row.get::<_, i64>(3)? != 0;
            let target_id: Option<String> = row.get(4)?;
            let target_path: Option<String> = row.get(5)?;
            let note_title: Option<String> = row.get(6)?;

            let (is_resolved, target_title) = if is_attachment {
                let resolved = target_id.is_some();
                let title = target_path
                    .as_ref()
                    .map(|p| p.strip_prefix("attachments/").unwrap_or(p).to_string());
                (resolved, title)
            } else {
                let resolved = target_id.is_some() && note_title.is_some();
                (resolved, note_title)
            };

            Ok(OutlinkDto {
                target_raw,
                link_text,
                kind,
                is_attachment,
                target_id,
                target_title,
                target_path,
                is_resolved,
            })
        })
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| CommandError::Sql(e.to_string()))?);
    }
    Ok(out)
}

fn check_link_references_impl(
    h: &VaultHandle,
    old_title: &str,
) -> Result<Vec<LinkReferenceDto>, CommandError> {
    let conn = h.index.conn();
    let mut stmt = conn
        .prepare("SELECT note_id, title, path FROM notes WHERE tombstoned = 0 ORDER BY title ASC")
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let notes = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| CommandError::Sql(e.to_string()))?;

    let mut refs = Vec::new();
    for n in notes {
        let (note_id, title, path) = n.map_err(|e| CommandError::Sql(e.to_string()))?;
        if let Ok(bytes) = h.vault.read(&path) {
            let content = String::from_utf8_lossy(&bytes);
            let (_, count) =
                notium_core::links::rewrite_wikilink_targets(&content, old_title, "PLACEHOLDER");
            if count > 0 {
                refs.push(LinkReferenceDto {
                    note_id,
                    title,
                    path,
                    count,
                });
            }
        }
    }
    Ok(refs)
}

fn repair_links_impl(
    h: &mut VaultHandle,
    old_title: &str,
    new_title: &str,
) -> Result<LinkRepairResultDto, CommandError> {
    let notes: Vec<(String, String, String)> = {
        let conn = h.index.conn();
        let mut stmt = conn
            .prepare("SELECT note_id, title, path FROM notes WHERE tombstoned = 0")
            .map_err(|e| CommandError::Sql(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| CommandError::Sql(e.to_string()))?;

        let res: Vec<(String, String, String)> = rows.filter_map(|r| r.ok()).collect();
        res
    };

    let mut affected_notes = 0;
    let mut total_updated = 0;

    for (note_id, _title, path) in notes {
        if let Ok(bytes) = h.vault.read(&path) {
            let original = String::from_utf8_lossy(&bytes);
            let (rewritten, count) =
                notium_core::links::rewrite_wikilink_targets(&original, old_title, new_title);
            if count > 0 && rewritten != original {
                h.vault.write_atomic(&path, rewritten.as_bytes())?;
                affected_notes += 1;
                total_updated += count;

                let now = vault::now_ts().parse::<u64>().unwrap_or(0);
                let current_rev = h
                    .sync
                    .get_current(&note_id)
                    .ok()
                    .flatten()
                    .map(|(r, _)| r)
                    .unwrap_or(0);
                let new_rev = current_rev + 1;
                let _ = h
                    .sync
                    .set_current(&note_id, new_rev, Some(rewritten.as_bytes()));
                let op = sync::local_update_op(
                    "local",
                    &note_id,
                    current_rev,
                    rewritten.as_bytes().to_vec(),
                    clock::Hlc { ts: now, cnt: 0 },
                    now,
                );
                let _ = h.sync.append_journal(&op);
            }
        }
    }

    if affected_notes > 0 {
        let _ = h.index.reconcile(&h.vault);
    }

    Ok(LinkRepairResultDto {
        affected_notes_count: affected_notes,
        updated_links_count: total_updated,
    })
}

// ---------------------------------------------------------------------------
// Desktop watcher (no-op on Android and iOS)
// ---------------------------------------------------------------------------

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn start_watcher(
    app: AppHandle,
    arc: &Arc<Mutex<Option<VaultHandle>>>,
    self_writes: Arc<Mutex<std::collections::HashMap<String, (Instant, String)>>>,
) {
    if let Ok(mut guard) = arc.lock() {
        if let Some(h) = guard.as_mut() {
            let root = h.vault.root.clone();
            let event_root = root.clone();
            let app2 = app.clone();
            let arc2 = arc.clone();
            let cb = move |paths: Vec<String>| {
                let paths: Vec<String> = paths
                    .into_iter()
                    .filter(|path| {
                        let mut writes = match self_writes.lock() {
                            Ok(w) => w,
                            Err(_) => return true,
                        };
                        if let Some((at, expected)) = writes.get(path) {
                            if at.elapsed() <= Duration::from_millis(1500) {
                                let matches = std::fs::read(event_root.join(path))
                                    .ok()
                                    .map(|bytes| {
                                        blake3::hash(&bytes).to_hex().to_string() == *expected
                                    })
                                    .unwrap_or(false);
                                if matches {
                                    writes.remove(path);
                                    return false;
                                }
                            }
                            writes.remove(path);
                        }
                        true
                    })
                    .collect();
                if !paths.is_empty() {
                    if let Ok(mut guard) = arc2.lock() {
                        if let Some(h) = guard.as_mut() {
                            if let Err(e) = h.index.reconcile(&h.vault) {
                                log::error!("watcher reconcile failed: {e}");
                            }
                        }
                    }
                    let _ = app2.emit("vault-changed", paths);
                }
            };
            match notium_core::watcher::watch(&root, cb) {
                Ok(w) => h.watcher = Some(w),
                Err(e) => log::error!("watcher start failed: {e}"),
            }
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn start_watcher(
    _app: AppHandle,
    _arc: &Arc<Mutex<Option<VaultHandle>>>,
    _writes: Arc<Mutex<std::collections::HashMap<String, (Instant, String)>>>,
) {
}

// ---------------------------------------------------------------------------
// Tests (temp vault, no Tauri runtime needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handle() -> (tempfile::TempDir, VaultHandle) {
        let dir = tempfile::tempdir().unwrap();
        let h = state::open_vault(dir.path()).unwrap();
        (dir, h)
    }

    #[test]
    fn create_read_save_roundtrip() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("Idea"), None).unwrap();
        assert_eq!(created.rev, 0);
        assert_eq!(created.note.format, "markdown");
        assert!(
            created.note.updated_at.is_some(),
            "created note must immediately have updated_at"
        );

        let read = read_impl(&h, &created.note.note_id).unwrap();
        assert_eq!(read.note.title, "Idea");
        assert!(
            !read.content.contains("# Idea"),
            "the title is not repeated as a heading"
        );

        let saved = save_impl(&mut h, &created.note.note_id, "# Idea\n\nhola", 0).unwrap();
        assert_eq!(saved.rev, 1);
        assert!(
            saved.note.updated_at.is_some(),
            "saved note must update updated_at"
        );
        let read2 = read_impl(&h, &created.note.note_id).unwrap();
        assert_eq!(read2.rev, 1);
        assert!(read2.content.contains("hola"));
    }

    #[test]
    fn save_stale_rev_conflicts() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        save_impl(&mut h, &created.note.note_id, "# A\n\nv1", 0).unwrap(); // rev -> 1
        let err = save_impl(&mut h, &created.note.note_id, "# A\n\nv2", 0).unwrap_err();
        assert!(matches!(err, CommandError::Conflict));
    }

    #[test]
    fn save_preserves_identity_when_frontmatter_stripped() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        // Save content with NO frontmatter; identity must be re-injected.
        save_impl(&mut h, &created.note.note_id, "# A\n\nsin frontmatter", 0).unwrap();
        let bytes = h.vault.read(&created.note.path).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("notium_id:"));
        // And the note keeps its id.
        let rec = h.index.get_note(&created.note.note_id).unwrap().unwrap();
        assert_eq!(rec.note_id, created.note.note_id);
    }

    #[test]
    fn markdown_editor_receives_body_only_and_save_preserves_the_yaml_envelope() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        assert!(!created.content.starts_with("---"));
        assert_eq!(created.content, "");
        save_impl(&mut h, &created.note.note_id, "# Edited body\n", 0).unwrap();
        let raw = String::from_utf8(h.vault.read(&created.note.path).unwrap()).unwrap();
        assert!(raw.starts_with("---\nnotium_id:"));
        assert!(raw.contains("created:"));
        assert!(raw.ends_with("# Edited body\n"));
        assert_eq!(
            read_impl(&h, &created.note.note_id).unwrap().content,
            "# Edited body\n"
        );
    }

    #[test]
    fn rename_keeps_identity_and_alias() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("Idea"), None).unwrap();
        let summary = rename_impl(&mut h, &created.note.note_id, "Concepto").unwrap();
        assert_eq!(summary.title, "Concepto");
        // old title resolves via alias
        let resolved = h.index.resolve_link("Idea").unwrap().unwrap();
        assert_eq!(resolved, created.note.note_id);
    }

    #[test]
    fn delete_tombstones() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        delete_impl(&mut h, &created.note.note_id).unwrap();
        assert!(h.sync.is_tombstoned(&created.note.note_id).unwrap());
        assert!(
            h.index
                .get_note(&created.note.note_id)
                .unwrap()
                .unwrap()
                .tombstoned
        );
        // read after delete -> NotFound
        assert!(matches!(
            read_impl(&h, &created.note.note_id),
            Err(CommandError::NotFound(_))
        ));
    }

    #[test]
    fn delete_folder_journals_delete_ops() {
        let (_dir, mut h) = test_handle();
        let a = create_impl(&mut h, "markdown", Some("A"), Some("sub")).unwrap();
        let b = create_impl(&mut h, "markdown", Some("B"), Some("sub")).unwrap();

        delete_folder_impl(&mut h, "sub", false).unwrap();

        for id in [&a.note.note_id, &b.note.note_id] {
            assert!(h.sync.is_tombstoned(id).unwrap());
            assert!(
                h.sync.has_delete_op(id).unwrap(),
                "delete op must be journaled for {id}"
            );
        }
    }

    #[test]
    fn search_and_backlinks() {
        let (_dir, mut h) = test_handle();
        let a = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        let b = create_impl(&mut h, "markdown", Some("B"), None).unwrap();
        save_impl(&mut h, &a.note.note_id, "# A\n\nSee [[B]] and #work.", 0).unwrap();

        let hits = search_impl(&h, "B", None, None).unwrap();
        assert!(!hits.is_empty());
        // Special FTS5 characters (`#`, `()`, `"`, `:`, `-`) must not fail with a SQL syntax error:
        let special = search_impl(&h, "#work (See) \"A:", None, None).unwrap();
        assert!(!special.is_empty());
        let backlinks = backlinks_impl(&h, &b.note.note_id).unwrap();
        assert!(backlinks.iter().any(|bl| bl.source_id == a.note.note_id));
    }

    #[test]
    fn suggest_titles_filters_by_prefix() {
        let (_dir, mut h) = test_handle();
        create_impl(&mut h, "markdown", Some("Proyecto"), None).unwrap();
        create_impl(&mut h, "markdown", Some("Trabajo"), None).unwrap();
        create_impl(&mut h, "markdown", Some("Procesos"), None).unwrap();

        let all = h.index.suggest_titles("", 500).unwrap();
        assert_eq!(all.len(), 3);

        let pro = h.index.suggest_titles("pro", 500).unwrap();
        assert_eq!(
            pro.len(),
            2,
            "prefix 'pro' should match Proyecto + Procesos (case-insensitive)"
        );
        let titles: Vec<&str> = pro.iter().map(|(_, t, _)| t.as_str()).collect();
        assert!(titles.contains(&"Proyecto"));
        assert!(titles.contains(&"Procesos"));

        let none = h.index.suggest_titles("zzz", 500).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn external_disk_edit_reconciles() {
        let (_dir, mut h) = test_handle();
        let created = create_impl(&mut h, "markdown", Some("A"), None).unwrap();
        let note_id = created.note.note_id.clone();
        let path = created.note.path.clone();

        // Simulate an external tool rewriting the file (bypassing our write path).
        std::fs::write(h.vault.root.join(&path), b"# A\n\neditado fuera\n").unwrap();

        // Reconcile (what the watcher / resume hook triggers) and re-read.
        h.index.reconcile(&h.vault).unwrap();
        let read = read_impl(&h, &note_id).unwrap();
        assert_eq!(
            read.note.note_id, note_id,
            "identity must survive external edit"
        );
        assert!(
            read.content.contains("editado fuera"),
            "external edit must be picked up"
        );
    }

    #[test]
    fn create_folder_and_note_in_folder() {
        let (_dir, mut h) = test_handle();
        let folder = h.vault.create_folder("Proyectos").unwrap();
        assert_eq!(folder, "notes/markdown/Proyectos");
        let created = create_impl(&mut h, "markdown", Some("Plan"), Some("Proyectos")).unwrap();
        assert_eq!(created.note.path, "notes/markdown/Proyectos/Plan.md");
    }

    #[test]
    fn folder_listing_includes_empty_directories() {
        let (_dir, h) = test_handle();
        h.vault.create_folder("Empty folder").unwrap();
        assert!(list_folders_impl(&h)
            .unwrap()
            .iter()
            .any(|f| f.path == "notes/markdown/Empty folder"));
        assert_eq!(vault_info(&h).unwrap().folders, 1);
    }

    #[test]
    fn rename_and_delete_folder_with_keep_or_trash_and_move_to_vault() {
        let (_dir, mut h) = test_handle();
        h.vault.create_folder("Proyectos").unwrap();
        let n1 = create_impl(&mut h, "markdown", Some("Idea 1"), Some("Proyectos")).unwrap();
        let n2 = create_impl(&mut h, "markdown", Some("Idea 2"), Some("Proyectos")).unwrap();

        // 1. Rename folder "Proyectos" -> "Archivo"
        let renamed = rename_folder_impl(&mut h, "Proyectos", "Archivo").unwrap();
        assert_eq!(renamed, "Archivo");
        let r1 = h.index.get_note(&n1.note.note_id).unwrap().unwrap();
        assert_eq!(r1.path, "notes/markdown/Archivo/Idea 1.md");

        // 2. Delete folder "Archivo" keeping content -> notes move to root `notes/markdown/`
        let moved_count = delete_folder_impl(&mut h, "Archivo", true).unwrap();
        assert_eq!(moved_count, 2);
        let r1_after = h.index.get_note(&n1.note.note_id).unwrap().unwrap();
        assert_eq!(r1_after.path, "notes/markdown/Idea 1.md");

        // 3. Move notes into a new folder "Borrar" and delete folder without keeping content
        move_impl(&mut h, &n1.note.note_id, "Borrar").unwrap();
        let trashed_count = delete_folder_impl(&mut h, "Borrar", false).unwrap();
        assert_eq!(trashed_count, 1);
        assert!(
            h.index
                .get_note(&n1.note.note_id)
                .unwrap()
                .unwrap()
                .tombstoned
        );
        assert_eq!(h.vault.list_trash().unwrap().len(), 1);

        // 4. Move folder "Exportar" to a second Vault
        move_impl(&mut h, &n2.note.note_id, "Exportar").unwrap();
        let (_dir2, mut h2) = test_handle();
        let dest_root = h2.vault.root.clone();
        let transferred = move_folder_to_vault_impl(&mut h, "Exportar", &dest_root).unwrap();
        assert_eq!(transferred, 1);
        assert!(
            h.index
                .get_note(&n2.note.note_id)
                .unwrap()
                .unwrap()
                .tombstoned
        );
        h2.index.reconcile(&h2.vault).unwrap();
        let in_dest = h2.index.get_note(&n2.note.note_id).unwrap().unwrap();
        assert_eq!(in_dest.path, "notes/markdown/Exportar/Idea 2.md");
    }

    #[test]
    fn unlinked_mentions_excludes_explicit_links() {
        let (_dir, mut h) = test_handle();
        let target = create_impl(&mut h, "markdown", Some("Destino"), None).unwrap();
        let linked = create_impl(&mut h, "markdown", Some("Vinculado"), None).unwrap();
        let mentioned = create_impl(&mut h, "markdown", Some("Mencionado"), None).unwrap();
        save_impl(
            &mut h,
            &linked.note.note_id,
            "# Vinculado\n\nVer [[Destino]].",
            0,
        )
        .unwrap();
        save_impl(
            &mut h,
            &mentioned.note.note_id,
            "# Mencionado\n\nHablamos de Destino aquí.",
            0,
        )
        .unwrap();

        let mentions = unlinked_mentions_impl(&h, &target.note.note_id, "Destino").unwrap();
        let ids: Vec<&str> = mentions.iter().map(|m| m.source_id.as_str()).collect();
        assert!(
            ids.contains(&mentioned.note.note_id.as_str()),
            "plain mention should be listed"
        );
        assert!(
            !ids.contains(&linked.note.note_id.as_str()),
            "explicit link should be excluded"
        );
        assert!(
            !ids.contains(&target.note.note_id.as_str()),
            "target itself excluded"
        );
        assert!(mentions.iter().any(|m| m.snippet.contains("Destino")));
    }

    #[test]
    fn snippet_around_handles_multibyte_unicode_expansion_without_panic() {
        // 'İ' (U+0130, 2 bytes in UTF-8) lowercases to "i\u{307}" (3 bytes),
        // which previously caused `body[..byte_idx]` to panic or land off-boundary.
        let body = "İstanbul — ciudad histórica con muchas notas sobre Destino y más texto";
        let snippet = snippet_around(body, "Destino");
        assert!(snippet.contains("Destino"));
    }

    #[test]
    fn attachment_list_and_orphan_detection() {
        let (_dir, mut h) = test_handle();
        h.vault
            .write_atomic("attachments/photo.jpg", b"fake_jpeg")
            .unwrap();
        let atts = h.vault.scan_attachments().unwrap();
        h.index.reconcile_attachments(&atts).unwrap();

        // Initially orphan
        let list1 = attachment_list_impl(&h).unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].filename, "photo.jpg");
        assert!(list1[0].is_orphan);
        assert!(list1[0].referencing_notes.is_empty());

        // Reference it from a note
        let note = create_impl(&mut h, "markdown", Some("Mi Nota"), None).unwrap();
        save_impl(&mut h, &note.note.note_id, "# Mi Nota\n\n![[photo.jpg]]", 0).unwrap();

        let list2 = attachment_list_impl(&h).unwrap();
        assert_eq!(list2.len(), 1);
        assert!(!list2[0].is_orphan);
        assert_eq!(list2[0].referencing_notes.len(), 1);
        assert_eq!(list2[0].referencing_notes[0].title, "Mi Nota");
    }

    #[test]
    fn outlinks_and_link_repair_lifecycle() {
        let (_dir, mut h) = test_handle();
        let _target = create_impl(&mut h, "markdown", Some("Destino"), None).unwrap();
        let source = create_impl(&mut h, "markdown", Some("Origen"), None).unwrap();

        save_impl(
            &mut h,
            &source.note.note_id,
            "# Origen\n\nVer [[Destino]] y [[Destino#Capitulo|Cap]] y [[Nota Inexistente]].",
            0,
        )
        .unwrap();

        // Check outlinks
        let outlinks = outlinks_impl(&h, &source.note.note_id).unwrap();
        assert_eq!(outlinks.len(), 3);

        let resolved: Vec<_> = outlinks.iter().filter(|o| o.is_resolved).collect();
        let unresolved: Vec<_> = outlinks.iter().filter(|o| !o.is_resolved).collect();
        assert_eq!(
            resolved.len(),
            2,
            "Destino and Destino#Capitulo should be resolved"
        );
        assert_eq!(unresolved.len(), 1, "Nota Inexistente should be unresolved");
        assert_eq!(unresolved[0].target_raw, "Nota Inexistente");

        // Check references to Destino
        let refs = check_link_references_impl(&h, "Destino").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].note_id, source.note.note_id);
        assert_eq!(refs[0].count, 2);

        // Repair links when renaming Destino -> Destino Nuevo
        let res = repair_links_impl(&mut h, "Destino", "Destino Nuevo").unwrap();
        assert_eq!(res.affected_notes_count, 1);
        assert_eq!(res.updated_links_count, 2);

        // Read source note content to verify markdown file text
        let bytes = h.vault.read(&source.note.path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("[[Destino Nuevo]]"));
        assert!(text.contains("[[Destino Nuevo#Capitulo|Cap]]"));
        assert!(!text.contains("[[Destino]]"));
    }
}
