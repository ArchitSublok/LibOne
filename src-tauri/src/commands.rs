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
use crate::models::{
    AppInfo, FileDetail, FileQueryParams, IndexedFile, LibraryExport, PaginatedFiles, ScanOptions,
    ScanProgress, ScanRoot, ScanStats, WatcherOverallStatus,
};
use crate::scanner;
use crate::validate::{validate_scan_root_path, ValidationError};
use crate::watcher::WatcherManager;

pub struct AppState {
    pub db: Mutex<Database>,
    pub active_scans: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    pub scan_progress: Mutex<HashMap<i64, ScanProgress>>,
    pub watcher_manager: Mutex<WatcherManager>,
}

impl AppState {
    pub fn new(database: Database) -> Self {
        Self {
            db: Mutex::new(database),
            active_scans: Mutex::new(HashMap::new()),
            scan_progress: Mutex::new(HashMap::new()),
            watcher_manager: Mutex::new(WatcherManager::new()),
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

    let (root, db_clone) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;

        let r = if let Some(existing) = db.get_scan_root_by_path(canonical_str).map_err(db_err)? {
            if existing.enabled {
                return Err(ValidationError::Duplicate.to_string());
            }
            db.enable_scan_root(existing.id, &name).map_err(db_err)?;
            db.get_scan_root(existing.id)
                .map_err(db_err)?
                .ok_or_else(|| "Root disappeared after re-enable".to_string())?
        } else {
            db.add_scan_root(canonical_str, &name).map_err(db_err)?
        };
        (r, db.clone())
    };

    if let Ok(mut wm) = state.watcher_manager.lock() {
        wm.start_watching_root(&root, db_clone, app);
    }

    Ok(root)
}

#[tauri::command]
pub fn remove_scan_root(root_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.remove_scan_root(root_id).map_err(db_err)?;
    }
    if let Ok(mut wm) = state.watcher_manager.lock() {
        wm.stop_watching_root(root_id);
    }
    Ok(())
}

#[tauri::command]
pub fn enable_scan_root(
    root_id: i64,
    display_name: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanRoot, String> {
    let (root, db_clone) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;

        let existing = db
            .get_scan_root(root_id)
            .map_err(db_err)?
            .ok_or_else(|| format!("No scan root with id {root_id}"))?;

        let name = display_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(existing.display_name);

        db.enable_scan_root(root_id, &name).map_err(db_err)?;
        let r = db
            .get_scan_root(root_id)
            .map_err(db_err)?
            .ok_or_else(|| "Root disappeared after enable".to_string())?;
        (r, db.clone())
    };

    if let Ok(mut wm) = state.watcher_manager.lock() {
        wm.start_watching_root(&root, db_clone, app);
    }

    Ok(root)
}

#[tauri::command]
pub fn list_scan_roots(state: State<'_, AppState>) -> Result<Vec<ScanRoot>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_scan_roots().map_err(db_err)
}

#[tauri::command]
pub fn get_watcher_status(state: State<'_, AppState>) -> Result<WatcherOverallStatus, String> {
    let wm = state.watcher_manager.lock().map_err(|e| e.to_string())?;
    Ok(wm.get_overall_status())
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

#[tauri::command]
pub fn query_indexed_files(
    params: FileQueryParams,
    state: State<'_, AppState>,
) -> Result<PaginatedFiles, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.query_files(&params).map_err(db_err)
}

#[tauri::command]
pub fn get_indexed_file_detail(
    file_id: i64,
    state: State<'_, AppState>,
) -> Result<FileDetail, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_file_detail(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))
}

#[tauri::command]
pub fn get_library_extensions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_distinct_extensions().map_err(db_err)
}

// ── File access authorization & safe launching ───────────────────────────────

/// Validate an indexed file before opening or revealing it.
/// Requirements:
/// - Target must have a valid indexed file record in SQLite.
/// - Target must belong to an enabled scan root.
/// - Target path must be strictly inside the authorized scan root boundary.
/// - Target must still exist on disk (if not, marks record as missing in SQLite).
/// - Target must be a regular file (not directory or special device).
pub fn validate_file_for_access(db: &Database, file_id: i64) -> Result<IndexedFile, String> {
    let file = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| {
            format!("Authorization denied: File #{file_id} is not indexed in the library.")
        })?;

    let root = db
        .get_scan_root(file.scan_root_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Authorization denied: Scan root for file #{file_id} not found."))?;

    if !root.enabled {
        return Err(format!(
            "Access denied: Containing folder '{}' is currently disabled in LibOne. Enable the folder to access its files.",
            root.display_name
        ));
    }

    // Boundary check: normalized file path must begin with normalized root path
    let norm_file = file.normalized_path.replace('/', "\\");
    let norm_root = root.absolute_path.replace('/', "\\").to_lowercase();
    let norm_root_trimmed = norm_root.trim_end_matches('\\');

    if !norm_file.starts_with(norm_root_trimmed) {
        return Err(
            "Security violation: Target file path is outside authorized scan root boundaries."
                .to_string(),
        );
    }

    let p = Path::new(&file.absolute_path);
    if !p.exists() {
        let _ = db.touch_indexed_file(file_id, "missing");
        return Err(format!(
            "File unavailable: '{}' was not found on disk. The record has been marked as missing.",
            file.absolute_path
        ));
    }

    if !p.is_file() {
        return Err(format!(
            "Access denied: '{}' is a directory or special link, not a regular document.",
            file.absolute_path
        ));
    }

    Ok(file)
}

/// Safely open a file with the operating system's registered default application.
/// NEVER executes via `cmd.exe` or shell command string interpolation.
fn launch_file_safely(path_str: &str, ext: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide_path: Vec<u16> = OsStr::new(path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let wide_op: Vec<u16> = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lpOperation: *const u16,
                lpFile: *const u16,
                lpParameters: *const u16,
                lpDirectory: *const u16,
                nShowCmd: i32,
            ) -> *mut std::ffi::c_void;
        }

        let ret = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                wide_op.as_ptr(),
                wide_path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1, // SW_SHOWNORMAL
            )
        };

        let code = ret as usize;
        if code <= 32 {
            return Err(match code {
                31 => format!("No application is associated with '.{ext}' files on this computer."),
                2 => format!("File not found on disk: '{path_str}'"),
                5 => "Access was denied by the operating system.".to_string(),
                _ => format!("Windows failed to open file (code {code})."),
            });
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let prog = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(prog)
            .arg(path_str)
            .spawn()
            .map_err(|e| format!("Failed to open file: {e}"))?;
        Ok(())
    }
}

/// Open original file using the operating system's default application.
/// Validates path against indexed records and enabled scan root before launching.
#[tauri::command]
pub fn open_original_file(file_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let file = validate_file_for_access(&db, file_id)?;
    launch_file_safely(&file.absolute_path, &file.extension)
}

/// Show original file in Windows File Explorer (or reveal containing folder).
/// Uses native process argument passing (`explorer.exe /select,"<path>"`), never a shell.
#[tauri::command]
pub fn show_in_file_explorer(file_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let file = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Access denied: File #{file_id} not indexed in library."))?;

    let root = db
        .get_scan_root(file.scan_root_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Scan root for file #{file_id} not found."))?;

    if !root.enabled {
        return Err(format!(
            "Access denied: Containing scan root '{}' is disabled.",
            root.display_name
        ));
    }

    let p = Path::new(&file.absolute_path);
    let target_exists = p.exists();

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if target_exists {
            Command::new("explorer.exe")
                .arg(format!("/select,{}", file.absolute_path))
                .spawn()
                .map_err(|e| format!("Failed to launch File Explorer: {e}"))?;
            Ok(())
        } else {
            let _ = db.touch_indexed_file(file_id, "missing");
            if let Some(parent) = p.parent().filter(|par| par.exists()) {
                Command::new("explorer.exe")
                    .arg(parent.to_string_lossy().to_string())
                    .spawn()
                    .map_err(|e| format!("Failed to launch File Explorer: {e}"))?;
                Err(format!(
                    "File '{}' is missing on disk. Opened containing folder instead.",
                    file.file_name
                ))
            } else {
                Err(format!(
                    "Neither the file nor its containing folder exists on disk: '{}'",
                    file.absolute_path
                ))
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let prog = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let reveal_target = if target_exists {
            if cfg!(target_os = "macos") {
                return std::process::Command::new("open")
                    .args(["-R", &file.absolute_path])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string());
            }
            p.parent().unwrap_or(p).to_string_lossy().to_string()
        } else if let Some(parent) = p.parent().filter(|par| par.exists()) {
            let _ = db.touch_indexed_file(file_id, "missing");
            parent.to_string_lossy().to_string()
        } else {
            return Err(format!("File does not exist: {}", file.absolute_path));
        };

        std::process::Command::new(prog)
            .arg(reveal_target)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
        Ok(())
    }
}

/// Backwards-compatible file open command that validates against SQLite.
#[tauri::command]
pub fn open_file_in_default_app(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;

    // Try finding by path in indexed records
    let norm = file_path
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    let file = db
        .get_file_by_normalized_path(&norm)
        .map_err(db_err)?
        .ok_or_else(|| {
            format!("Security refusal: Path '{file_path}' is not an authorized file in LibOne.")
        })?;

    let authorized_file = validate_file_for_access(&db, file.id)?;
    launch_file_safely(&authorized_file.absolute_path, &authorized_file.extension)
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Validates that a LibraryExport is well-formed before serialization.
fn validate_export(export: &LibraryExport) -> Result<(), String> {
    if export.format_version.is_empty() {
        return Err("Export validation failed: missing format_version".into());
    }
    if export.app.is_empty() {
        return Err("Export validation failed: missing app name".into());
    }
    if export.version.is_empty() {
        return Err("Export validation failed: missing app version".into());
    }
    if export.exported_at.is_empty() {
        return Err("Export validation failed: missing export timestamp".into());
    }
    if export.summary.total_roots != export.scan_roots.len() {
        return Err(format!(
            "Export validation failed: summary total_roots ({}) does not match scan_roots length ({})",
            export.summary.total_roots,
            export.scan_roots.len()
        ));
    }
    if export.summary.total_files != export.files.len() {
        return Err(format!(
            "Export validation failed: summary total_files ({}) does not match files length ({})",
            export.summary.total_files,
            export.files.len()
        ));
    }
    Ok(())
}

/// Writes JSON to a temporary file next to the target, then atomically renames it.
/// This prevents partial/corrupt files if the process is interrupted during writing.
fn atomic_write_json(target_path: &str, json: &str) -> Result<(), String> {
    let target = PathBuf::from(target_path);
    let parent = target
        .parent()
        .ok_or_else(|| format!("Cannot determine parent directory for '{target_path}'"))?;

    if !parent.exists() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {e}", parent.display()))?;
    }

    // Write to a temporary file in the same directory (same filesystem for rename)
    let tmp_name = format!(
        ".libone-export-{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let tmp_path = parent.join(&tmp_name);

    std::fs::write(&tmp_path, json).map_err(|e| {
        format!(
            "Failed to write temporary export file '{}': {e}",
            tmp_path.display()
        )
    })?;

    // Atomic rename (on the same filesystem, this is atomic on NTFS and most POSIX)
    if let Err(e) = std::fs::rename(&tmp_path, &target) {
        // Rename failed — clean up temp file and report
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "Failed to finalize export file '{}': {e}",
            target_path
        ));
    }

    Ok(())
}

#[tauri::command]
pub fn export_library_metadata(
    target_file_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<LibraryExport, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let export_data = db
        .export_library()
        .map_err(|e| format!("Failed to export library metadata: {e}"))?;

    // Validate the export before serialization
    validate_export(&export_data)?;

    if let Some(target) = target_file_path {
        let json = serde_json::to_string_pretty(&export_data)
            .map_err(|e| format!("Serialization error: {e}"))?;
        atomic_write_json(&target, &json)?;
    }

    Ok(export_data)
}

// ── On-demand hashing ─────────────────────────────────────────────────────────

/// Compute SHA-256 hash on demand for a single file using streaming I/O and persist it.
#[tauri::command]
pub fn compute_file_hash(file_id: i64, state: State<'_, AppState>) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_auth_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "libone_auth_test_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn test_auth_db(name: &str) -> (Database, PathBuf) {
        let p = std::env::temp_dir().join(format!(
            "libone_auth_db_{}_{}_{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        (Database::new(&p).unwrap(), p)
    }

    #[test]
    fn test_authorization_succeeds_for_valid_indexed_file() {
        let dir = temp_auth_dir("auth_ok");
        let (db, db_dir) = test_auth_db("auth_ok");
        let file_path = dir.join("valid_doc.pdf");
        fs::write(&file_path, b"%PDF-1.4 sample content").unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Test Root")
            .unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().to_string(),
            normalized_path: file_path
                .to_str()
                .unwrap()
                .replace('/', "\\")
                .to_lowercase(),
            file_name: "valid_doc.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 23,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        let indexed = db.insert_indexed_file(&new_file).unwrap();

        let res = validate_file_for_access(&db, indexed.id);
        assert!(
            res.is_ok(),
            "Validation should succeed for a valid indexed file"
        );
        let validated = res.unwrap();
        assert_eq!(validated.file_name, "valid_doc.pdf");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_authorization_rejects_unindexed_file() {
        let (_db, db_dir) = test_auth_db("auth_unindexed");
        let res = validate_file_for_access(&_db, 999999);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("not indexed in the library"));
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_authorization_rejects_disabled_scan_root() {
        let dir = temp_auth_dir("auth_disabled");
        let (db, db_dir) = test_auth_db("auth_disabled");
        let file_path = dir.join("secret.docx");
        fs::write(&file_path, b"content").unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Disabled Root")
            .unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().to_string(),
            normalized_path: file_path
                .to_str()
                .unwrap()
                .replace('/', "\\")
                .to_lowercase(),
            file_name: "secret.docx".into(),
            extension: "docx".into(),
            size_bytes: 7,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        let indexed = db.insert_indexed_file(&new_file).unwrap();

        // Disable root
        db.remove_scan_root(root.id).unwrap();

        let res = validate_file_for_access(&db, indexed.id);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("currently disabled"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_authorization_handles_missing_file_and_updates_status() {
        let dir = temp_auth_dir("auth_missing");
        let (db, db_dir) = test_auth_db("auth_missing");
        let file_path = dir.join("deleted_doc.txt");
        fs::write(&file_path, b"soon deleted").unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Missing Root")
            .unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().to_string(),
            normalized_path: file_path
                .to_str()
                .unwrap()
                .replace('/', "\\")
                .to_lowercase(),
            file_name: "deleted_doc.txt".into(),
            extension: "txt".into(),
            size_bytes: 12,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        let indexed = db.insert_indexed_file(&new_file).unwrap();

        // Delete from disk
        fs::remove_file(&file_path).unwrap();

        let res = validate_file_for_access(&db, indexed.id);
        assert!(res.is_err(), "Access should fail for missing file");
        assert!(res.unwrap_err().contains("marked as missing"));

        // Check that database status was updated to missing
        let updated = db.get_indexed_file(indexed.id).unwrap().unwrap();
        assert_eq!(updated.status, "missing");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_authorization_rejects_directory_path() {
        let dir = temp_auth_dir("auth_dir");
        let (db, db_dir) = test_auth_db("auth_dir");
        let sub = dir.join("subfolder");
        fs::create_dir_all(&sub).unwrap();

        let root = db.add_scan_root(dir.to_str().unwrap(), "Dir Root").unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: sub.to_str().unwrap().to_string(),
            normalized_path: sub.to_str().unwrap().replace('/', "\\").to_lowercase(),
            file_name: "subfolder".into(),
            extension: "".into(),
            size_bytes: 0,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        let indexed = db.insert_indexed_file(&new_file).unwrap();

        let res = validate_file_for_access(&db, indexed.id);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("is a directory"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_authorization_rejects_path_traversal_outside_root() {
        let dir = temp_auth_dir("auth_outside");
        let other_dir = temp_auth_dir("other_dir");
        let (db, db_dir) = test_auth_db("auth_outside");
        let file_path = other_dir.join("outside.pdf");
        fs::write(&file_path, b"outside content").unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Inside Root")
            .unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().to_string(),
            normalized_path: file_path
                .to_str()
                .unwrap()
                .replace('/', "\\")
                .to_lowercase(),
            file_name: "outside.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 15,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        let indexed = db.insert_indexed_file(&new_file).unwrap();

        let res = validate_file_for_access(&db, indexed.id);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("outside authorized scan root"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&other_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    // ── Export tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_export_validation_passes_for_valid_export() {
        let export = LibraryExport {
            format_version: "1.0.0".into(),
            app: "LibOne".into(),
            version: "0.1.0".into(),
            exported_at: "2026-01-01T00:00:00Z".into(),
            summary: crate::models::ExportSummary {
                total_roots: 0,
                total_files: 0,
                total_size_bytes: 0,
                export_timestamp: "2026-01-01T00:00:00Z".into(),
            },
            scan_roots: vec![],
            files: vec![],
        };
        assert!(validate_export(&export).is_ok());
    }

    #[test]
    fn test_export_validation_fails_missing_format_version() {
        let export = LibraryExport {
            format_version: "".into(),
            app: "LibOne".into(),
            version: "0.1.0".into(),
            exported_at: "2026-01-01T00:00:00Z".into(),
            summary: crate::models::ExportSummary {
                total_roots: 0,
                total_files: 0,
                total_size_bytes: 0,
                export_timestamp: "2026-01-01T00:00:00Z".into(),
            },
            scan_roots: vec![],
            files: vec![],
        };
        let err = validate_export(&export).unwrap_err();
        assert!(err.contains("format_version"));
    }

    #[test]
    fn test_export_validation_fails_mismatched_counts() {
        let export = LibraryExport {
            format_version: "1.0.0".into(),
            app: "LibOne".into(),
            version: "0.1.0".into(),
            exported_at: "2026-01-01T00:00:00Z".into(),
            summary: crate::models::ExportSummary {
                total_roots: 5, // doesn't match scan_roots.len() == 0
                total_files: 0,
                total_size_bytes: 0,
                export_timestamp: "2026-01-01T00:00:00Z".into(),
            },
            scan_roots: vec![],
            files: vec![],
        };
        let err = validate_export(&export).unwrap_err();
        assert!(err.contains("total_roots"));
    }

    #[test]
    fn test_export_json_schema_contains_required_fields() {
        let (db, db_dir) = test_auth_db("export_schema");

        let export = db.export_library().unwrap();
        let json_str = serde_json::to_string_pretty(&export).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        // Verify top-level fields
        assert!(
            parsed.get("format_version").is_some(),
            "missing format_version"
        );
        assert!(parsed.get("app").is_some(), "missing app");
        assert!(parsed.get("version").is_some(), "missing version");
        assert!(parsed.get("exported_at").is_some(), "missing exported_at");
        assert!(parsed.get("summary").is_some(), "missing summary");
        assert!(parsed.get("scan_roots").is_some(), "missing scan_roots");
        assert!(parsed.get("files").is_some(), "missing files");

        // Verify summary sub-fields
        let summary = parsed.get("summary").unwrap();
        assert!(summary.get("total_roots").is_some());
        assert!(summary.get("total_files").is_some());
        assert!(summary.get("total_size_bytes").is_some());
        assert!(summary.get("export_timestamp").is_some());

        // format_version should be "1.0.0"
        assert_eq!(parsed["format_version"].as_str().unwrap(), "1.0.0");

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_atomic_write_creates_valid_file() {
        let dir = temp_auth_dir("atomic_write");
        let target = dir.join("export.json");
        let json = r#"{"format_version":"1.0.0","app":"LibOne"}"#;

        let res = atomic_write_json(target.to_str().unwrap(), json);
        assert!(res.is_ok());

        let contents = fs::read_to_string(&target).unwrap();
        assert_eq!(contents, json);

        // Ensure no temp files remain
        let remaining: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(remaining.len(), 1, "Only the target file should remain");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_atomic_write_fails_for_readonly_directory() {
        // Writing to a non-existent drive path should fail
        let bad_path = "Z:\\non_existent_drive\\export.json";
        let res = atomic_write_json(bad_path, "{}");
        assert!(res.is_err());
    }

    #[test]
    fn test_export_roundtrip_deserializes() {
        let (db, db_dir) = test_auth_db("export_roundtrip");
        let dir = temp_auth_dir("export_rt_files");
        let file_path = dir.join("sample.pdf");
        fs::write(&file_path, b"PDF content").unwrap();

        let root = db.add_scan_root(dir.to_str().unwrap(), "RT Root").unwrap();
        let new_file = crate::models::NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().to_string(),
            normalized_path: file_path
                .to_str()
                .unwrap()
                .replace('/', "\\")
                .to_lowercase(),
            file_name: "sample.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 11,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        };
        db.insert_indexed_file(&new_file).unwrap();

        let export = db.export_library().unwrap();
        let json_str = serde_json::to_string_pretty(&export).unwrap();

        // Deserialize back and verify round-trip
        let deserialized: LibraryExport = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.format_version, "1.0.0");
        assert_eq!(deserialized.app, "LibOne");
        assert_eq!(deserialized.summary.total_roots, 1);
        assert_eq!(deserialized.summary.total_files, 1);
        assert_eq!(deserialized.files[0].file_name, "sample.pdf");
        assert_eq!(deserialized.scan_roots[0].display_name, "RT Root");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }
}
