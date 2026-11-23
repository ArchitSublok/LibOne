//! Tauri IPC commands — the only public surface between the Rust backend
//! and the React frontend.  All database access is delegated to `db.rs`;
//! all path validation is delegated to `validate.rs`;
//! all file crawling is delegated to `scanner.rs`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Database;
use crate::intelligence::{DisabledLocalProvider, LocalAiStatus, LocalIntelligenceProvider};
use crate::models::{
    AppInfo, BulkSuggestionResult, Collection, DuplicateGroup, DuplicateGroupDetail,
    FileCollectionLink, FileComparisonItem, FileDetail, FileNote, FileQueryParams, FileTagLink,
    IndexedFile, LibraryExport, OrganizationSuggestion, PaginatedFiles, ScanOptions, ScanProgress,
    ScanRoot, ScanStats, Tag, WatcherOverallStatus,
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

pub fn validate_positive_id(id: i64, entity: &str) -> Result<(), String> {
    if id <= 0 {
        return Err(format!(
            "Authorization denied: Invalid ID: {entity} #{id} must be a positive integer."
        ));
    }
    Ok(())
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(root_id, "Scan root")?;
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
    validate_positive_id(file_id, "File")?;
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
    validate_positive_id(file_id, "File")?;
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
    let norm_file = file.normalized_path.replace('/', "\\").to_lowercase();
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
    validate_positive_id(file_id, "File")?;
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

    // Boundary check: normalized file path must begin with normalized root path
    let norm_file = file.normalized_path.replace('/', "\\").to_lowercase();
    let norm_root = root.absolute_path.replace('/', "\\").to_lowercase();
    let norm_root_trimmed = norm_root.trim_end_matches('\\');

    if !norm_file.starts_with(norm_root_trimmed) {
        return Err(
            "Security violation: Target file path is outside authorized scan root boundaries."
                .to_string(),
        );
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
///
/// This guards both the Phase 1 fields and the Phase 2 organization/notes
/// arrays, including referential integrity between files, collections, tags,
/// links, and notes.
fn validate_export(export: &LibraryExport) -> Result<(), String> {
    fn invalid(message: impl AsRef<str>) -> Result<(), String> {
        Err(format!("Export validation failed: {}", message.as_ref()))
    }

    if export.format_version.is_empty() {
        return invalid("missing format_version");
    }
    if export.app.is_empty() {
        return invalid("missing app name");
    }
    if export.version.is_empty() {
        return invalid("missing app version");
    }
    if export.exported_at.is_empty() {
        return invalid("missing export timestamp");
    }
    if export.summary.export_timestamp != export.exported_at {
        return invalid("summary export_timestamp does not match exported_at");
    }
    if export.summary.total_roots != export.scan_roots.len() {
        return invalid(format!(
            "summary total_roots ({}) does not match scan_roots length ({})",
            export.summary.total_roots,
            export.scan_roots.len()
        ));
    }
    if export.summary.total_files != export.files.len() {
        return invalid(format!(
            "summary total_files ({}) does not match files length ({})",
            export.summary.total_files,
            export.files.len()
        ));
    }
    let expected_size: i64 = export.files.iter().map(|f| f.size_bytes).sum();
    if export.summary.total_size_bytes != expected_size {
        return invalid(format!(
            "summary total_size_bytes ({}) does not match the sum of file sizes ({expected_size})",
            export.summary.total_size_bytes
        ));
    }

    let root_ids: HashSet<i64> = export.scan_roots.iter().map(|r| r.id).collect();
    if export.scan_roots.iter().any(|r| r.id <= 0) || root_ids.len() != export.scan_roots.len() {
        return invalid("scan_roots must have unique positive IDs");
    }

    let file_ids: HashSet<i64> = export.files.iter().map(|f| f.id).collect();
    if export.files.iter().any(|f| f.id <= 0) || file_ids.len() != export.files.len() {
        return invalid("files must have unique positive IDs");
    }
    if export
        .files
        .iter()
        .any(|f| f.scan_root_id <= 0 || !root_ids.contains(&f.scan_root_id))
    {
        return invalid("file references an unknown scan_root_id");
    }

    let collection_ids: HashSet<i64> = export.collections.iter().map(|c| c.id).collect();
    if export
        .collections
        .iter()
        .any(|c| c.id <= 0 || c.name.trim().is_empty())
        || collection_ids.len() != export.collections.len()
    {
        return invalid("collections must have unique positive IDs and non-empty names");
    }

    let tag_ids: HashSet<i64> = export.tags.iter().map(|t| t.id).collect();
    if export
        .tags
        .iter()
        .any(|t| t.id <= 0 || t.name.trim().is_empty())
        || tag_ids.len() != export.tags.len()
    {
        return invalid("tags must have unique positive IDs and non-empty names");
    }

    let mut seen_collection_links: HashSet<(i64, i64)> = HashSet::new();
    for link in &export.file_collections {
        if !file_ids.contains(&link.file_id) || !collection_ids.contains(&link.collection_id) {
            return invalid("file_collections contains an unknown file_id or collection_id");
        }
        if !seen_collection_links.insert((link.file_id, link.collection_id)) {
            return invalid("file_collections contains a duplicate link");
        }
    }

    let mut seen_tag_links: HashSet<(i64, i64)> = HashSet::new();
    for link in &export.file_tags {
        if !file_ids.contains(&link.file_id) || !tag_ids.contains(&link.tag_id) {
            return invalid("file_tags contains an unknown file_id or tag_id");
        }
        if !seen_tag_links.insert((link.file_id, link.tag_id)) {
            return invalid("file_tags contains a duplicate link");
        }
    }

    let mut seen_note_ids: HashSet<i64> = HashSet::new();
    for note in &export.file_notes {
        if note.id <= 0 || !seen_note_ids.insert(note.id) {
            return invalid("file_notes must have unique positive IDs");
        }
        if !file_ids.contains(&note.file_id) {
            return invalid("file_notes contains an unknown file_id");
        }
        if note.note_text.trim().is_empty() {
            return invalid("file_notes contains an empty note");
        }
        if note.created_at.is_empty() || note.updated_at.is_empty() {
            return invalid("file_notes is missing a timestamp");
        }
    }

    // Phase 3: Organization suggestions validation
    let mut suggestion_ids: HashSet<i64> = HashSet::new();
    for suggestion in &export.organization_suggestions {
        if suggestion.id <= 0 || !suggestion_ids.insert(suggestion.id) {
            return invalid("organization_suggestions must have unique positive IDs");
        }
        if suggestion.suggestion_type.trim().is_empty() {
            return invalid("organization_suggestions contains an empty suggestion_type");
        }
        if suggestion.title.trim().is_empty() {
            return invalid("organization_suggestions contains an empty title");
        }
        if !(0.0..=1.0).contains(&suggestion.confidence) || suggestion.confidence.is_nan() {
            return invalid("organization_suggestions confidence must be between 0.0 and 1.0");
        }
        match suggestion.status.as_str() {
            "pending" | "approved" | "rejected" | "dismissed" => {}
            _ => {
                return invalid(format!(
                    "organization_suggestions contains invalid status '{}'",
                    suggestion.status
                ))
            }
        }
        if suggestion.created_at.trim().is_empty() {
            return invalid("organization_suggestions contains missing created_at timestamp");
        }
        if let Some(reviewed_at) = &suggestion.reviewed_at {
            if reviewed_at.trim().is_empty() {
                return invalid(
                    "organization_suggestions reviewed_at cannot be empty if specified",
                );
            }
        }
    }

    // Phase 3: Suggestion files validation
    let mut seen_suggestion_files: HashSet<(i64, i64)> = HashSet::new();
    for link in &export.suggestion_files {
        if !suggestion_ids.contains(&link.suggestion_id) {
            return invalid("suggestion_files contains an unknown suggestion_id");
        }
        if !file_ids.contains(&link.file_id) {
            return invalid("suggestion_files contains an unknown file_id");
        }
        if !seen_suggestion_files.insert((link.suggestion_id, link.file_id)) {
            return invalid("suggestion_files contains a duplicate link");
        }
    }

    // Phase 3: Duplicate groups validation
    let mut duplicate_group_ids: HashSet<i64> = HashSet::new();
    for group in &export.duplicate_groups {
        if group.id <= 0 || !duplicate_group_ids.insert(group.id) {
            return invalid("duplicate_groups must have unique positive IDs");
        }
        if group.match_type.trim().is_empty() {
            return invalid("duplicate_groups contains an empty match_type");
        }
        if !(0.0..=1.0).contains(&group.confidence) || group.confidence.is_nan() {
            return invalid("duplicate_groups confidence must be between 0.0 and 1.0");
        }
        match group.status.as_str() {
            "pending" | "approved" | "rejected" | "dismissed" => {}
            _ => {
                return invalid(format!(
                    "duplicate_groups contains invalid status '{}'",
                    group.status
                ))
            }
        }
        if group.created_at.trim().is_empty() {
            return invalid("duplicate_groups contains missing created_at timestamp");
        }
    }

    // Phase 3: Duplicate group files validation
    let mut seen_duplicate_group_files: HashSet<(i64, i64)> = HashSet::new();
    for link in &export.duplicate_group_files {
        if !duplicate_group_ids.contains(&link.group_id) {
            return invalid("duplicate_group_files contains an unknown group_id");
        }
        if !file_ids.contains(&link.file_id) {
            return invalid("duplicate_group_files contains an unknown file_id");
        }
        if !seen_duplicate_group_files.insert((link.group_id, link.file_id)) {
            return invalid("duplicate_group_files contains a duplicate link");
        }
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
    validate_positive_id(file_id, "File")?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let file = validate_file_for_access(&db, file_id)?;

    let p = Path::new(&file.absolute_path);
    if !p.exists() {
        return Err(format!("File does not exist: {}", file.absolute_path));
    }

    let hash = scanner::compute_file_sha256(p).map_err(|e| {
        format!(
            "Failed to compute SHA-256 for '{}': {e}",
            file.absolute_path
        )
    })?;

    db.update_file_hash(file_id, &hash).map_err(db_err)?;

    Ok(hash)
}

// ── Helpers for Phase 2 ───────────────────────────────────────────────────────

pub fn validate_file_for_organization(db: &Database, file_id: i64) -> Result<IndexedFile, String> {
    validate_positive_id(file_id, "File")?;
    let file = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;

    let root = db
        .get_scan_root(file.scan_root_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Scan root for file #{file_id} not found."))?;

    if !root.enabled {
        return Err(format!(
            "Action denied: Scan root '{}' containing file #{file_id} is disabled.",
            root.display_name
        ));
    }

    // Boundary check: normalized file path must begin with normalized root path
    let norm_file = file.normalized_path.replace('/', "\\").to_lowercase();
    let norm_root = root.absolute_path.replace('/', "\\").to_lowercase();
    let norm_root_trimmed = norm_root.trim_end_matches('\\');

    if !norm_file.starts_with(norm_root_trimmed) {
        return Err(
            "Security violation: Target file path is outside authorized scan root boundaries."
                .to_string(),
        );
    }

    Ok(file)
}

// ── Phase 2: Collections ──────────────────────────────────────────────────────

pub fn create_collection_inner(
    db: &Database,
    name: &str,
    description: Option<&str>,
    color: Option<&str>,
) -> Result<Collection, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Collection name cannot be empty.".to_string());
    }
    if db
        .get_collection_by_name(trimmed)
        .map_err(db_err)?
        .is_some()
    {
        return Err(format!(
            "A collection with the name '{trimmed}' already exists."
        ));
    }
    db.create_collection(trimmed, description, color)
        .map_err(db_err)
}

#[tauri::command]
pub fn create_collection(
    name: String,
    description: Option<String>,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<Collection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    create_collection_inner(&db, &name, description.as_deref(), color.as_deref())
}

pub fn get_collection_inner(db: &Database, id: i64) -> Result<Collection, String> {
    validate_positive_id(id, "Collection")?;
    db.get_collection(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Collection #{id} not found."))
}

#[tauri::command]
pub fn get_collection(id: i64, state: State<'_, AppState>) -> Result<Collection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_collection_inner(&db, id)
}

pub fn list_collections_inner(db: &Database) -> Result<Vec<Collection>, String> {
    db.list_collections().map_err(db_err)
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> Result<Vec<Collection>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_collections_inner(&db)
}

pub fn update_collection_inner(
    db: &Database,
    id: i64,
    name: &str,
    description: Option<&str>,
    color: Option<&str>,
) -> Result<Collection, String> {
    validate_positive_id(id, "Collection")?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Collection name cannot be empty.".to_string());
    }
    let existing = db
        .get_collection(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Collection #{id} not found."))?;

    if let Some(other) = db.get_collection_by_name(trimmed).map_err(db_err)? {
        if other.id != existing.id {
            return Err(format!(
                "A collection with the name '{trimmed}' already exists."
            ));
        }
    }

    db.update_collection(id, trimmed, description, color)
        .map_err(db_err)
}

#[tauri::command]
pub fn update_collection(
    id: i64,
    name: String,
    description: Option<String>,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<Collection, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    update_collection_inner(&db, id, &name, description.as_deref(), color.as_deref())
}

pub fn delete_collection_inner(db: &Database, id: i64) -> Result<(), String> {
    validate_positive_id(id, "Collection")?;
    let _ = db
        .get_collection(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Collection #{id} not found."))?;
    db.delete_collection(id).map_err(db_err)
}

#[tauri::command]
pub fn delete_collection(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    delete_collection_inner(&db, id)
}

// ── Phase 2: Tags ─────────────────────────────────────────────────────────────

pub fn create_tag_inner(db: &Database, name: &str, color: Option<&str>) -> Result<Tag, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tag name cannot be empty.".to_string());
    }
    if db.get_tag_by_name(trimmed).map_err(db_err)?.is_some() {
        return Err(format!("A tag with the name '{trimmed}' already exists."));
    }
    db.create_tag(trimmed, color).map_err(db_err)
}

#[tauri::command]
pub fn create_tag(
    name: String,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<Tag, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    create_tag_inner(&db, &name, color.as_deref())
}

pub fn get_tag_inner(db: &Database, id: i64) -> Result<Tag, String> {
    validate_positive_id(id, "Tag")?;
    db.get_tag(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Tag #{id} not found."))
}

#[tauri::command]
pub fn get_tag(id: i64, state: State<'_, AppState>) -> Result<Tag, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_tag_inner(&db, id)
}

pub fn list_tags_inner(db: &Database) -> Result<Vec<Tag>, String> {
    db.list_tags().map_err(db_err)
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_tags_inner(&db)
}

pub fn update_tag_inner(
    db: &Database,
    id: i64,
    name: &str,
    color: Option<&str>,
) -> Result<Tag, String> {
    validate_positive_id(id, "Tag")?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Tag name cannot be empty.".to_string());
    }
    let existing = db
        .get_tag(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Tag #{id} not found."))?;

    if let Some(other) = db.get_tag_by_name(trimmed).map_err(db_err)? {
        if other.id != existing.id {
            return Err(format!("A tag with the name '{trimmed}' already exists."));
        }
    }

    db.update_tag(id, trimmed, color).map_err(db_err)
}

#[tauri::command]
pub fn update_tag(
    id: i64,
    name: String,
    color: Option<String>,
    state: State<'_, AppState>,
) -> Result<Tag, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    update_tag_inner(&db, id, &name, color.as_deref())
}

pub fn delete_tag_inner(db: &Database, id: i64) -> Result<(), String> {
    validate_positive_id(id, "Tag")?;
    let _ = db
        .get_tag(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Tag #{id} not found."))?;
    db.delete_tag(id).map_err(db_err)
}

#[tauri::command]
pub fn delete_tag(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    delete_tag_inner(&db, id)
}

// ── Phase 2: File-Collection Links ────────────────────────────────────────────

pub fn add_file_to_collection_inner(
    db: &Database,
    file_id: i64,
    collection_id: i64,
) -> Result<FileCollectionLink, String> {
    validate_file_for_organization(db, file_id)?;
    validate_positive_id(collection_id, "Collection")?;
    let _ = db
        .get_collection(collection_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Collection #{collection_id} not found."))?;
    db.add_file_to_collection(file_id, collection_id)
        .map_err(db_err)
}

#[tauri::command]
pub fn add_file_to_collection(
    file_id: i64,
    collection_id: i64,
    state: State<'_, AppState>,
) -> Result<FileCollectionLink, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    add_file_to_collection_inner(&db, file_id, collection_id)
}

pub fn remove_file_from_collection_inner(
    db: &Database,
    file_id: i64,
    collection_id: i64,
) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    validate_positive_id(collection_id, "Collection")?;
    db.remove_file_from_collection(file_id, collection_id)
        .map_err(db_err)
}

#[tauri::command]
pub fn remove_file_from_collection(
    file_id: i64,
    collection_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    remove_file_from_collection_inner(&db, file_id, collection_id)
}

pub fn list_file_collections_inner(db: &Database, file_id: i64) -> Result<Vec<Collection>, String> {
    validate_positive_id(file_id, "File")?;
    let _ = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;
    db.list_file_collections(file_id).map_err(db_err)
}

#[tauri::command]
pub fn list_file_collections(
    file_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Collection>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_file_collections_inner(&db, file_id)
}

pub fn bulk_add_file_collections_inner(
    db: &Database,
    file_id: i64,
    collection_ids: &[i64],
) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    for &cid in collection_ids {
        validate_positive_id(cid, "Collection")?;
        if db.get_collection(cid).map_err(db_err)?.is_none() {
            return Err(format!("Collection #{cid} not found."));
        }
    }
    db.bulk_add_file_collections(file_id, collection_ids)
        .map_err(db_err)
}

#[tauri::command]
pub fn bulk_add_file_collections(
    file_id: i64,
    collection_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_add_file_collections_inner(&db, file_id, &collection_ids)
}

pub fn bulk_remove_file_collections_inner(
    db: &Database,
    file_id: i64,
    collection_ids: &[i64],
) -> Result<(), String> {
    validate_positive_id(file_id, "File")?;
    for &cid in collection_ids {
        validate_positive_id(cid, "Collection")?;
    }
    let _ = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;
    db.bulk_remove_file_collections(file_id, collection_ids)
        .map_err(db_err)
}

#[tauri::command]
pub fn bulk_remove_file_collections(
    file_id: i64,
    collection_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_remove_file_collections_inner(&db, file_id, &collection_ids)
}

pub fn set_file_collections_inner(
    db: &Database,
    file_id: i64,
    collection_ids: &[i64],
) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    for &cid in collection_ids {
        validate_positive_id(cid, "Collection")?;
        if db.get_collection(cid).map_err(db_err)?.is_none() {
            return Err(format!("Collection #{cid} not found."));
        }
    }
    db.set_file_collections(file_id, collection_ids)
        .map_err(db_err)
}

#[tauri::command]
pub fn set_file_collections(
    file_id: i64,
    collection_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    set_file_collections_inner(&db, file_id, &collection_ids)
}

// ── Phase 2: File-Tag Links ───────────────────────────────────────────────────

pub fn add_file_to_tag_inner(
    db: &Database,
    file_id: i64,
    tag_id: i64,
) -> Result<FileTagLink, String> {
    validate_file_for_organization(db, file_id)?;
    validate_positive_id(tag_id, "Tag")?;
    let _ = db
        .get_tag(tag_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Tag #{tag_id} not found."))?;
    db.add_file_to_tag(file_id, tag_id).map_err(db_err)
}

#[tauri::command]
pub fn add_file_to_tag(
    file_id: i64,
    tag_id: i64,
    state: State<'_, AppState>,
) -> Result<FileTagLink, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    add_file_to_tag_inner(&db, file_id, tag_id)
}

pub fn remove_file_from_tag_inner(db: &Database, file_id: i64, tag_id: i64) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    validate_positive_id(tag_id, "Tag")?;
    db.remove_file_from_tag(file_id, tag_id).map_err(db_err)
}

#[tauri::command]
pub fn remove_file_from_tag(
    file_id: i64,
    tag_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    remove_file_from_tag_inner(&db, file_id, tag_id)
}

pub fn list_file_tags_inner(db: &Database, file_id: i64) -> Result<Vec<Tag>, String> {
    validate_positive_id(file_id, "File")?;
    let _ = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;
    db.list_file_tags(file_id).map_err(db_err)
}

#[tauri::command]
pub fn list_file_tags(file_id: i64, state: State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_file_tags_inner(&db, file_id)
}

pub fn bulk_add_file_tags_inner(
    db: &Database,
    file_id: i64,
    tag_ids: &[i64],
) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    for &tid in tag_ids {
        validate_positive_id(tid, "Tag")?;
        if db.get_tag(tid).map_err(db_err)?.is_none() {
            return Err(format!("Tag #{tid} not found."));
        }
    }
    db.bulk_add_file_tags(file_id, tag_ids).map_err(db_err)
}

#[tauri::command]
pub fn bulk_add_file_tags(
    file_id: i64,
    tag_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_add_file_tags_inner(&db, file_id, &tag_ids)
}

pub fn bulk_remove_file_tags_inner(
    db: &Database,
    file_id: i64,
    tag_ids: &[i64],
) -> Result<(), String> {
    validate_positive_id(file_id, "File")?;
    for &tid in tag_ids {
        validate_positive_id(tid, "Tag")?;
    }
    let _ = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;
    db.bulk_remove_file_tags(file_id, tag_ids).map_err(db_err)
}

#[tauri::command]
pub fn bulk_remove_file_tags(
    file_id: i64,
    tag_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_remove_file_tags_inner(&db, file_id, &tag_ids)
}

pub fn set_file_tags_inner(db: &Database, file_id: i64, tag_ids: &[i64]) -> Result<(), String> {
    validate_file_for_organization(db, file_id)?;
    for &tid in tag_ids {
        validate_positive_id(tid, "Tag")?;
        if db.get_tag(tid).map_err(db_err)?.is_none() {
            return Err(format!("Tag #{tid} not found."));
        }
    }
    db.set_file_tags(file_id, tag_ids).map_err(db_err)
}

#[tauri::command]
pub fn set_file_tags(
    file_id: i64,
    tag_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    set_file_tags_inner(&db, file_id, &tag_ids)
}

// ── Phase 2: File Notes ───────────────────────────────────────────────────────
//
// Notes are strictly user-authored private text. They are stored in the local
// SQLite database only, keyed by indexed file ID, and never written to or
// derived from the original document. No extraction pipeline calls into these
// commands.

pub fn create_file_note_inner(
    db: &Database,
    file_id: i64,
    note_text: &str,
) -> Result<FileNote, String> {
    validate_positive_id(file_id, "File")?;
    let trimmed = note_text.trim();
    if trimmed.is_empty() {
        return Err("Note content cannot be empty.".to_string());
    }
    validate_file_for_organization(db, file_id)?;
    db.create_file_note(file_id, trimmed).map_err(db_err)
}

#[tauri::command]
pub fn create_file_note(
    file_id: i64,
    note_text: String,
    state: State<'_, AppState>,
) -> Result<FileNote, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    create_file_note_inner(&db, file_id, &note_text)
}

pub fn get_file_note_inner(db: &Database, note_id: i64) -> Result<FileNote, String> {
    validate_positive_id(note_id, "Note")?;
    db.get_file_note(note_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Note #{note_id} not found."))
}

/// Applies the same disabled-root policy as collection/tag mutations by
/// resolving the note's indexed file first.
fn validate_note_for_organization(db: &Database, note_id: i64) -> Result<FileNote, String> {
    validate_positive_id(note_id, "Note")?;
    let note = get_file_note_inner(db, note_id)?;
    validate_file_for_organization(db, note.file_id)?;
    Ok(note)
}

#[tauri::command]
pub fn get_file_note(note_id: i64, state: State<'_, AppState>) -> Result<FileNote, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_file_note_inner(&db, note_id)
}

pub fn list_file_notes_inner(db: &Database, file_id: i64) -> Result<Vec<FileNote>, String> {
    validate_positive_id(file_id, "File")?;
    let _ = db
        .get_indexed_file(file_id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Indexed file #{file_id} not found in library."))?;
    db.list_file_notes(file_id).map_err(db_err)
}

#[tauri::command]
pub fn list_file_notes(file_id: i64, state: State<'_, AppState>) -> Result<Vec<FileNote>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_file_notes_inner(&db, file_id)
}

pub fn update_file_note_inner(
    db: &Database,
    note_id: i64,
    note_text: &str,
) -> Result<FileNote, String> {
    validate_positive_id(note_id, "Note")?;
    let trimmed = note_text.trim();
    if trimmed.is_empty() {
        return Err("Note content cannot be empty.".to_string());
    }
    let _ = validate_note_for_organization(db, note_id)?;
    db.update_file_note(note_id, trimmed).map_err(db_err)
}

#[tauri::command]
pub fn update_file_note(
    note_id: i64,
    note_text: String,
    state: State<'_, AppState>,
) -> Result<FileNote, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    update_file_note_inner(&db, note_id, &note_text)
}

pub fn delete_file_note_inner(db: &Database, note_id: i64) -> Result<(), String> {
    validate_positive_id(note_id, "Note")?;
    let _ = validate_note_for_organization(db, note_id)?;
    db.delete_file_note(note_id).map_err(db_err)
}

#[tauri::command]
pub fn delete_file_note(note_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    delete_file_note_inner(&db, note_id)
}

// ── Phase 3: Suggestions & Duplicates ────────────────────────────────────────

pub fn list_organization_suggestions_inner(
    db: &Database,
    status: Option<&str>,
    suggestion_type: Option<&str>,
) -> Result<Vec<OrganizationSuggestion>, String> {
    db.list_organization_suggestions(status, suggestion_type)
        .map_err(db_err)
}

#[tauri::command]
pub fn list_pending_suggestions(
    state: State<'_, AppState>,
) -> Result<Vec<OrganizationSuggestion>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_organization_suggestions_inner(&db, Some("pending"), None)
}

#[tauri::command]
pub fn list_organization_suggestions(
    status: Option<String>,
    suggestion_type: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<OrganizationSuggestion>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_organization_suggestions_inner(
        &db,
        status.as_deref().filter(|s| !s.trim().is_empty()),
        suggestion_type.as_deref().filter(|s| !s.trim().is_empty()),
    )
}

pub fn get_organization_suggestion_inner(
    db: &Database,
    id: i64,
) -> Result<OrganizationSuggestion, String> {
    validate_positive_id(id, "Suggestion")?;
    db.get_organization_suggestion(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Suggestion #{id} not found."))
}

#[tauri::command]
pub fn get_organization_suggestion(
    id: i64,
    state: State<'_, AppState>,
) -> Result<OrganizationSuggestion, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_organization_suggestion_inner(&db, id)
}

pub fn approve_suggestion_inner(db: &Database, id: i64) -> Result<OrganizationSuggestion, String> {
    validate_positive_id(id, "Suggestion")?;
    let suggestion = db
        .get_organization_suggestion(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Suggestion #{id} not found."))?;

    if suggestion.status == "approved" {
        return Ok(suggestion);
    }

    // Validate that each affected file exists and belongs to an enabled scan root
    for &fid in &suggestion.affected_file_ids {
        validate_file_for_organization(db, fid)?;
    }

    db.approve_suggestion(id).map_err(db_err)
}

#[tauri::command]
pub fn approve_suggestion(
    id: i64,
    state: State<'_, AppState>,
) -> Result<OrganizationSuggestion, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    approve_suggestion_inner(&db, id)
}

pub fn reject_suggestion_inner(db: &Database, id: i64) -> Result<OrganizationSuggestion, String> {
    validate_positive_id(id, "Suggestion")?;
    let suggestion = db
        .get_organization_suggestion(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Suggestion #{id} not found."))?;

    if suggestion.status == "rejected" {
        return Ok(suggestion);
    }

    db.reject_suggestion(id).map_err(db_err)
}

#[tauri::command]
pub fn reject_suggestion(
    id: i64,
    state: State<'_, AppState>,
) -> Result<OrganizationSuggestion, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    reject_suggestion_inner(&db, id)
}

pub fn dismiss_suggestion_inner(db: &Database, id: i64) -> Result<OrganizationSuggestion, String> {
    validate_positive_id(id, "Suggestion")?;
    let suggestion = db
        .get_organization_suggestion(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Suggestion #{id} not found."))?;

    if suggestion.status == "dismissed" {
        return Ok(suggestion);
    }

    db.dismiss_suggestion(id).map_err(db_err)
}

#[tauri::command]
pub fn dismiss_suggestion(
    id: i64,
    state: State<'_, AppState>,
) -> Result<OrganizationSuggestion, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    dismiss_suggestion_inner(&db, id)
}

pub fn list_duplicate_groups_inner(
    db: &Database,
    status: Option<&str>,
) -> Result<Vec<DuplicateGroup>, String> {
    db.list_duplicate_groups(status).map_err(db_err)
}

#[tauri::command]
pub fn list_duplicate_groups(
    status: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DuplicateGroup>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    list_duplicate_groups_inner(&db, status.as_deref().filter(|s| !s.trim().is_empty()))
}

pub fn get_duplicate_group_inner(db: &Database, id: i64) -> Result<DuplicateGroup, String> {
    validate_positive_id(id, "Duplicate group")?;
    db.get_duplicate_group(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))
}

#[tauri::command]
pub fn get_duplicate_group(id: i64, state: State<'_, AppState>) -> Result<DuplicateGroup, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_duplicate_group_inner(&db, id)
}

pub fn get_duplicate_group_detail_inner(
    db: &Database,
    id: i64,
) -> Result<DuplicateGroupDetail, String> {
    validate_positive_id(id, "Duplicate group")?;
    db.get_duplicate_group_detail(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))
}

#[tauri::command]
pub fn get_duplicate_group_detail(
    id: i64,
    state: State<'_, AppState>,
) -> Result<DuplicateGroupDetail, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_duplicate_group_detail_inner(&db, id)
}

pub fn get_file_comparison_inner(
    db: &Database,
    file_ids: Vec<i64>,
) -> Result<Vec<FileComparisonItem>, String> {
    for &fid in &file_ids {
        validate_positive_id(fid, "File")?;
    }
    db.get_files_comparison(&file_ids).map_err(db_err)
}

#[tauri::command]
pub fn get_file_comparison(
    file_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<FileComparisonItem>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    get_file_comparison_inner(&db, file_ids)
}

pub fn approve_duplicate_group_inner(
    db: &Database,
    id: i64,
    preferred_file_id: Option<i64>,
) -> Result<DuplicateGroup, String> {
    validate_positive_id(id, "Duplicate group")?;
    let group = db
        .get_duplicate_group(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))?;

    if group.status == "approved" && preferred_file_id.is_none() {
        return Ok(group);
    }

    for &fid in &group.file_ids {
        validate_file_for_organization(db, fid)?;
    }

    if let Some(pref_id) = preferred_file_id {
        validate_positive_id(pref_id, "Preferred file")?;
        if !group.file_ids.contains(&pref_id) {
            return Err(format!(
                "Preferred file #{pref_id} is not part of duplicate group #{id}."
            ));
        }
        let pref_file = validate_file_for_organization(db, pref_id)?;
        if pref_file.status == "missing" {
            return Err(format!(
                "Action denied: Cannot mark file #{pref_id} as preferred because it is missing."
            ));
        }
        if pref_file.status == "inaccessible" {
            return Err(format!(
                "Action denied: Cannot mark file #{pref_id} as preferred because it is inaccessible."
            ));
        }
        if !std::path::Path::new(&pref_file.absolute_path).exists() {
            return Err(format!(
                "Action denied: Cannot mark file #{pref_id} as preferred because it does not exist on disk."
            ));
        }
    } else {
        // Ensure at least one file in the group exists on disk and is not missing/inaccessible
        let mut available = false;
        for &fid in &group.file_ids {
            if let Ok(Some(f)) = db.get_indexed_file(fid) {
                if f.status != "missing"
                    && f.status != "inaccessible"
                    && std::path::Path::new(&f.absolute_path).exists()
                {
                    available = true;
                    break;
                }
            }
        }
        if !available {
            return Err(format!(
                "Action denied: Cannot approve duplicate group #{id} because all files in the group are missing, inaccessible, or deleted from disk."
            ));
        }
    }

    db.approve_duplicate_group(id, preferred_file_id)
        .map_err(db_err)
}

pub fn mark_duplicate_preferred_inner(
    db: &Database,
    id: i64,
    preferred_file_id: i64,
) -> Result<DuplicateGroupDetail, String> {
    validate_positive_id(id, "Duplicate group")?;
    validate_positive_id(preferred_file_id, "Preferred file")?;
    let group = db
        .get_duplicate_group(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))?;

    if !group.file_ids.contains(&preferred_file_id) {
        return Err(format!(
            "Preferred file #{preferred_file_id} is not part of duplicate group #{id}."
        ));
    }

    for &fid in &group.file_ids {
        validate_file_for_organization(db, fid)?;
    }

    let pref_file = validate_file_for_organization(db, preferred_file_id)?;
    if pref_file.status == "missing" {
        return Err(format!(
            "Action denied: Cannot mark file #{preferred_file_id} as preferred because it is missing."
        ));
    }
    if pref_file.status == "inaccessible" {
        return Err(format!(
            "Action denied: Cannot mark file #{preferred_file_id} as preferred because it is inaccessible."
        ));
    }
    if !std::path::Path::new(&pref_file.absolute_path).exists() {
        return Err(format!(
            "Action denied: Cannot mark file #{preferred_file_id} as preferred because it does not exist on disk."
        ));
    }

    db.mark_duplicate_preferred(id, preferred_file_id)
        .map_err(db_err)
}

#[tauri::command]
pub fn mark_duplicate_preferred(
    id: i64,
    preferred_file_id: i64,
    state: State<'_, AppState>,
) -> Result<DuplicateGroupDetail, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    mark_duplicate_preferred_inner(&db, id, preferred_file_id)
}

#[tauri::command]
pub fn approve_duplicate_group(
    id: i64,
    preferred_file_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<DuplicateGroup, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    approve_duplicate_group_inner(&db, id, preferred_file_id)
}

pub fn reject_duplicate_group_inner(db: &Database, id: i64) -> Result<DuplicateGroup, String> {
    validate_positive_id(id, "Duplicate group")?;
    let group = db
        .get_duplicate_group(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))?;

    if group.status == "rejected" {
        return Ok(group);
    }

    db.reject_duplicate_group(id).map_err(db_err)
}

#[tauri::command]
pub fn reject_duplicate_group(
    id: i64,
    state: State<'_, AppState>,
) -> Result<DuplicateGroup, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    reject_duplicate_group_inner(&db, id)
}

pub fn dismiss_duplicate_group_inner(db: &Database, id: i64) -> Result<DuplicateGroup, String> {
    validate_positive_id(id, "Duplicate group")?;
    let group = db
        .get_duplicate_group(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("Duplicate group #{id} not found."))?;

    if group.status == "dismissed" {
        return Ok(group);
    }

    db.dismiss_duplicate_group(id).map_err(db_err)
}

#[tauri::command]
pub fn dismiss_duplicate_group(
    id: i64,
    state: State<'_, AppState>,
) -> Result<DuplicateGroup, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    dismiss_duplicate_group_inner(&db, id)
}

pub fn bulk_approve_suggestions_inner(
    db: &Database,
    suggestion_ids: &[i64],
) -> Result<BulkSuggestionResult, String> {
    let mut approved_ids = Vec::new();
    let mut failed_ids = Vec::new();
    let mut errors = Vec::new();

    for &id in suggestion_ids {
        match approve_suggestion_inner(db, id) {
            Ok(_) => approved_ids.push(id),
            Err(e) => {
                failed_ids.push(id);
                errors.push(format!("Suggestion #{id}: {e}"));
            }
        }
    }

    Ok(BulkSuggestionResult {
        approved_ids,
        rejected_ids: Vec::new(),
        failed_ids,
        errors,
    })
}

#[tauri::command]
pub fn bulk_approve_suggestions(
    suggestion_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<BulkSuggestionResult, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_approve_suggestions_inner(&db, &suggestion_ids)
}

pub fn bulk_reject_suggestions_inner(
    db: &Database,
    suggestion_ids: &[i64],
) -> Result<BulkSuggestionResult, String> {
    let mut rejected_ids = Vec::new();
    let mut failed_ids = Vec::new();
    let mut errors = Vec::new();

    for &id in suggestion_ids {
        match reject_suggestion_inner(db, id) {
            Ok(_) => rejected_ids.push(id),
            Err(e) => {
                failed_ids.push(id);
                errors.push(format!("Suggestion #{id}: {e}"));
            }
        }
    }

    Ok(BulkSuggestionResult {
        approved_ids: Vec::new(),
        rejected_ids,
        failed_ids,
        errors,
    })
}

#[tauri::command]
pub fn bulk_reject_suggestions(
    suggestion_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<BulkSuggestionResult, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    bulk_reject_suggestions_inner(&db, &suggestion_ids)
}

pub fn generate_suggestions_inner(db: &Database) -> Result<(usize, usize), String> {
    db.generate_and_store_suggestions(None).map_err(db_err)
}

#[tauri::command]
pub fn generate_suggestions(state: State<'_, AppState>) -> Result<(usize, usize), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    generate_suggestions_inner(&db)
}

pub fn get_local_ai_status_inner() -> Result<LocalAiStatus, String> {
    let provider = DisabledLocalProvider::new();
    Ok(provider.status())
}

#[tauri::command]
pub fn get_local_ai_status() -> Result<LocalAiStatus, String> {
    get_local_ai_status_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EXPORT_FORMAT_VERSION;
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
            collections: vec![],
            tags: vec![],
            file_collections: vec![],
            file_tags: vec![],
            file_notes: vec![],
            organization_suggestions: vec![],
            suggestion_files: vec![],
            duplicate_groups: vec![],
            duplicate_group_files: vec![],
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
            collections: vec![],
            tags: vec![],
            file_collections: vec![],
            file_tags: vec![],
            file_notes: vec![],
            organization_suggestions: vec![],
            suggestion_files: vec![],
            duplicate_groups: vec![],
            duplicate_group_files: vec![],
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
            collections: vec![],
            tags: vec![],
            file_collections: vec![],
            file_tags: vec![],
            file_notes: vec![],
            organization_suggestions: vec![],
            suggestion_files: vec![],
            duplicate_groups: vec![],
            duplicate_group_files: vec![],
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
        assert!(parsed.get("collections").is_some(), "missing collections");
        assert!(parsed.get("tags").is_some(), "missing tags");
        assert!(
            parsed.get("file_collections").is_some(),
            "missing file_collections"
        );
        assert!(parsed.get("file_tags").is_some(), "missing file_tags");
        assert!(parsed.get("file_notes").is_some(), "missing file_notes");
        assert!(
            parsed.get("organization_suggestions").is_some(),
            "missing organization_suggestions"
        );
        assert!(
            parsed.get("suggestion_files").is_some(),
            "missing suggestion_files"
        );
        assert!(
            parsed.get("duplicate_groups").is_some(),
            "missing duplicate_groups"
        );
        assert!(
            parsed.get("duplicate_group_files").is_some(),
            "missing duplicate_group_files"
        );

        // Verify summary sub-fields
        let summary = parsed.get("summary").unwrap();
        assert!(summary.get("total_roots").is_some());
        assert!(summary.get("total_files").is_some());
        assert!(summary.get("total_size_bytes").is_some());
        assert!(summary.get("export_timestamp").is_some());

        // format_version should match the current export format
        assert_eq!(
            parsed["format_version"].as_str().unwrap(),
            EXPORT_FORMAT_VERSION
        );

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
        assert_eq!(deserialized.format_version, EXPORT_FORMAT_VERSION);
        assert_eq!(deserialized.app, "LibOne");
        assert_eq!(deserialized.summary.total_roots, 1);
        assert_eq!(deserialized.summary.total_files, 1);
        assert_eq!(deserialized.files[0].file_name, "sample.pdf");
        assert_eq!(deserialized.scan_roots[0].display_name, "RT Root");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_commands_invalid_ids() {
        let (db, db_dir) = test_auth_db("cmd_invalid_ids");

        // Invalid file ID
        let res_fc = add_file_to_collection_inner(&db, 99999, 1);
        assert!(res_fc.is_err());
        assert!(res_fc.unwrap_err().contains("not found in library"));

        let res_ft = add_file_to_tag_inner(&db, 99999, 1);
        assert!(res_ft.is_err());
        assert!(res_ft.unwrap_err().contains("not found in library"));

        let res_note = create_file_note_inner(&db, 99999, "Note text");
        assert!(res_note.is_err());
        assert!(res_note.unwrap_err().contains("not found in library"));

        // Invalid collection ID
        let res_col = get_collection_inner(&db, 99999);
        assert!(res_col.is_err());
        assert!(res_col.unwrap_err().contains("Collection #99999 not found"));

        // Invalid tag ID
        let res_tag = get_tag_inner(&db, 99999);
        assert!(res_tag.is_err());
        assert!(res_tag.unwrap_err().contains("Tag #99999 not found"));

        // Invalid note ID
        let res_n = get_file_note_inner(&db, 99999);
        assert!(res_n.is_err());
        assert!(res_n.unwrap_err().contains("Note #99999 not found"));

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_commands_disabled_roots() {
        let dir = temp_auth_dir("cmd_disabled_root");
        let (db, db_dir) = test_auth_db("cmd_disabled_root");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Test Root")
            .unwrap();
        let file = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("doc.pdf").to_str().unwrap().into(),
                normalized_path: dir
                    .join("doc.pdf")
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "doc.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let col = create_collection_inner(&db, "Test Col", None, None).unwrap();
        let tag = create_tag_inner(&db, "Test Tag", None).unwrap();

        // Seed metadata while the root is enabled.
        let _ = add_file_to_collection_inner(&db, file.id, col.id).unwrap();
        let _ = add_file_to_tag_inner(&db, file.id, tag.id).unwrap();
        let note = create_file_note_inner(&db, file.id, "Note before disable").unwrap();

        // Disable root
        db.remove_scan_root(root.id).unwrap();

        // Organization commands should reject files in disabled roots
        let res_col = add_file_to_collection_inner(&db, file.id, col.id);
        assert!(res_col.is_err());
        assert!(res_col.unwrap_err().contains("is disabled"));

        let res_tag = add_file_to_tag_inner(&db, file.id, tag.id);
        assert!(res_tag.is_err());
        assert!(res_tag.unwrap_err().contains("is disabled"));

        let res_note = create_file_note_inner(&db, file.id, "Note on disabled");
        assert!(res_note.is_err());
        assert!(res_note.unwrap_err().contains("is disabled"));

        // Removals and edits are blocked too, so all organization mutations
        // behave the same for a disabled root.
        let res_unlink = remove_file_from_collection_inner(&db, file.id, col.id);
        assert!(res_unlink.is_err());
        assert!(res_unlink.unwrap_err().contains("is disabled"));

        let res_untag = remove_file_from_tag_inner(&db, file.id, tag.id);
        assert!(res_untag.is_err());
        assert!(res_untag.unwrap_err().contains("is disabled"));

        let res_edit = update_file_note_inner(&db, note.id, "Edited while disabled");
        assert!(res_edit.is_err());
        assert!(res_edit.unwrap_err().contains("is disabled"));

        let res_delete = delete_file_note_inner(&db, note.id);
        assert!(res_delete.is_err());
        assert!(res_delete.unwrap_err().contains("is disabled"));

        // Existing metadata remains readable and intact.
        assert_eq!(list_file_notes_inner(&db, file.id).unwrap().len(), 1);
        assert_eq!(list_file_collections_inner(&db, file.id).unwrap().len(), 1);
        assert_eq!(list_file_tags_inner(&db, file.id).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_commands_duplicate_links() {
        let dir = temp_auth_dir("cmd_dup_links");
        let (db, db_dir) = test_auth_db("cmd_dup_links");

        let root = db.add_scan_root(dir.to_str().unwrap(), "Dup Root").unwrap();
        let file = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("f.pdf").to_str().unwrap().into(),
                normalized_path: dir
                    .join("f.pdf")
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "f.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let col = create_collection_inner(&db, "Col Dup", None, None).unwrap();
        let tag = create_tag_inner(&db, "Tag Dup", None).unwrap();

        // First link
        let l1 = add_file_to_collection_inner(&db, file.id, col.id).unwrap();
        assert_eq!(l1.collection_id, col.id);
        // Duplicate link succeeds safely
        let l1_dup = add_file_to_collection_inner(&db, file.id, col.id).unwrap();
        assert_eq!(l1_dup.collection_id, col.id);

        let t1 = add_file_to_tag_inner(&db, file.id, tag.id).unwrap();
        assert_eq!(t1.tag_id, tag.id);
        let t1_dup = add_file_to_tag_inner(&db, file.id, tag.id).unwrap();
        assert_eq!(t1_dup.tag_id, tag.id);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_commands_missing_records_and_deletion() {
        let (db, db_dir) = test_auth_db("cmd_missing_del");

        // Deleting non-existent entities
        assert!(delete_collection_inner(&db, 99999).is_err());
        assert!(delete_tag_inner(&db, 99999).is_err());
        assert!(delete_file_note_inner(&db, 99999).is_err());

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_commands_valid_operations_full_flow() {
        let dir = temp_auth_dir("cmd_full_flow");
        let (db, db_dir) = test_auth_db("cmd_full_flow");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Full Root")
            .unwrap();
        let f1 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("invoice.pdf").to_str().unwrap().into(),
                normalized_path: dir
                    .join("invoice.pdf")
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "invoice.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 256,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        // Collections CRUD
        let c1 =
            create_collection_inner(&db, "Invoices", Some("Incoming invoices"), Some("#3b82f6"))
                .unwrap();
        assert_eq!(c1.name, "Invoices");
        let c_list = list_collections_inner(&db).unwrap();
        assert_eq!(c_list.len(), 1);

        let c1_up =
            update_collection_inner(&db, c1.id, "Paid Invoices", None, Some("#10b981")).unwrap();
        assert_eq!(c1_up.name, "Paid Invoices");

        // Tags CRUD
        let t1 = create_tag_inner(&db, "2026", Some("#f59e0b")).unwrap();
        let t2 = create_tag_inner(&db, "Q4", None).unwrap();
        let t_list = list_tags_inner(&db).unwrap();
        assert_eq!(t_list.len(), 2);

        // Link file to collection & tags
        add_file_to_collection_inner(&db, f1.id, c1.id).unwrap();
        bulk_add_file_tags_inner(&db, f1.id, &[t1.id, t2.id]).unwrap();

        // Listing collections and tags for file
        let file_cols = list_file_collections_inner(&db, f1.id).unwrap();
        assert_eq!(file_cols.len(), 1);
        assert_eq!(file_cols[0].id, c1.id);

        let file_tags = list_file_tags_inner(&db, f1.id).unwrap();
        assert_eq!(file_tags.len(), 2);

        // Notes CRUD
        let note = create_file_note_inner(&db, f1.id, "Paid on time").unwrap();
        assert_eq!(note.note_text, "Paid on time");

        let note_up = update_file_note_inner(&db, note.id, "Paid via ACH on Nov 14").unwrap();
        assert_eq!(note_up.note_text, "Paid via ACH on Nov 14");

        let notes_for_file = list_file_notes_inner(&db, f1.id).unwrap();
        assert_eq!(notes_for_file.len(), 1);

        // Combined file query
        let query_res = db
            .query_files(&FileQueryParams {
                collection_id: Some(c1.id),
                tag_ids: Some(vec![t1.id, t2.id]),
                has_notes: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(query_res.total, 1);
        assert_eq!(query_res.files[0].id, f1.id);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_file_notes_crud_validation_missing_files_and_persistence() {
        let dir = temp_auth_dir("cmd_notes_lifecycle");
        let (db, db_dir) = test_auth_db("cmd_notes_lifecycle");
        let file_path = dir.join("notes_target.pdf");
        fs::write(&file_path, b"%PDF-1.4 note target").unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Notes Root")
            .unwrap();
        let file = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file_path.to_str().unwrap().to_string(),
                normalized_path: file_path
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "notes_target.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 19,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        // Create: text is trimmed, timestamps are populated, multiple notes allowed.
        let first = create_file_note_inner(&db, file.id, "  First private note  ").unwrap();
        assert_eq!(first.note_text, "First private note");
        assert_eq!(first.file_id, file.id);
        assert_eq!(first.created_at, first.updated_at);

        let second = create_file_note_inner(&db, file.id, "Second private note").unwrap();
        let listed = list_file_notes_inner(&db, file.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id, "newest note should be first");

        // Edit.
        let edited = update_file_note_inner(&db, first.id, " Edited private note ").unwrap();
        assert_eq!(edited.note_text, "Edited private note");
        assert!(edited.updated_at >= edited.created_at);
        assert_eq!(
            get_file_note_inner(&db, first.id).unwrap().note_text,
            "Edited private note"
        );

        // Delete.
        delete_file_note_inner(&db, second.id).unwrap();
        assert_eq!(list_file_notes_inner(&db, file.id).unwrap().len(), 1);

        // Empty content is rejected on create and update.
        assert!(create_file_note_inner(&db, file.id, " \n\t ")
            .unwrap_err()
            .contains("cannot be empty"));
        assert!(update_file_note_inner(&db, first.id, "   ")
            .unwrap_err()
            .contains("cannot be empty"));

        // Invalid file IDs are rejected by the backend.
        assert!(create_file_note_inner(&db, 999_999, "x")
            .unwrap_err()
            .contains("not found in library"));
        assert!(list_file_notes_inner(&db, 999_999)
            .unwrap_err()
            .contains("not found in library"));

        // Invalid note IDs are rejected by the backend.
        assert!(get_file_note_inner(&db, 999_999)
            .unwrap_err()
            .contains("Note #999999 not found"));
        assert!(update_file_note_inner(&db, 999_999, "x")
            .unwrap_err()
            .contains("Note #999999 not found"));
        assert!(delete_file_note_inner(&db, 999_999)
            .unwrap_err()
            .contains("Note #999999 not found"));

        // Missing original file: notes stay readable and editable, and the file is never recreated.
        fs::remove_file(&file_path).unwrap();
        assert!(!file_path.exists());
        let missing_notes = list_file_notes_inner(&db, file.id).unwrap();
        assert_eq!(missing_notes.len(), 1);
        let edited_missing =
            update_file_note_inner(&db, first.id, "Note about a missing file").unwrap();
        assert_eq!(edited_missing.note_text, "Note about a missing file");
        assert!(
            !file_path.exists(),
            "note operations must never recreate or modify the original file"
        );
        assert_eq!(
            db.get_indexed_file(file.id).unwrap().unwrap().status,
            "indexed",
            "note operations must not change indexed file metadata"
        );

        // Persistence after a full database reload.
        drop(db);
        let reopened = Database::new(&db_dir).unwrap();
        let persisted = list_file_notes_inner(&reopened, file.id).unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].note_text, "Note about a missing file");
        delete_file_note_inner(&reopened, first.id).unwrap();
        assert!(list_file_notes_inner(&reopened, file.id)
            .unwrap()
            .is_empty());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_export_validation_covers_phase2_integrity_and_summary() {
        let dir = temp_auth_dir("cmd_export_validation");
        let (db, db_dir) = test_auth_db("cmd_export_validation");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Validation Root")
            .unwrap();
        let file = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("doc.pdf").to_str().unwrap().into(),
                normalized_path: dir
                    .join("doc.pdf")
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "doc.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 11,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();
        let collection = db.create_collection("Validation Col", None, None).unwrap();
        let tag = db.create_tag("validation-tag", None).unwrap();
        db.add_file_to_collection(file.id, collection.id).unwrap();
        db.add_file_to_tag(file.id, tag.id).unwrap();
        let note = db.create_file_note(file.id, "Own note").unwrap();

        let valid = db.export_library().unwrap();
        assert!(validate_export(&valid).is_ok());

        let mut bad = valid.clone();
        bad.summary.total_size_bytes += 1;
        assert!(validate_export(&bad)
            .unwrap_err()
            .contains("total_size_bytes"));

        let mut bad = valid.clone();
        bad.summary.export_timestamp = "1999-01-01T00:00:00Z".into();
        assert!(validate_export(&bad)
            .unwrap_err()
            .contains("export_timestamp"));

        let mut bad = valid.clone();
        bad.file_tags.push(FileTagLink {
            file_id: 999_999,
            tag_id: tag.id,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        assert!(validate_export(&bad).unwrap_err().contains("file_tags"));

        let mut bad = valid.clone();
        bad.file_collections.push(FileCollectionLink {
            file_id: file.id,
            collection_id: 999_999,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        assert!(validate_export(&bad)
            .unwrap_err()
            .contains("file_collections"));

        let mut bad = valid.clone();
        bad.file_notes.push(FileNote {
            id: note.id,
            file_id: file.id,
            note_text: "duplicate id".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        });
        assert!(validate_export(&bad)
            .unwrap_err()
            .contains("unique positive IDs"));

        let mut bad = valid.clone();
        bad.file_notes[0].note_text = "   ".into();
        assert!(validate_export(&bad).unwrap_err().contains("empty note"));

        let mut bad = valid;
        bad.tags.push(Tag {
            id: tag.id,
            name: "duplicated tag".into(),
            color_nullable: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        assert!(validate_export(&bad)
            .unwrap_err()
            .contains("unique positive IDs"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_export_writes_metadata_only_json_atomically() {
        let dir = temp_auth_dir("cmd_export_atomic");
        let (db, db_dir) = test_auth_db("cmd_export_atomic");
        let file_path = dir.join("private_document.txt");
        let document_body = "PRIVATE_DOCUMENT_BODY_MUST_NOT_BE_EXPORTED_7c21";
        fs::write(&file_path, document_body).unwrap();

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Atomic Root")
            .unwrap();
        let file = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file_path.to_str().unwrap().into(),
                normalized_path: file_path
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "private_document.txt".into(),
                extension: "txt".into(),
                size_bytes: document_body.len() as i64,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();
        let collection = db.create_collection("Atomic Col", None, None).unwrap();
        let tag = db.create_tag("atomic-tag", None).unwrap();
        db.add_file_to_collection(file.id, collection.id).unwrap();
        db.add_file_to_tag(file.id, tag.id).unwrap();
        db.create_file_note(file.id, "Private user note").unwrap();

        let export = db.export_library().unwrap();
        validate_export(&export).unwrap();
        let json = serde_json::to_string_pretty(&export).unwrap();

        assert!(
            !json.contains(document_body),
            "document contents leaked into export"
        );
        assert!(json.contains("Private user note"));
        for key in [
            "format_version",
            "scan_roots",
            "files",
            "collections",
            "tags",
            "file_collections",
            "file_tags",
            "file_notes",
        ] {
            assert!(
                json.contains(&format!("\"{key}\"")),
                "missing export key {key}"
            );
        }

        let target = dir.join("libone-export.json");
        atomic_write_json(target.to_str().unwrap(), &json).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), json);

        // Rewriting replaces the whole file and leaves no temporary siblings.
        let updated = format!("{json}\n");
        atomic_write_json(target.to_str().unwrap(), &updated).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), updated);
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary export files must be renamed"
        );

        // The original document was never touched by the export.
        assert_eq!(fs::read_to_string(&file_path).unwrap(), document_body);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_suggestion_lifecycle_transitions() {
        let (db, db_dir) = test_auth_db("sugg_lifecycle");

        let s1 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag proposal",
                "Based on year token",
                0.8,
                &[],
                None,
                Some("2024"),
                None,
                None,
            )
            .unwrap();
        assert_eq!(s1.status, "pending");
        assert!(s1.reviewed_at.is_none());

        // 1. Pending -> Approved
        let approved = approve_suggestion_inner(&db, s1.id).unwrap();
        assert_eq!(approved.status, "approved");
        assert!(approved.reviewed_at.is_some());

        // 2. Pending -> Rejected
        let s2 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag proposal 2",
                "Based on keyword",
                0.8,
                &[],
                None,
                Some("taxes"),
                None,
                None,
            )
            .unwrap();
        let rejected = reject_suggestion_inner(&db, s2.id).unwrap();
        assert_eq!(rejected.status, "rejected");
        assert!(rejected.reviewed_at.is_some());

        // 3. Pending -> Dismissed
        let s3 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag proposal 3",
                "Based on keyword",
                0.8,
                &[],
                None,
                Some("archive"),
                None,
                None,
            )
            .unwrap();
        let dismissed = dismiss_suggestion_inner(&db, s3.id).unwrap();
        assert_eq!(dismissed.status, "dismissed");
        assert!(dismissed.reviewed_at.is_some());

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_suggestion_approval_creates_and_reuses_phase2_metadata() {
        let dir = temp_auth_dir("sugg_metadata");
        let (db, db_dir) = test_auth_db("sugg_metadata");

        let root = db.add_scan_root(dir.to_str().unwrap(), "Root").unwrap();

        let f1 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("f1.pdf").to_str().unwrap().into(),
                normalized_path: dir.join("f1.pdf").to_str().unwrap().into(),
                file_name: "f1.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let f2 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("f2.pdf").to_str().unwrap().into(),
                normalized_path: dir.join("f2.pdf").to_str().unwrap().into(),
                file_name: "f2.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 200,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // 1. Suggest collection (new collection name "Tax Audits")
        let s_col = db
            .insert_organization_suggestion(
                "suggest_collection",
                "Group files into Tax Audits",
                "Matching directory cluster",
                0.85,
                &[f1.id, f2.id],
                None,
                None,
                None,
                Some("Tax Audits"),
            )
            .unwrap();

        let app_col = approve_suggestion_inner(&db, s_col.id).unwrap();
        assert_eq!(app_col.status, "approved");

        let cols = db.list_collections().unwrap();
        let created_col = cols.iter().find(|c| c.name == "Tax Audits").unwrap();

        let links1 = db.list_file_collections(f1.id).unwrap();
        let links2 = db.list_file_collections(f2.id).unwrap();
        assert!(links1.iter().any(|c| c.id == created_col.id));
        assert!(links2.iter().any(|c| c.id == created_col.id));

        // 2. Suggest collection reusing existing collection
        let s_col_reuse = db
            .insert_organization_suggestion(
                "suggest_collection",
                "Group file 2 into Tax Audits again",
                "Testing reuse",
                0.9,
                &[f2.id],
                None,
                None,
                Some(created_col.id),
                Some("Tax Audits"),
            )
            .unwrap();
        let app_reuse = approve_suggestion_inner(&db, s_col_reuse.id).unwrap();
        assert_eq!(app_reuse.status, "approved");

        // Verify no duplicate collection was created
        let cols_after = db.list_collections().unwrap();
        assert_eq!(cols_after.len(), 1);

        // 3. Suggest tag (new tag "invoice-2024")
        let s_tag = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag with invoice-2024",
                "Found in filename",
                0.8,
                &[f1.id],
                None,
                Some("invoice-2024"),
                None,
                None,
            )
            .unwrap();

        let app_tag = approve_suggestion_inner(&db, s_tag.id).unwrap();
        assert_eq!(app_tag.status, "approved");

        let tags = db.list_tags().unwrap();
        let created_tag = tags.iter().find(|t| t.name == "invoice-2024").unwrap();

        let tag_links = db.list_file_tags(f1.id).unwrap();
        assert!(tag_links.iter().any(|t| t.id == created_tag.id));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_duplicate_group_approval_metadata_only_and_preservation_of_original_files() {
        let dir = temp_auth_dir("dup_preserve");
        let (db, db_dir) = test_auth_db("dup_preserve");

        let root = db.add_scan_root(dir.to_str().unwrap(), "Root").unwrap();

        // Create actual physical files on disk
        let file1_path = dir.join("original.txt");
        let file2_path = dir.join("copy.txt");
        let body1 = "CONFIDENTIAL ORIGINAL CONTRACT DATA 12345";
        let body2 = "CONFIDENTIAL ORIGINAL CONTRACT DATA 12345";
        fs::write(&file1_path, body1).unwrap();
        fs::write(&file2_path, body2).unwrap();

        let f1 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file1_path.to_str().unwrap().into(),
                normalized_path: file1_path.to_str().unwrap().into(),
                file_name: "original.txt".into(),
                extension: "txt".into(),
                size_bytes: body1.len() as i64,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_exact".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f2 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file2_path.to_str().unwrap().into(),
                normalized_path: file2_path.to_str().unwrap().into(),
                file_name: "copy.txt".into(),
                extension: "txt".into(),
                size_bytes: body2.len() as i64,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_exact".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let group = db
            .insert_duplicate_group("exact_hash", 1.0, &[f1.id, f2.id])
            .unwrap();
        assert_eq!(group.status, "pending");

        // Approve duplicate group, designating f1 as the canonical/preferred file
        let approved_group = approve_duplicate_group_inner(&db, group.id, Some(f1.id)).unwrap();
        assert_eq!(approved_group.status, "approved");

        // 1. Verify metadata changes in database
        let tags_f1 = db.list_file_tags(f1.id).unwrap();
        let tags_f2 = db.list_file_tags(f2.id).unwrap();

        assert!(tags_f1.iter().any(|t| t.name == "duplicate"));
        assert!(tags_f1.iter().any(|t| t.name == "canonical"));
        assert!(tags_f2.iter().any(|t| t.name == "duplicate"));
        assert!(!tags_f2.iter().any(|t| t.name == "canonical"));

        let notes_f2 = db.list_file_notes(f2.id).unwrap();
        assert!(notes_f2
            .iter()
            .any(|n| n.note_text.contains(&format!("file #{}", f1.id))));

        // 2. CRITICAL RULE: Original physical files on disk MUST NOT be moved, renamed, altered, or deleted!
        assert!(
            file1_path.exists(),
            "original file 1 must still exist on disk"
        );
        assert!(
            file2_path.exists(),
            "original file 2 must still exist on disk"
        );
        assert_eq!(
            fs::read_to_string(&file1_path).unwrap(),
            body1,
            "original file 1 contents must be unaltered"
        );
        assert_eq!(
            fs::read_to_string(&file2_path).unwrap(),
            body2,
            "original file 2 contents must be unaltered"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_suggestion_invalid_and_missing_ids() {
        let dir = temp_auth_dir("sugg_invalids");
        let (db, db_dir) = test_auth_db("sugg_invalids");

        // Invalid negative or zero suggestion ID
        let err_neg = approve_suggestion_inner(&db, -1);
        assert!(err_neg.is_err());
        assert!(err_neg.unwrap_err().contains("must be a positive integer"));

        let err_zero = approve_suggestion_inner(&db, 0);
        assert!(err_zero.is_err());
        assert!(err_zero.unwrap_err().contains("must be a positive integer"));

        // Nonexistent suggestion ID
        let err_nonexist = approve_suggestion_inner(&db, 99999);
        assert!(err_nonexist.is_err());
        assert!(err_nonexist
            .unwrap_err()
            .contains("Suggestion #99999 not found"));

        // Suggestion referencing nonexistent file ID
        let root = db.add_scan_root(dir.to_str().unwrap(), "Root").unwrap();
        let f_ghost = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("ghost.txt").to_str().unwrap().into(),
                normalized_path: dir.join("ghost.txt").to_str().unwrap().into(),
                file_name: "ghost.txt".into(),
                extension: "txt".into(),
                size_bytes: 50,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let s_missing_file = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag ghost file",
                "Invalid test",
                0.8,
                &[f_ghost.id],
                None,
                Some("ghost-tag"),
                None,
                None,
            )
            .unwrap();

        // Delete the file with foreign keys disabled to simulate an orphaned link
        db.open()
            .unwrap()
            .execute_batch(&format!(
                "PRAGMA foreign_keys = OFF; DELETE FROM indexed_files WHERE id = {}; PRAGMA foreign_keys = ON;",
                f_ghost.id
            ))
            .unwrap();

        let err_ghost = approve_suggestion_inner(&db, s_missing_file.id);
        assert!(err_ghost.is_err());
        assert!(err_ghost.unwrap_err().contains("not found in library"));

        // Suggestion referencing file in disabled scan root
        let f_disabled = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: dir.join("disabled.txt").to_str().unwrap().into(),
                normalized_path: dir.join("disabled.txt").to_str().unwrap().into(),
                file_name: "disabled.txt".into(),
                extension: "txt".into(),
                size_bytes: 50,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        db.remove_scan_root(root.id).unwrap();

        let s_disabled = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Tag disabled root file",
                "Testing disabled root",
                0.8,
                &[f_disabled.id],
                None,
                Some("disabled-tag"),
                None,
                None,
            )
            .unwrap();

        let err_disabled = approve_suggestion_inner(&db, s_disabled.id);
        assert!(err_disabled.is_err());
        assert!(err_disabled.unwrap_err().contains("is disabled"));

        // Duplicate group invalid ID
        let err_dg_neg = approve_duplicate_group_inner(&db, -2, None);
        assert!(err_dg_neg.is_err());
        assert!(err_dg_neg
            .unwrap_err()
            .contains("must be a positive integer"));

        let err_dg_nonexist = approve_duplicate_group_inner(&db, 99999, None);
        assert!(err_dg_nonexist.is_err());
        assert!(err_dg_nonexist
            .unwrap_err()
            .contains("Duplicate group #99999 not found"));

        // Duplicate group with invalid preferred file ID
        let root2_dir = dir.join("root2");
        let _ = fs::create_dir_all(&root2_dir);
        let root2 = db
            .add_scan_root(root2_dir.to_str().unwrap(), "Root2")
            .unwrap();
        let f_dg = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root2.id,
                absolute_path: root2_dir.join("dg.txt").to_str().unwrap().into(),
                normalized_path: root2_dir.join("dg.txt").to_str().unwrap().into(),
                file_name: "dg.txt".into(),
                extension: "txt".into(),
                size_bytes: 50,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();
        let grp = db
            .insert_duplicate_group("name_and_size", 0.85, &[f_dg.id])
            .unwrap();
        let err_pref = approve_duplicate_group_inner(&db, grp.id, Some(99999));
        assert!(err_pref.is_err());
        assert!(err_pref
            .unwrap_err()
            .contains("not part of duplicate group"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_suggestion_repeated_actions_idempotency() {
        let (db, db_dir) = test_auth_db("sugg_idempotent");

        let s = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Idempotent tag proposal",
                "Testing idempotency",
                0.8,
                &[],
                None,
                Some("idempotent-tag"),
                None,
                None,
            )
            .unwrap();

        // 1. Repeated approval
        let app1 = approve_suggestion_inner(&db, s.id).unwrap();
        assert_eq!(app1.status, "approved");

        let app2 = approve_suggestion_inner(&db, s.id).unwrap();
        assert_eq!(app2.status, "approved");
        assert_eq!(app1.id, app2.id);

        // 2. Repeated rejection
        let s_rej = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Reject proposal",
                "Testing reject idempotency",
                0.8,
                &[],
                None,
                Some("rejected-tag"),
                None,
                None,
            )
            .unwrap();

        let rej1 = reject_suggestion_inner(&db, s_rej.id).unwrap();
        assert_eq!(rej1.status, "rejected");

        let rej2 = reject_suggestion_inner(&db, s_rej.id).unwrap();
        assert_eq!(rej2.status, "rejected");

        // 3. Repeated dismissal
        let s_dis = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Dismiss proposal",
                "Testing dismiss idempotency",
                0.8,
                &[],
                None,
                Some("dismissed-tag"),
                None,
                None,
            )
            .unwrap();

        let dis1 = dismiss_suggestion_inner(&db, s_dis.id).unwrap();
        assert_eq!(dis1.status, "dismissed");

        let dis2 = dismiss_suggestion_inner(&db, s_dis.id).unwrap();
        assert_eq!(dis2.status, "dismissed");

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_bulk_approve_and_reject_suggestions() {
        let (db, db_dir) = test_auth_db("sugg_bulk");

        let s1 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Bulk 1",
                "Rationale",
                0.8,
                &[],
                None,
                Some("bulk1"),
                None,
                None,
            )
            .unwrap();

        let s2 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Bulk 2",
                "Rationale",
                0.8,
                &[],
                None,
                Some("bulk2"),
                None,
                None,
            )
            .unwrap();

        // Bulk approve s1, s2, and invalid id 99999
        let bulk_app = bulk_approve_suggestions_inner(&db, &[s1.id, s2.id, 99999]).unwrap();
        assert_eq!(bulk_app.approved_ids, vec![s1.id, s2.id]);
        assert_eq!(bulk_app.failed_ids, vec![99999]);
        assert_eq!(bulk_app.errors.len(), 1);
        assert!(bulk_app.errors[0].contains("Suggestion #99999 not found"));

        // Verify status
        assert_eq!(
            db.get_organization_suggestion(s1.id)
                .unwrap()
                .unwrap()
                .status,
            "approved"
        );
        assert_eq!(
            db.get_organization_suggestion(s2.id)
                .unwrap()
                .unwrap()
                .status,
            "approved"
        );

        // Bulk reject
        let s3 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Bulk 3",
                "Rationale",
                0.8,
                &[],
                None,
                Some("bulk3"),
                None,
                None,
            )
            .unwrap();

        let s4 = db
            .insert_organization_suggestion(
                "suggest_tag",
                "Bulk 4",
                "Rationale",
                0.8,
                &[],
                None,
                Some("bulk4"),
                None,
                None,
            )
            .unwrap();

        let bulk_rej = bulk_reject_suggestions_inner(&db, &[s3.id, s4.id]).unwrap();
        assert_eq!(bulk_rej.rejected_ids, vec![s3.id, s4.id]);
        assert!(bulk_rej.failed_ids.is_empty());

        assert_eq!(
            db.get_organization_suggestion(s3.id)
                .unwrap()
                .unwrap()
                .status,
            "rejected"
        );
        assert_eq!(
            db.get_organization_suggestion(s4.id)
                .unwrap()
                .unwrap()
                .status,
            "rejected"
        );

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_duplicate_group_and_comparison_detail_views() {
        let dir = temp_auth_dir("dup_detail_view");
        let (db, db_dir) = test_auth_db("dup_detail_view");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Test Root")
            .unwrap();
        let path1 = dir.join("report_v1.pdf");
        let path2 = dir.join("report_v2.pdf");
        fs::write(&path1, "PDF content 1").unwrap();
        fs::write(&path2, "PDF content 2").unwrap();

        let f1 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path1.to_str().unwrap().into(),
                normalized_path: path1.to_str().unwrap().into(),
                file_name: "report_v1.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 13,
                modified_at: Some("2026-11-20T10:00:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_1111".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f2 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path2.to_str().unwrap().into(),
                normalized_path: path2.to_str().unwrap().into(),
                file_name: "report_v2.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 13,
                modified_at: Some("2026-11-20T10:05:00Z".into()),
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("hash_1111".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let group = db
            .insert_duplicate_group("exact_hash", 1.0, &[f1.id, f2.id])
            .unwrap();

        // 1. Test get_duplicate_group_detail_inner
        let detail = get_duplicate_group_detail_inner(&db, group.id).unwrap();
        assert_eq!(detail.group.id, group.id);
        assert_eq!(detail.files.len(), 2);
        assert_eq!(detail.files[0].file.id, f1.id);
        assert_eq!(detail.files[0].root_display_name, "Test Root");
        assert!(detail.files[0].root_enabled);
        assert!(detail.files[0].exists_on_disk);
        assert!(!detail.files[0].is_canonical);

        // 2. Test get_file_comparison_inner
        let comp = get_file_comparison_inner(&db, vec![f1.id, f2.id]).unwrap();
        assert_eq!(comp.len(), 2);
        assert_eq!(comp[0].file.file_name, "report_v1.pdf");
        assert_eq!(comp[1].file.file_name, "report_v2.pdf");
        assert_eq!(
            comp[0].file.content_hash_nullable.as_deref(),
            Some("hash_1111")
        );
        assert_eq!(
            comp[1].file.content_hash_nullable.as_deref(),
            Some("hash_1111")
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_mark_duplicate_preferred_action_and_metadata_updates() {
        let dir = temp_auth_dir("dup_pref_action");
        let (db, db_dir) = test_auth_db("dup_pref_action");

        let root = db.add_scan_root(dir.to_str().unwrap(), "Docs").unwrap();
        let path1 = dir.join("doc1.pdf");
        let path2 = dir.join("doc2.pdf");
        fs::write(&path1, "Contract doc content").unwrap();
        fs::write(&path2, "Contract doc content").unwrap();

        let f1 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path1.to_str().unwrap().into(),
                normalized_path: path1.to_str().unwrap().into(),
                file_name: "doc1.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 20,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("sha_contract".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f2 = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path2.to_str().unwrap().into(),
                normalized_path: path2.to_str().unwrap().into(),
                file_name: "doc2.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 20,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("sha_contract".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let group = db
            .insert_duplicate_group("exact_hash", 1.0, &[f1.id, f2.id])
            .unwrap();

        // Mark f2 as preferred
        let updated = mark_duplicate_preferred_inner(&db, group.id, f2.id).unwrap();
        assert_eq!(updated.group.status, "approved");

        // Verify f2 is canonical and f1 is not
        let item_f1 = updated.files.iter().find(|i| i.file.id == f1.id).unwrap();
        let item_f2 = updated.files.iter().find(|i| i.file.id == f2.id).unwrap();
        assert!(!item_f1.is_canonical);
        assert!(item_f2.is_canonical);

        // Verify tags: both have "duplicate", only f2 has "canonical"
        let tags1 = db.list_file_tags(f1.id).unwrap();
        let tags2 = db.list_file_tags(f2.id).unwrap();
        assert!(tags1.iter().any(|t| t.name == "duplicate"));
        assert!(!tags1.iter().any(|t| t.name == "canonical"));
        assert!(tags2.iter().any(|t| t.name == "duplicate"));
        assert!(tags2.iter().any(|t| t.name == "canonical"));

        // Switch preferred to f1
        let updated2 = mark_duplicate_preferred_inner(&db, group.id, f1.id).unwrap();
        let item_f1_new = updated2.files.iter().find(|i| i.file.id == f1.id).unwrap();
        let item_f2_new = updated2.files.iter().find(|i| i.file.id == f2.id).unwrap();
        assert!(item_f1_new.is_canonical);
        assert!(!item_f2_new.is_canonical);

        // Verify disk contents untouched
        assert_eq!(fs::read_to_string(&path1).unwrap(), "Contract doc content");
        assert_eq!(fs::read_to_string(&path2).unwrap(), "Contract doc content");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_duplicate_and_organization_safety_states() {
        let dir = temp_auth_dir("dup_safety_states");
        let (db, db_dir) = test_auth_db("dup_safety_states");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Safety Root")
            .unwrap();
        let path_real = dir.join("exists.pdf");
        let path_missing = dir.join("missing.pdf");
        let path_inacc = dir.join("inaccessible.pdf");
        fs::write(&path_real, "real file").unwrap();
        fs::write(&path_inacc, "inaccessible file").unwrap();

        let f_real = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path_real.to_str().unwrap().into(),
                normalized_path: path_real.to_str().unwrap().into(),
                file_name: "exists.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 9,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("h_real".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f_missing = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path_missing.to_str().unwrap().into(),
                normalized_path: path_missing.to_str().unwrap().into(),
                file_name: "missing.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 9,
                modified_at: None,
                created_at_if_available: None,
                status: "missing".into(),
                content_hash_nullable: Some("h_real".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let f_inacc = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path_inacc.to_str().unwrap().into(),
                normalized_path: path_inacc.to_str().unwrap().into(),
                file_name: "inaccessible.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 17,
                modified_at: None,
                created_at_if_available: None,
                status: "inaccessible".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let grp = db
            .insert_duplicate_group("exact_hash", 1.0, &[f_real.id, f_missing.id])
            .unwrap();

        // 1. Attempting to mark a missing file as preferred fails safely
        let err_missing = mark_duplicate_preferred_inner(&db, grp.id, f_missing.id);
        assert!(err_missing.is_err());
        let msg = err_missing.unwrap_err();
        assert!(msg.contains("missing") || msg.contains("not exist on disk"));

        // 2. Attempting to mark an inaccessible file as preferred fails safely
        let grp2 = db
            .insert_duplicate_group("name_and_size", 0.7, &[f_real.id, f_inacc.id])
            .unwrap();
        let err_inacc = mark_duplicate_preferred_inner(&db, grp2.id, f_inacc.id);
        assert!(err_inacc.is_err());
        assert!(err_inacc.unwrap_err().contains("inaccessible"));

        // 3. Approving duplicate group where all files are missing fails safely
        let grp_all_missing = db
            .insert_duplicate_group("exact_hash", 1.0, &[f_missing.id])
            .unwrap();
        let err_all_missing = approve_duplicate_group_inner(&db, grp_all_missing.id, None);
        assert!(err_all_missing.is_err());
        assert!(err_all_missing
            .unwrap_err()
            .contains("all files in the group are missing"));

        // 4. Disabled scan root fails safely for approval and preferred action
        db.remove_scan_root(root.id).unwrap(); // disables root
        let err_disabled_app = approve_duplicate_group_inner(&db, grp.id, Some(f_real.id));
        assert!(err_disabled_app.is_err());
        assert!(err_disabled_app.unwrap_err().contains("is disabled"));

        let err_disabled_pref = mark_duplicate_preferred_inner(&db, grp.id, f_real.id);
        assert!(err_disabled_pref.is_err());
        assert!(err_disabled_pref.unwrap_err().contains("is disabled"));

        // 5. Moved file handling:
        // Re-enable root
        db.enable_scan_root(root.id, "Safety Root").unwrap();
        let path_moved = dir.join("moved_to_subfolder.pdf");
        fs::rename(&path_real, &path_moved).unwrap();

        // Before scanner runs, old path does not exist on disk, so marking f_real as preferred fails safely
        let err_not_found = mark_duplicate_preferred_inner(&db, grp.id, f_real.id);
        assert!(err_not_found.is_err());
        assert!(err_not_found
            .unwrap_err()
            .contains("does not exist on disk"));

        // Once updated in DB with new path, it succeeds safely
        let conn = db.open().unwrap();
        conn.execute(
            "UPDATE indexed_files SET absolute_path=?1, normalized_path=?2 WHERE id=?3",
            rusqlite::params![
                path_moved.to_str().unwrap(),
                path_moved.to_str().unwrap(),
                f_real.id
            ],
        )
        .unwrap();

        let succ_moved = mark_duplicate_preferred_inner(&db, grp.id, f_real.id);
        assert!(succ_moved.is_ok());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_local_ai_disabled_and_app_works_without_ai() {
        // 1. Verify get_local_ai_status_inner returns disabled status
        let status = get_local_ai_status_inner().unwrap();
        assert!(!status.is_available);
        assert_eq!(status.status_message, "Local AI is not configured");
        assert_eq!(status.provider_name, "DisabledLocalProvider");
        assert!(status.model_name.is_none());
        assert!(
            !status.allows_network,
            "Network requests must be prohibited"
        );

        // 2. Verify deterministic suggestion generation works completely offline without AI
        let (db, db_dir) = test_auth_db("no_ai_app_test");
        let dir = temp_auth_dir("no_ai_app_test");
        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Offline Root")
            .unwrap();

        let path1 = dir.join("file1.pdf");
        let path2 = dir.join("file2.pdf");
        fs::write(&path1, "Offline doc 1").unwrap();
        fs::write(&path2, "Offline doc 2").unwrap();

        let _ = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path1.to_str().unwrap().into(),
                normalized_path: path1.to_str().unwrap().into(),
                file_name: "file1.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 13,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("offline_hash".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let _ = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: path2.to_str().unwrap().into(),
                normalized_path: path2.to_str().unwrap().into(),
                file_name: "file2.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 13,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("offline_hash".into()),
                file_id_nullable: None,
            })
            .unwrap();

        // Deterministic engine runs with no error when AI is not configured
        let gen_res = generate_suggestions_inner(&db);
        assert!(gen_res.is_ok());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    // ══════════════════════════════════════════════════════════════════════
    //  Phase 4 — Security-focused tests
    // ══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_security_path_traversal_outside_root_blocked() {
        // Create two separate directories — root and sibling
        let root_dir = temp_auth_dir("sec_traversal_root");
        let sibling_dir = temp_auth_dir("sec_traversal_sibling");
        let (db, db_dir) = test_auth_db("sec_traversal");

        let root = db
            .add_scan_root(root_dir.to_str().unwrap(), "Safe Root")
            .unwrap();

        // Create a file in the sibling directory (outside root)
        let outside_file = sibling_dir.join("secret.pdf");
        fs::write(&outside_file, "secret content").unwrap();

        // Index the outside file under the wrong root
        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: outside_file.to_str().unwrap().into(),
                normalized_path: outside_file
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "secret.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 14,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // validate_file_for_access must reject this path
        let result = validate_file_for_access(&db, indexed.id);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("Security violation"),
            "Path outside root must be caught as a security violation"
        );

        let _ = fs::remove_dir_all(&root_dir);
        let _ = fs::remove_dir_all(&sibling_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_dotdot_traversal_in_path_blocked() {
        let root_dir = temp_auth_dir("sec_dotdot");
        let (db, db_dir) = test_auth_db("sec_dotdot");

        let root = db
            .add_scan_root(root_dir.to_str().unwrap(), "Dot-Dot Root")
            .unwrap();

        // Craft a path with "../" that would escape the root
        let malicious_path = format!(
            "{}\\..\\..\\Windows\\System32\\drivers\\etc\\hosts",
            root_dir.to_str().unwrap()
        );

        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: malicious_path.clone(),
                normalized_path: malicious_path.to_lowercase(),
                file_name: "hosts".into(),
                extension: "".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let result = validate_file_for_access(&db, indexed.id);
        assert!(result.is_err(), "Path with ../ traversal must be rejected");

        let _ = fs::remove_dir_all(&root_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_unauthorized_file_id_rejected() {
        let (db, db_dir) = test_auth_db("sec_bad_id");

        // Various invalid / non-existent file IDs
        for bad_id in &[0i64, -1, -999, 999_999_999, i64::MAX] {
            let result = validate_file_for_access(&db, *bad_id);
            assert!(result.is_err(), "File ID {} must be rejected", bad_id);
            let err = result.unwrap_err();
            assert!(
                err.contains("not indexed") || err.contains("Authorization denied"),
                "Error for ID {} must reference authorization failure",
                bad_id
            );
        }

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_disabled_root_blocks_file_access() {
        let dir = temp_auth_dir("sec_disabled_root");
        let (db, db_dir) = test_auth_db("sec_disabled_root");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Disabled Root")
            .unwrap();

        let file_path = dir.join("document.pdf");
        fs::write(&file_path, "some content").unwrap();

        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file_path.to_str().unwrap().into(),
                normalized_path: file_path
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "document.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 12,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // Access should work while root is enabled
        assert!(validate_file_for_access(&db, indexed.id).is_ok());

        // Disable the root (remove_scan_root sets enabled=false)
        db.remove_scan_root(root.id).unwrap();

        // Access must now fail
        let result = validate_file_for_access(&db, indexed.id);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("disabled"),
            "Disabled root must block file access"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_missing_file_safe_error_message() {
        let dir = temp_auth_dir("sec_missing_file");
        let (db, db_dir) = test_auth_db("sec_missing_file");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Missing Root")
            .unwrap();

        let missing_path = dir.join("vanished.pdf");
        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: missing_path.to_str().unwrap().into(),
                normalized_path: missing_path
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "vanished.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let result = validate_file_for_access(&db, indexed.id);
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        // Error should mention the file is missing, not expose secrets
        assert!(err_msg.contains("not found on disk") || err_msg.contains("unavailable"));
        // Must not contain stack traces, SQL, or credential keywords
        assert!(!err_msg.contains("password"));
        assert!(!err_msg.contains("token"));
        assert!(!err_msg.contains("rusqlite::"));
        assert!(!err_msg.contains("PRAGMA"));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_invalid_export_path_rejected() {
        let (db, db_dir) = test_auth_db("sec_export_path");
        let dir = temp_auth_dir("sec_export_path");
        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Export Root")
            .unwrap();

        let f = dir.join("export_test.txt");
        fs::write(&f, "content").unwrap();
        let _ = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: f.to_str().unwrap().into(),
                normalized_path: f.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "export_test.txt".into(),
                extension: "txt".into(),
                size_bytes: 7,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // Export to a nonexistent deeply nested path should fail
        let bad_path = "Z:\\nonexistent\\deeply\\nested\\output.json";
        let export = db.export_library().map_err(|e| format!("{e}")).unwrap();
        validate_export(&export).unwrap(); // export data itself is valid
        let json = serde_json::to_string_pretty(&export).unwrap();
        let write_result = atomic_write_json(bad_path, &json);
        assert!(
            write_result.is_err(),
            "Writing to a nonexistent path must fail"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_malformed_metadata_rejected() {
        let (db, db_dir) = test_auth_db("sec_malformed");
        let dir = temp_auth_dir("sec_malformed");
        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Malform Root")
            .unwrap();

        let f = dir.join("test.pdf");
        fs::write(&f, "data").unwrap();
        let _ = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: f.to_str().unwrap().into(),
                normalized_path: f.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "test.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // Build a valid export then corrupt various fields
        let valid_export = db.export_library().unwrap();

        // Test 1: NaN confidence in suggestion
        {
            let mut e = valid_export.clone();
            e.organization_suggestions
                .push(crate::models::OrganizationSuggestion {
                    id: 1,
                    suggestion_type: "suggest_tag".into(),
                    title: "Bad confidence".into(),
                    rationale: "test".into(),
                    confidence: f64::NAN,
                    status: "pending".into(),
                    created_at: "2024-01-01T00:00:00Z".into(),
                    reviewed_at: None,
                    affected_file_ids: vec![],
                    suggested_tag_id: None,
                    suggested_tag_name: Some("tag".into()),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            assert!(validate_export(&e).is_err());
            assert!(validate_export(&e).unwrap_err().contains("confidence"));
        }

        // Test 2: Negative file ID
        {
            let mut e = valid_export.clone();
            e.files.push(crate::models::IndexedFile {
                id: -5,
                scan_root_id: root.id,
                absolute_path: "/bad".into(),
                normalized_path: "/bad".into(),
                file_name: "bad.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 0,
                modified_at: None,
                created_at_if_available: None,
                discovered_at: "2024-01-01T00:00:00Z".into(),
                last_seen_at: "2024-01-01T00:00:00Z".into(),
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            });
            e.summary.total_files = e.files.len();
            assert!(validate_export(&e).is_err());
            assert!(validate_export(&e)
                .unwrap_err()
                .contains("unique positive IDs"));
        }

        // Test 3: Invalid suggestion status
        {
            let mut e = valid_export.clone();
            e.organization_suggestions
                .push(crate::models::OrganizationSuggestion {
                    id: 2,
                    suggestion_type: "suggest_tag".into(),
                    title: "Bad status".into(),
                    rationale: "test".into(),
                    confidence: 0.5,
                    status: "hacked".into(),
                    created_at: "2024-01-01T00:00:00Z".into(),
                    reviewed_at: None,
                    affected_file_ids: vec![],
                    suggested_tag_id: None,
                    suggested_tag_name: Some("tag".into()),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            assert!(validate_export(&e).is_err());
            assert!(validate_export(&e).unwrap_err().contains("invalid status"));
        }

        // Test 4: Empty suggestion title
        {
            let mut e = valid_export.clone();
            e.organization_suggestions
                .push(crate::models::OrganizationSuggestion {
                    id: 3,
                    suggestion_type: "suggest_tag".into(),
                    title: "   ".into(),
                    rationale: "test".into(),
                    confidence: 0.5,
                    status: "pending".into(),
                    created_at: "2024-01-01T00:00:00Z".into(),
                    reviewed_at: None,
                    affected_file_ids: vec![],
                    suggested_tag_id: None,
                    suggested_tag_name: Some("tag".into()),
                    suggested_collection_id: None,
                    suggested_collection_name: None,
                });
            assert!(validate_export(&e).is_err());
            assert!(validate_export(&e).unwrap_err().contains("empty title"));
        }

        // Test 5: Empty format_version
        {
            let mut e = valid_export.clone();
            e.format_version = String::new();
            assert!(validate_export(&e).is_err());
        }

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_zero_network_dependencies() {
        // Parse Cargo.toml to verify no networking crates are present
        let cargo_toml =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();

        let network_crates = [
            "reqwest",
            "hyper",
            "actix",
            "warp",
            "axum",
            "surf",
            "isahc",
            "ureq",
            "curl",
            "attohttpc",
            "tungstenite",
            "tokio-tungstenite",
            "websocket",
            "h2",
            "tower-http",
        ];

        for crate_name in &network_crates {
            assert!(
                !cargo_toml.contains(crate_name),
                "Network crate '{}' must not appear in Cargo.toml. \
                 LibOne is a fully offline application.",
                crate_name
            );
        }
    }

    #[test]
    fn test_security_export_contains_metadata_only() {
        let (db, db_dir) = test_auth_db("sec_metadata_only");
        let dir = temp_auth_dir("sec_metadata_only");

        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Metadata Root")
            .unwrap();

        let f = dir.join("document.pdf");
        // Write realistic-looking document content
        fs::write(&f, "%PDF-1.4 This is actual document body content that must NEVER appear in exports. Confidential info here.").unwrap();

        let _ = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: f.to_str().unwrap().into(),
                normalized_path: f.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "document.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 97,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: Some("abc123".into()),
                file_id_nullable: None,
            })
            .unwrap();

        let export = db.export_library().unwrap();
        let json = serde_json::to_string(&export).unwrap();

        // Document body content must NEVER appear in export
        assert!(
            !json.contains("actual document body content"),
            "Export must not contain document file body content"
        );
        assert!(
            !json.contains("Confidential info here"),
            "Export must not leak file body text"
        );
        assert!(
            !json.contains("%PDF-1.4 This is"),
            "Export must not embed raw file bytes"
        );

        // Metadata fields that SHOULD be present
        assert!(
            json.contains("document.pdf"),
            "Export should contain file name"
        );
        assert!(json.contains("pdf"), "Export should contain extension");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_errors_do_not_expose_sensitive_data() {
        let (db, db_dir) = test_auth_db("sec_error_safety");
        let dir = temp_auth_dir("sec_error_safety");

        // Collect error messages from various failure scenarios
        let mut error_messages: Vec<String> = Vec::new();

        // 1. Nonexistent file ID
        if let Err(e) = validate_file_for_access(&db, 99999) {
            error_messages.push(e);
        }

        // 2. Nonexistent collection
        if let Err(e) = get_collection_inner(&db, 99999) {
            error_messages.push(e);
        }

        // 3. Nonexistent tag
        if let Err(e) = get_tag_inner(&db, 99999) {
            error_messages.push(e);
        }

        // 4. Empty collection name
        if let Err(e) = create_collection_inner(&db, "", None, None) {
            error_messages.push(e);
        }

        // 5. Empty tag name
        if let Err(e) = create_tag_inner(&db, "", None) {
            error_messages.push(e);
        }

        // 6. File in disabled root
        let root = db
            .add_scan_root(dir.to_str().unwrap(), "Error Root")
            .unwrap();
        let fp = dir.join("file.txt");
        fs::write(&fp, "data").unwrap();
        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: fp.to_str().unwrap().into(),
                normalized_path: fp.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "file.txt".into(),
                extension: "txt".into(),
                size_bytes: 4,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();
        db.remove_scan_root(root.id).unwrap();
        if let Err(e) = validate_file_for_access(&db, indexed.id) {
            error_messages.push(e);
        }

        // Verify no error message contains sensitive patterns
        let sensitive_patterns = [
            "password",
            "secret",
            "token",
            "api_key",
            "apikey",
            "credential",
            "PRAGMA",
            "sqlite3_",
            "backtrace",
            "panicked at",
        ];

        for msg in &error_messages {
            for pattern in &sensitive_patterns {
                assert!(
                    !msg.to_lowercase().contains(&pattern.to_lowercase()),
                    "Error message must not contain '{}'. Found in: {}",
                    pattern,
                    msg
                );
            }
        }

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_parameterized_sql_injection_attempt() {
        let (db, db_dir) = test_auth_db("sec_sql_inject");
        let dir = temp_auth_dir("sec_sql_inject");

        // Attempt SQL injection in scan root name
        let injection_name = "'; DROP TABLE scan_roots; --";
        let root = db
            .add_scan_root(dir.to_str().unwrap(), injection_name)
            .unwrap();
        // If SQL injection worked, the table would be gone and this would fail
        let roots = db.list_scan_roots().unwrap();
        assert!(
            !roots.is_empty(),
            "scan_roots table must survive SQL injection attempt"
        );
        assert_eq!(roots[0].display_name, injection_name);

        // Attempt SQL injection in collection name
        let inj_collection = "test'); DELETE FROM collections; --";
        let _ = db.create_collection(inj_collection, None, None).unwrap();
        let cols = db.list_collections().unwrap();
        assert!(
            !cols.is_empty(),
            "collections table must survive SQL injection attempt"
        );
        assert_eq!(cols[0].name, inj_collection);

        // Attempt SQL injection in tag name
        let inj_tag = "tag'; UPDATE tags SET name='hacked'; --";
        let _ = db.create_tag(inj_tag, None).unwrap();
        let tags = db.list_tags().unwrap();
        let found = tags.iter().find(|t| t.name == inj_tag);
        assert!(
            found.is_some(),
            "Tag with injection string must be stored literally"
        );

        // Attempt SQL injection in file note
        let fp = dir.join("note_test.pdf");
        fs::write(&fp, "data").unwrap();
        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: fp.to_str().unwrap().into(),
                normalized_path: fp.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "note_test.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        let inj_note = "note'); DROP TABLE file_notes; --";
        let note = create_file_note_inner(&db, indexed.id, inj_note).unwrap();
        assert_eq!(note.note_text, inj_note);
        // Table still exists
        let notes = list_file_notes_inner(&db, indexed.id).unwrap();
        assert_eq!(notes.len(), 1);

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_csp_is_strict_in_config() {
        let config_content =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap();

        // CSP must NOT be null (wide open)
        assert!(
            !config_content.contains("\"csp\": null"),
            "CSP must not be null — it must enforce a strict local-only policy"
        );

        // CSP must contain restrictive directives
        assert!(
            config_content.contains("default-src 'self'"),
            "CSP must set default-src to 'self'"
        );
        assert!(
            config_content.contains("script-src 'self'"),
            "CSP must restrict script-src to 'self'"
        );
        assert!(
            config_content.contains("object-src 'none'"),
            "CSP must block object-src"
        );
        assert!(
            config_content.contains("form-action 'none'"),
            "CSP must block form submissions"
        );
    }

    #[test]
    fn test_security_no_opener_plugin_in_config() {
        let config_content =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json")).unwrap();

        assert!(
            !config_content.contains("\"opener\""),
            "Unused opener plugin config must be removed from tauri.conf.json"
        );

        let cargo_toml =
            fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        assert!(
            !cargo_toml.contains("tauri-plugin-opener"),
            "Unused tauri-plugin-opener must be removed from Cargo.toml"
        );
    }

    #[test]
    fn test_security_capabilities_are_minimal() {
        let caps_content = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/capabilities/default.json"
        ))
        .unwrap();

        // Must not grant opener permissions
        assert!(
            !caps_content.contains("opener"),
            "Capabilities must not include opener permissions"
        );

        // Must not grant shell permissions
        assert!(
            !caps_content.contains("shell"),
            "Capabilities must not include shell permissions"
        );

        // Must not grant HTTP or fetch permissions
        assert!(
            !caps_content.contains("http"),
            "Capabilities must not include HTTP permissions"
        );
    }

    #[test]
    fn test_security_validate_file_for_organization_disabled_root() {
        let dir = temp_auth_dir("sec_org_disabled");
        let (db, db_dir) = test_auth_db("sec_org_disabled");

        let root = db.add_scan_root(dir.to_str().unwrap(), "Org Root").unwrap();
        let fp = dir.join("org_file.pdf");
        fs::write(&fp, "data").unwrap();

        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: fp.to_str().unwrap().into(),
                normalized_path: fp.to_str().unwrap().replace('/', "\\").to_lowercase(),
                file_name: "org_file.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // Works when enabled
        assert!(validate_file_for_organization(&db, indexed.id).is_ok());

        // Disable root
        db.remove_scan_root(root.id).unwrap();

        // Organization operations must be blocked
        let res = validate_file_for_organization(&db, indexed.id);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("disabled"));

        // Adding to collection must also fail
        let col = db.create_collection("Test Col", None, None).unwrap();
        let add_res = add_file_to_collection_inner(&db, indexed.id, col.id);
        assert!(add_res.is_err());

        // Adding tag must also fail
        let tag = db.create_tag("Test Tag", None).unwrap();
        let tag_res = add_file_to_tag_inner(&db, indexed.id, tag.id);
        assert!(tag_res.is_err());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_export_format_version_matches() {
        let (db, db_dir) = test_auth_db("sec_version");
        let dir = temp_auth_dir("sec_version");
        let _ = db
            .add_scan_root(dir.to_str().unwrap(), "Version Root")
            .unwrap();

        let export = db.export_library().unwrap();
        assert_eq!(
            export.format_version, EXPORT_FORMAT_VERSION,
            "Export format version must match the declared constant"
        );
        assert_eq!(
            export.format_version, "1.2.0",
            "Current export format version must be 1.2.0"
        );

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_id_validation_rejects_negative_and_zero_ids() {
        let (db, db_dir) = test_auth_db("sec_id_val");

        for &bad_id in &[0i64, -1, -500] {
            // Roots
            assert!(validate_positive_id(bad_id, "Scan root").is_err());
            assert!(validate_positive_id(bad_id, "Scan root")
                .unwrap_err()
                .contains("must be a positive integer"));

            // Files
            assert!(validate_file_for_access(&db, bad_id).is_err());
            assert!(validate_file_for_organization(&db, bad_id).is_err());
            assert!(list_file_collections_inner(&db, bad_id).is_err());
            assert!(list_file_tags_inner(&db, bad_id).is_err());
            assert!(list_file_notes_inner(&db, bad_id).is_err());

            // Collections
            assert!(get_collection_inner(&db, bad_id).is_err());
            assert!(update_collection_inner(&db, bad_id, "Col", None, None).is_err());
            assert!(delete_collection_inner(&db, bad_id).is_err());

            // Tags
            assert!(get_tag_inner(&db, bad_id).is_err());
            assert!(update_tag_inner(&db, bad_id, "Tag", None).is_err());
            assert!(delete_tag_inner(&db, bad_id).is_err());

            // Notes
            assert!(get_file_note_inner(&db, bad_id).is_err());
            assert!(update_file_note_inner(&db, bad_id, "Note text").is_err());
            assert!(delete_file_note_inner(&db, bad_id).is_err());

            // Suggestions
            assert!(get_organization_suggestion_inner(&db, bad_id).is_err());
            assert!(approve_suggestion_inner(&db, bad_id).is_err());
            assert!(reject_suggestion_inner(&db, bad_id).is_err());
            assert!(dismiss_suggestion_inner(&db, bad_id).is_err());

            // Duplicate groups
            assert!(get_duplicate_group_inner(&db, bad_id).is_err());
            assert!(get_duplicate_group_detail_inner(&db, bad_id).is_err());
            assert!(get_file_comparison_inner(&db, vec![bad_id]).is_err());
            assert!(approve_duplicate_group_inner(&db, bad_id, None).is_err());
            assert!(reject_duplicate_group_inner(&db, bad_id).is_err());
            assert!(dismiss_duplicate_group_inner(&db, bad_id).is_err());
        }

        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_security_organization_and_explorer_boundary_enforcement() {
        let (db, db_dir) = test_auth_db("sec_bounds");
        let root_dir = temp_auth_dir("sec_bounds_root");
        let outside_dir = temp_auth_dir("sec_bounds_outside");

        let root = db
            .add_scan_root(root_dir.to_str().unwrap(), "Bounds Root")
            .unwrap();

        // Create file outside scan root
        let outside_file = outside_dir.join("outside.pdf");
        fs::write(&outside_file, "outside content").unwrap();

        let indexed = db
            .insert_indexed_file(&crate::models::NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: outside_file.to_str().unwrap().into(),
                normalized_path: outside_file
                    .to_str()
                    .unwrap()
                    .replace('/', "\\")
                    .to_lowercase(),
                file_name: "outside.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 15,
                modified_at: None,
                created_at_if_available: None,
                status: "indexed".into(),
                content_hash_nullable: None,
                file_id_nullable: None,
            })
            .unwrap();

        // validate_file_for_organization must block this file
        let org_res = validate_file_for_organization(&db, indexed.id);
        assert!(org_res.is_err());
        assert!(org_res
            .unwrap_err()
            .contains("outside authorized scan root boundaries"));

        // validate_file_for_access must also block this file
        let access_res = validate_file_for_access(&db, indexed.id);
        assert!(access_res.is_err());
        assert!(access_res
            .unwrap_err()
            .contains("outside authorized scan root boundaries"));

        let _ = fs::remove_dir_all(&root_dir);
        let _ = fs::remove_dir_all(&outside_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }
}
