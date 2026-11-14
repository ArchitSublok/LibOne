use tauri::Manager;

pub mod commands;
pub mod db;
pub mod models;
pub mod scanner;
pub mod validate;
pub mod watcher;

use commands::AppState;
use db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to determine app data directory");

            let database =
                Database::new(app_data_dir).expect("Failed to initialize SQLite database");

            let app_state = AppState::new(database.clone());

            // Initialize watchers for all enabled scan roots while app runs
            if let Ok(roots) = database.list_scan_roots() {
                if let Ok(mut wm) = app_state.watcher_manager.lock() {
                    for root in roots.into_iter().filter(|r| r.enabled) {
                        wm.start_watching_root(&root, database.clone(), app.handle().clone());
                    }
                }
            }

            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::add_scan_root,
            commands::remove_scan_root,
            commands::enable_scan_root,
            commands::list_scan_roots,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_scan_progress,
            commands::scan_root,
            commands::scan_all_roots,
            commands::list_indexed_files,
            commands::query_indexed_files,
            commands::get_indexed_file_detail,
            commands::get_library_extensions,
            commands::open_original_file,
            commands::show_in_file_explorer,
            commands::open_file_in_default_app,
            commands::export_library_metadata,
            commands::compute_file_hash,
            commands::get_watcher_status,
            // Phase 2: Collections
            commands::create_collection,
            commands::get_collection,
            commands::list_collections,
            commands::update_collection,
            commands::delete_collection,
            // Phase 2: Tags
            commands::create_tag,
            commands::get_tag,
            commands::list_tags,
            commands::update_tag,
            commands::delete_tag,
            // Phase 2: File-Collection Links
            commands::add_file_to_collection,
            commands::remove_file_from_collection,
            commands::list_file_collections,
            commands::bulk_add_file_collections,
            commands::bulk_remove_file_collections,
            commands::set_file_collections,
            // Phase 2: File-Tag Links
            commands::add_file_to_tag,
            commands::remove_file_from_tag,
            commands::list_file_tags,
            commands::bulk_add_file_tags,
            commands::bulk_remove_file_tags,
            commands::set_file_tags,
            // Phase 2: File Notes
            commands::create_file_note,
            commands::get_file_note,
            commands::list_file_notes,
            commands::update_file_note,
            commands::delete_file_note,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LibOne application");
}
