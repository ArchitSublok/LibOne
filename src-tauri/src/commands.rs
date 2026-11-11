use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::db::Database;
use crate::models::{AppInfo, IndexedFile, LibraryExport, ScanRoot, ScanStats};
use crate::scanner;

pub struct AppState {
    pub db: Mutex<Database>,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> Result<AppInfo, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?
        .to_string_lossy()
        .to_string();

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let db_path = db.get_path().to_string_lossy().to_string();

    Ok(AppInfo {
        app_name: "LibOne".to_string(),
        version: "0.1.0".to_string(),
        app_data_dir,
        db_path,
        os: std::env::consts::OS.to_string(),
    })
}

#[tauri::command]
pub fn add_scan_root(
    path: String,
    label: Option<String>,
    state: State<'_, AppState>,
) -> Result<ScanRoot, String> {
    let folder_path = Path::new(&path);
    if !folder_path.exists() {
        return Err(format!("Folder does not exist: {}", path));
    }
    if !folder_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let resolved_label = label.unwrap_or_else(|| {
        folder_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path)
            .to_string()
    });

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.add_scan_root(&path, &resolved_label)
        .map_err(|e| format!("Failed to add scan root: {}", e))
}

#[tauri::command]
pub fn remove_scan_root(root_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.remove_scan_root(root_id)
        .map_err(|e| format!("Failed to remove scan root: {}", e))
}

#[tauri::command]
pub fn list_scan_roots(state: State<'_, AppState>) -> Result<Vec<ScanRoot>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_scan_roots()
        .map_err(|e| format!("Failed to list scan roots: {}", e))
}

#[tauri::command]
pub fn scan_root(root_id: i64, state: State<'_, AppState>) -> Result<ScanStats, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let root = db
        .get_scan_root(root_id)
        .map_err(|e| format!("Failed to find scan root: {}", e))?
        .ok_or_else(|| format!("Scan root with ID {} not found", root_id))?;

    scanner::scan_folder(&db, root.id, &root.absolute_path)
}

#[tauri::command]
pub fn scan_all_roots(state: State<'_, AppState>) -> Result<Vec<ScanStats>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let roots = db
        .list_scan_roots()
        .map_err(|e| format!("Failed to retrieve scan roots: {}", e))?;

    let mut results = Vec::new();
    for root in roots {
        match scanner::scan_folder(&db, root.id, &root.absolute_path) {
            Ok(stats) => results.push(stats),
            Err(err) => eprintln!("Error scanning root {}: {}", root.absolute_path, err),
        }
    }

    Ok(results)
}

#[tauri::command]
pub fn list_indexed_files(
    query: Option<String>,
    root_id: Option<i64>,
    status_filter: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<IndexedFile>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_files(query, root_id, status_filter, limit, offset)
        .map_err(|e| format!("Failed to query files: {}", e))
}

#[tauri::command]
pub fn open_file_in_default_app(file_path: String) -> Result<(), String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File does not exist on disk: {}", file_path));
    }

    // Windows native open
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // Use cmd /c start "" "<file>" or direct shell execution
        Command::new("cmd")
            .args(["/c", "start", "", &file_path])
            .spawn()
            .map_err(|e| format!("Failed to launch Windows application: {}", e))?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        let prog = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        Command::new(prog)
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("Failed to launch default application: {}", e))?;
        Ok(())
    }
}

#[tauri::command]
pub fn export_library_metadata(
    target_file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<LibraryExport, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let export_data = db
        .export_library()
        .map_err(|e| format!("Failed to export library metadata: {}", e))?;

    if let Some(target) = target_file_path {
        let json_content = serde_json::to_string_pretty(&export_data)
            .map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&target, json_content)
            .map_err(|e| format!("Failed to write export to '{}': {}", target, e))?;
    }

    Ok(export_data)
}
