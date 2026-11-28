mod commands;
mod dto;
mod error;
mod state;
mod sync;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Official logging plugin: routes `log::` records to stdout, the app log
        // directory and - on Android - logcat (where `eprintln!` is invisible).
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            if let Ok(dir) = app.path().app_data_dir() {
                sync::init_install_id(&dir);
            }
            app.manage(state::AppState::new());
            #[cfg(desktop)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    if let Some(icon) = app.default_window_icon() {
                        let _ = window.set_icon(icon.clone());
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_init,
            commands::vault_open,
            commands::vault_create_named,
            commands::vault_rename_current,
            commands::vault_delete_profile,
            commands::vault_list_profiles,
            commands::vault_export_bundle,
            commands::vault_import_bundle,
            commands::vault_get_settings,
            commands::vault_update_settings,
            commands::vault_rebuild_index,
            commands::vault_pick_folder,
            commands::vault_create_folder,
            commands::vault_rename_folder,
            commands::vault_delete_folder,
            commands::vault_move_folder_to_vault,
            commands::vault_list_folders,
            commands::vault_import_attachment,
            commands::vault_read_attachment,
            commands::attachment_list,
            commands::attachment_delete,
            commands::attachment_delete_orphans,
            commands::attachments_to_compress,
            commands::attachment_mark_checked,
            commands::attachment_replace,
            commands::note_list,
            commands::note_create,
            commands::note_read,
            commands::note_save,
            commands::note_rename,
            commands::note_move,
            commands::note_delete,
            commands::trash_list,
            commands::trash_restore,
            commands::trash_delete_item,
            commands::trash_empty,
            commands::note_search_fts,
            commands::note_get_backlinks,
            commands::note_get_outlinks,
            commands::note_check_link_references,
            commands::note_repair_links,
            commands::note_unlinked_mentions,
            commands::note_suggest_titles,
            commands::get_sync_status,
            commands::sync_now,
            commands::configure_sync_server,
            commands::pause_sync,
            commands::resume_sync,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(mobile)]
        {
            // Lifecycle resume-reconcile: on Android there is no inotify/FSEvents
            // watcher, so re-index the vault whenever the app returns to foreground.
            if let tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Resumed,
                ..
            } = event
            {
                commands::reconcile_on_resume(app_handle);
            }
        }
        #[cfg(not(mobile))]
        {
            let _ = (app_handle, event);
        }
    });
}
