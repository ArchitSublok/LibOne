//! Tauri IPC commands — the only public surface between the Rust backend
//! and the React frontend.  All database access is delegated to `db.rs`;
//! all path validation is delegated to `validate.rs`;
//! all file crawling is delegated to `scanner.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Database;
use crate::models::{AppInfo, IndexedFile, LibraryExport, ScanOptions, ScanProgress, ScanRoot, ScanStats};
use crate::scanner;
use crate::validate::{validate_scan_root_path, ValidationError};

pub struct AppState {
    pub db: Mutex<Database>,
    pub active_scans: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    pub scan_progress: Mutex<HashMap<i64, ScanProgress>>,
}

impl AppState {
    pub fn new(database: Database) -> Self {
        Self {
            db: Mutex::new(database),
            active_scans: Mutex::new(HashMap::new()),
            scan_progress: Mutex::new(HashMap::new()),
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn db_err(e: rusqlite::Error) -> String {
    format!("Database error: {e}")
}

fn val_err(e: ValidationError) -> String {
    e.to_string()
}

// ── App info ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_app_info(app: AppHandle, state: State<'_, AppState>) -> Result<AppInfo, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {e}"))?
        .to_string_lossy()
        .to_string();

    let db = state.db.lock().map_err(|e| e.to_string())?;
    let db_path = db.get_path().to_string_lossy().to_string();

    Ok(AppInfo {
        app_name: "LibOne".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        app_data_dir,
        db_path,
        os: std::env::consts::OS.to_string(),
    })
}

// ── Scan-root management ──────────────────────────────────────────────────────

#[tauri::command]
pub fn add_scan_root(
    path: String,
    display_name: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanRoot, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {e}"))?;

    let canonical = validate_scan_root_path(&path, &app_data_dir).map_err(val_err)?;
    let canonical_str = canonical.to_str().unwrap();

    let name = display_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            canonical
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(canonical_str)
                .to_string()
        });

    let db = state.db.lock().map_err(|e| e.to_string())?;

    if let Some(existing) = db.get_scan_root_by_path(canonical_str).map_err(db_err)? {
        if existing.enabled {
            return Err(ValidationError::Duplicate.to_string());
        }
        db.enable_scan_root(existing.id, &name).map_err(db_err)?;
        return db
            .get_scan_root(existing.id)
            .map_err(db_err)?
            .ok_or_else(|| "Root disappeared after re-enable".to_string());
    }

    db.add_scan_root(canonical_str, &name).map_err(db_err)
}

#[tauri::command]
pub fn remove_scan_root(root_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.remove_scan_root(root_id).map_err(db_err)
}

#[tauri::command]
pub fn enable_scan_root(
    root_id: i64,
    display_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<ScanRoot, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    let existing = db
        .get_scan_root(root_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("No scan root with id {root_id}"))?;

    let name = display_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(existing.display_name);

    db.enable_scan_root(root_id, &name).map_err(db_err)?;
    db.get_scan_root(root_id)
        .map_err(db_err)?
        .ok_or_else(|| "Root disappeared after enable".to_string())
}

#[tauri::command]
pub fn list_scan_roots(state: State<'_, AppState>) -> Result<Vec<ScanRoot>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_scan_roots().map_err(db_err)
}

// ── Scanning with real-time progress & cancellation ───────────────────────────

/// Start scanning a selected root in the background. Emits `scan-progress` events.
/// Pass `options` to enable SHA-256 hashing during the scan (off by default).
#[tauri::command]
pub fn start_scan(
    root_id: i64,
    options: Option<ScanOptions>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanProgress, String> {
    let (db_clone, root) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let root = db
            .get_scan_root(root_id)
            .map_err(db_err)?
            .ok_or_else(|| format!("Scan root {root_id} not found"))?;
        (db.clone(), root)
    };

    if !root.enabled {
        return Err(format!("Scan root '{}' is disabled.", root.display_name));
    }

    // Cancellation token for this scan
    let cancel_token = Arc::new(AtomicBool::new(false));
    {
        let mut active = state.active_scans.lock().map_err(|e| e.to_string())?;
        active.insert(root_id, cancel_token.clone());
    }

    let initial_progress = ScanProgress {
        root_id,
        current_root: root.absolute_path.clone(),
        current_directory: root.absolute_path.clone(),
        directories_visited: 0,
        files_discovered: 0,
        files_indexed: 0,
        warnings: Vec::new(),
        state: "running".into(),
        stats: None,
        error_message: None,
    };

    {
        let mut progs = state.scan_progress.lock().map_err(|e| e.to_string())?;
        progs.insert(root_id, initial_progress.clone());
    }

    let app_data_dir: Option<PathBuf> = app.path().app_data_dir().ok();
    let app_handle = app.clone();

    // Spawn scanner in a worker thread
    std::thread::spawn(move || {
        let folder_path = root.absolute_path.clone();
        let app_handle_for_events = app_handle.clone();
        let scan_opts = options.clone();

        let _ = scanner::scan_folder_with_progress(
            &db_clone,
            root_id,
            &folder_path,
            app_data_dir.as_deref(),
            scan_opts.as_ref(),
            Some(&cancel_token),
            |progress| {
                // Emit progress to frontend via Tauri event
                let _ = app_handle_for_events.emit("scan-progress", progress);
                if let Some(state) = app_handle_for_events.try_state::<AppState>() {
                    if let Ok(mut map) = state.scan_progress.lock() {
                        map.insert(root_id, progress.clone());
                    }
                }
            },
        );

        if let Some(state) = app_handle.try_state::<AppState>() {
            if let Ok(mut active) = state.active_scans.lock() {
                active.remove(&root_id);
            }
        }
    });

    Ok(initial_progress)
}

/// Cancel an ongoing scan for a scan root.
#[tauri::command]
pub fn cancel_scan(root_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let active = state.active_scans.lock().map_err(|e| e.to_string())?;
    if let Some(token) = active.get(&root_id) {
        token.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Query the latest scan progress for a root.
#[tauri::command]
pub fn get_scan_progress(
    root_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<ScanProgress>, String> {
    let progs = state.scan_progress.lock().map_err(|e| e.to_string())?;
    Ok(progs.get(&root_id).cloned())
}

/// Synchronous scan_root helper for backwards-compatibility or direct calls.
#[tauri::command]
pub fn scan_root(
    root_id: i64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanStats, String> {
    let (db_clone, root) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let root = db
            .get_scan_root(root_id)
            .map_err(db_err)?
            .ok_or_else(|| format!("Scan root with id {root_id} not found"))?;
        (db.clone(), root)
    };

    if !root.enabled {
        return Err(format!(
            "Scan root '{}' is disabled. Enable it first.",
            root.display_name
        ));
    }

    let app_data_dir: Option<PathBuf> = app.path().app_data_dir().ok();
    let app_handle = app.clone();

    let progress = scanner::scan_folder_with_progress(
        &db_clone,
        root.id,
        &root.absolute_path,
        app_data_dir.as_deref(),
        None,
        None,
        |p| {
            let _ = app_handle.emit("scan-progress", p);
            if let Some(st) = app_handle.try_state::<AppState>() {
                if let Ok(mut map) = st.scan_progress.lock() {
                    map.insert(root_id, p.clone());
                }
            }
        },
    )?;

    progress
        .stats
        .ok_or_else(|| "Scan completed without producing summary stats".to_string())
}

#[tauri::command]
pub fn scan_all_roots(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ScanStats>, String> {
    let (db_clone, enabled_roots) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let roots = db.list_scan_roots().map_err(db_err)?;
        let enabled: Vec<_> = roots.into_iter().filter(|r| r.enabled).collect();
        (db.clone(), enabled)
    };

    let app_data_dir: Option<PathBuf> = app.path().app_data_dir().ok();
    let mut results = Vec::new();

    for root in enabled_roots {
        let app_handle = app.clone();
        let res = scanner::scan_folder_with_progress(
            &db_clone,
            root.id,
            &root.absolute_path,
            app_data_dir.as_deref(),
            None,
            None,
            |p| {
                let _ = app_handle.emit("scan-progress", p);
                if let Some(st) = app_handle.try_state::<AppState>() {
                    if let Ok(mut map) = st.scan_progress.lock() {
                        map.insert(root.id, p.clone());
                    }
                }
            },
        );

        match res {
            Ok(p) => {
                if let Some(stats) = p.stats {
                    results.push(stats);
                }
            }
            Err(err) => eprintln!("[LibOne] Error scanning '{}': {err}", root.absolute_path),
        }
    }
    Ok(results)
}

// ── File queries ──────────────────────────────────────────────────────────────

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
        .map_err(db_err)
}

// ── File open ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn open_file_in_default_app(file_path: String) -> Result<(), String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File no longer exists on disk: {file_path}"));
    }
    if !path.is_file() {
        return Err(format!("Path is not a regular file: {file_path}"));
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(["/c", "start", "", &file_path])
            .spawn()
            .map_err(|e| format!("Failed to open file with Windows default application: {e}"))?;
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
            .map_err(|e| format!("Failed to open file: {e}"))?;
        Ok(())
    }
}

// ── Export ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn export_library_metadata(
    target_file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<LibraryExport, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let export_data = db
        .export_library()
        .map_err(|e| format!("Failed to export library metadata: {e}"))?;

    if let Some(target) = target_file_path {
        let json = serde_json::to_string_pretty(&export_data)
            .map_err(|e| format!("Serialization error: {e}"))?;
        std::fs::write(&target, json)
            .map_err(|e| format!("Failed to write export to '{target}': {e}"))?;
    }

    Ok(export_data)
}

// ── On-demand hashing ─────────────────────────────────────────────────────────

/// Compute SHA-256 hash on demand for a single file using streaming I/O and persist it.
#[tauri::command]
pub fn compute_file_hash(
    file_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let path_str = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let file = db
            .get_indexed_file(file_id)
            .map_err(db_err)?
            .ok_or_else(|| format!("Indexed file {file_id} not found"))?;
        file.absolute_path
    };

    let p = Path::new(&path_str);
    if !p.exists() {
        return Err(format!("File does not exist: {path_str}"));
    }

    let hash = scanner::compute_file_sha256(p)
        .map_err(|e| format!("Failed to compute SHA-256 for '{path_str}': {e}"))?;

    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.update_file_hash(file_id, &hash).map_err(db_err)?;

    Ok(hash)
}
