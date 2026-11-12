use tauri::Manager;

pub mod commands;
pub mod db;
pub mod models;
pub mod scanner;
pub mod validate;

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

            app.manage(AppState::new(database));

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running LibOne application");
}
