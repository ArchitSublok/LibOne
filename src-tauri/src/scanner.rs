//! Robust local file scanner for LibOne scan roots.
//!
//! Features:
//! - Recursively discovers regular files beneath an explicitly selected root.
//! - Fast change detection based on normalized path, size, and modified time.
//! - Stable file identity on Windows using volume serial number and 64-bit file index,
//!   falling back safely to path tracking when unavailable (e.g. FAT32/exFAT/network).
//! - Status model distinguishing `indexed`, `changed`, `missing`, `inaccessible`, and `error`.
//! - Move & rename detection: preserves existing record when identity or unambiguous match allows it.
//! - Optional SHA-256 streaming content hashing (performed only on demand or when requested).
//! - Skips directory symlinks, junctions, build/dependency directories (.git, node_modules, target, etc.),
//!   and LibOne's application-data directory.
//! - Safe cancellation preserving partial progress consistently without marking unscanned files missing.

use crate::db::Database;
use crate::models::{IndexedFile, NewIndexedFile, ScanOptions, ScanProgress, ScanStats, ScanWarning};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

/// Check if a directory name matches common generated build/cache directories that should be skipped.
pub fn is_excluded_dir_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        ".git"
            | ".svn"
            | ".hg"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | ".turbo"
            | ".cache"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".tox"
            | "$recycle.bin"
            | "system volume information"
    )
}

/// Retrieve stable file identity where supported (Windows NTFS/ReFS volume serial + file index).
/// Falls back to None on filesystems that do not expose stable IDs.
pub fn get_file_identity(path: &Path, _metadata: &std::fs::Metadata) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;

        #[repr(C)]
        struct BY_HANDLE_FILE_INFORMATION {
            dw_file_attributes: u32,
            ft_creation_time: [u32; 2],
            ft_last_access_time: [u32; 2],
            ft_last_write_time: [u32; 2],
            dw_volume_serial_number: u32,
            n_file_size_high: u32,
            n_file_size_low: u32,
            n_number_of_links: u32,
            n_file_index_high: u32,
            n_file_index_low: u32,
        }

        extern "system" {
            fn GetFileInformationByHandle(
                h_file: *mut std::ffi::c_void,
                lp_file_information: *mut BY_HANDLE_FILE_INFORMATION,
            ) -> i32;
        }

        // Open with FILE_READ_ATTRIBUTES (0x0080) and full share mode (7) so we don't lock the file
        let file = std::fs::OpenOptions::new()
            .access_mode(0x0080)
            .share_mode(7)
            .open(path)
            .ok()?;

        let handle = file.as_raw_handle();
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        let success = unsafe {
            GetFileInformationByHandle(handle as *mut std::ffi::c_void, info.as_mut_ptr())
        };

        if success != 0 {
            let info = unsafe { info.assume_init() };
            let volume = info.dw_volume_serial_number;
            let index = ((info.n_file_index_high as u64) << 32) | (info.n_file_index_low as u64);
            if index != 0 {
                return Some(format!("{:08x}:{:016x}", volume, index));
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::MetadataExt;
        let dev = _metadata.dev();
        let ino = _metadata.ino();
        if ino != 0 {
            Some(format!("{:x}:{:x}", dev, ino))
        } else {
            None
        }
    }
}

/// Compute SHA-256 hash using a 64 KB streaming buffer.
/// Never loads whole documents into memory.
pub fn compute_file_sha256<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn timestamp(time: SystemTime) -> Option<String> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| chrono::DateTime::<chrono::Utc>::from(UNIX_EPOCH + d).to_rfc3339())
}

/// Execute a robust scan on `folder_path` for `root_id`.
pub fn scan_folder_with_progress<F>(
    db: &Database,
    root_id: i64,
    folder_path: &str,
    app_data_dir: Option<&Path>,
    options: Option<&ScanOptions>,
    cancel_token: Option<&AtomicBool>,
    mut on_progress: F,
) -> Result<ScanProgress, String>
where
    F: FnMut(&ScanProgress),
{
    let root = Path::new(folder_path);
    if !root.is_dir() {
        return Err(format!(
            "Folder path does not exist or is not a directory: {folder_path}"
        ));
    }

    let default_options = ScanOptions::default();
    let opts = options.unwrap_or(&default_options);

    let canonical_app_data: Option<PathBuf> = app_data_dir
        .and_then(|p| p.canonicalize().ok().or_else(|| Some(p.to_path_buf())));

    db.start_scan(root_id).map_err(|e| e.to_string())?;
    let started = Instant::now();

    let mut progress = ScanProgress {
        root_id,
        current_root: folder_path.to_string(),
        current_directory: folder_path.to_string(),
        directories_visited: 0,
        files_discovered: 0,
        files_indexed: 0,
        warnings: Vec::new(),
        state: "running".into(),
        stats: None,
        error_message: None,
    };
    on_progress(&progress);

    let existing = db.get_files_for_root(root_id).map_err(|e| e.to_string())?;

    // Build lookup maps for fast change detection and rename matching
    let mut by_path: HashMap<String, IndexedFile> = HashMap::new();
    let mut by_file_id: HashMap<String, IndexedFile> = HashMap::new();

    for f in &existing {
        by_path.insert(f.normalized_path.clone(), f.clone());
        if let Some(ref fid) = f.file_id_nullable {
            by_file_id.insert(fid.clone(), f.clone());
        }
    }

    let mut matched_existing_ids: HashSet<i64> = HashSet::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    let mut added = 0;
    let mut modified = 0;
    let mut moved = 0;
    let mut unchanged = 0;
    let mut last_progress_emit = Instant::now();

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.file_type().is_dir() {
                let name = entry.file_name().to_string_lossy();
                if is_excluded_dir_name(&name) {
                    return false;
                }
                if entry.path_is_symlink() {
                    return false;
                }
                if let Some(app_data) = &canonical_app_data {
                    if let Ok(canon) = entry.path().canonicalize() {
                        if canon.starts_with(app_data) {
                            return false;
                        }
                    } else if entry.path().starts_with(app_data) {
                        return false;
                    }
                }
            }
            true
        });

    for item in walker {
        // Check for cancellation
        if let Some(token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                progress.state = "cancelled".into();
                let stats = ScanStats {
                    root_id,
                    total_scanned: seen_paths.len(),
                    added,
                    modified,
                    missing: 0,
                    moved,
                    unchanged,
                    duration_ms: started.elapsed().as_millis() as u64,
                };
                progress.stats = Some(stats);
                let _ = db.record_scan_cancelled(root_id);
                on_progress(&progress);
                return Ok(progress);
            }
        }

        // Handle directory traversal errors
        let entry = match item {
            Ok(e) => e,
            Err(err) => {
                let path_str = err
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder_path.to_string());
                progress.warnings.push(ScanWarning {
                    path: path_str,
                    message: err.to_string(),
                    code: "unreadable_path".into(),
                });
                continue;
            }
        };

        if entry.file_type().is_dir() {
            progress.directories_visited += 1;
            progress.current_directory = entry.path().to_string_lossy().to_string();
            if last_progress_emit.elapsed().as_millis() >= 75 {
                on_progress(&progress);
                last_progress_emit = Instant::now();
            }
            continue;
        }

        // Skip symlinks and junctions
        if entry.path_is_symlink() || entry.file_type().is_symlink() {
            progress.warnings.push(ScanWarning {
                path: entry.path().to_string_lossy().to_string(),
                message: "Symbolic link or reparse point skipped".into(),
                code: "symlink_skipped".into(),
            });
            continue;
        }

        if !entry.file_type().is_file() {
            continue;
        }

        // Read metadata safely without reading document bytes into memory
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                let is_permission = e.io_error().map(|io| io.kind() == std::io::ErrorKind::PermissionDenied).unwrap_or(false);
                let is_not_found = e.io_error().map(|io| io.kind() == std::io::ErrorKind::NotFound).unwrap_or(false);

                let code = if is_permission {
                    "inaccessible"
                } else if is_not_found {
                    "file_disappeared"
                } else {
                    "error"
                };

                let path_str = entry.path().to_string_lossy().to_string();
                let norm = normalized_path(entry.path());

                // If previously indexed, update status to inaccessible or error
                if let Some(old) = by_path.get(&norm) {
                    matched_existing_ids.insert(old.id);
                    let _ = db.touch_indexed_file(old.id, code);
                }

                progress.warnings.push(ScanWarning {
                    path: path_str,
                    message: e.to_string(),
                    code: code.into(),
                });
                continue;
            }
        };

        let path = entry.path();
        let normalized = normalized_path(path);
        seen_paths.insert(normalized.clone());
        progress.files_discovered += 1;

        if path.file_name().and_then(|x| x.to_str()).is_none() {
            progress.warnings.push(ScanWarning {
                path: path.to_string_lossy().to_string(),
                message: "Filename contains non-UTF-8 characters, indexed using lossy representation".into(),
                code: "invalid_filename".into(),
            });
        }

        let file_name = path
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default();
        let extension = path
            .extension()
            .map(|x| x.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let size_bytes = metadata.len() as i64;
        let modified_at = metadata.modified().ok().and_then(timestamp);
        let created_at_if_available = metadata.created().ok().and_then(timestamp);
        let file_id = get_file_identity(path, &metadata);

        let hash = if opts.compute_hashes {
            compute_file_sha256(path).ok()
        } else {
            None
        };

        let mut input = NewIndexedFile {
            scan_root_id: root_id,
            absolute_path: path.to_string_lossy().to_string(),
            normalized_path: normalized.clone(),
            file_name,
            extension,
            size_bytes,
            modified_at,
            created_at_if_available,
            content_hash_nullable: hash,
            file_id_nullable: file_id.clone(),
            status: "indexed".into(),
        };

        // ── Case 1: Match by normalized path ─────────────────────────────────
        if let Some(old) = by_path.get(&normalized) {
            matched_existing_ids.insert(old.id);

            let changed = old.size_bytes != input.size_bytes || old.modified_at != input.modified_at;
            if changed {
                input.status = "changed".into();
                // If not recalculating hash now, preserve old hash only if content was not changed
                if !opts.compute_hashes {
                    input.content_hash_nullable = None;
                }
                if let Err(e) = db.update_indexed_file(old.id, &input) {
                    progress.warnings.push(ScanWarning {
                        path: input.absolute_path.clone(),
                        message: format!("Database update error: {e}"),
                        code: "error".into(),
                    });
                    continue;
                }
                modified += 1;
            } else {
                // Fast path: file is unchanged!
                input.status = "indexed".into();
                if let Err(e) = db.touch_indexed_file(old.id, "indexed") {
                    progress.warnings.push(ScanWarning {
                        path: input.absolute_path.clone(),
                        message: format!("Database touch error: {e}"),
                        code: "error".into(),
                    });
                    continue;
                }
                unchanged += 1;
            }
            progress.files_indexed += 1;
            continue;
        }

        // ── Case 2: Match by stable Windows file identity (Rename / Move) ────
        let matched_by_id = if let Some(ref fid) = file_id {
            by_file_id.get(fid).filter(|old| !matched_existing_ids.contains(&old.id))
        } else {
            None
        };

        if let Some(old) = matched_by_id {
            matched_existing_ids.insert(old.id);
            let is_mod = old.size_bytes != input.size_bytes || old.modified_at != input.modified_at;
            input.status = if is_mod { "changed".into() } else { "indexed".into() };

            if let Err(e) = db.move_indexed_file(old.id, &input) {
                progress.warnings.push(ScanWarning {
                    path: input.absolute_path.clone(),
                    message: format!("Database rename/move update error: {e}"),
                    code: "error".into(),
                });
                continue;
            }
            moved += 1;
            progress.files_indexed += 1;
            continue;
        }

        // ── Case 3: Unambiguous match fallback (exact single candidate) ──────
        // If file_id is not available (e.g. FAT32/non-Windows), check if there is
        // exactly ONE unmatched old record with the same filename, size, and modified time.
        let unambiguous_match: Option<&IndexedFile> = if file_id.is_none() {
            let candidates: Vec<&IndexedFile> = existing
                .iter()
                .filter(|old| {
                    !matched_existing_ids.contains(&old.id)
                        && old.file_name == input.file_name
                        && old.size_bytes == input.size_bytes
                        && old.modified_at == input.modified_at
                })
                .collect();
            if candidates.len() == 1 {
                Some(candidates[0])
            } else {
                None
            }
        } else {
            None
        };

        if let Some(old) = unambiguous_match {
            matched_existing_ids.insert(old.id);
            input.status = "indexed".into();

            if let Err(e) = db.move_indexed_file(old.id, &input) {
                progress.warnings.push(ScanWarning {
                    path: input.absolute_path.clone(),
                    message: format!("Database move error: {e}"),
                    code: "error".into(),
                });
                continue;
            }
            moved += 1;
            progress.files_indexed += 1;
            continue;
        }

        // ── Case 4: New file ─────────────────────────────────────────────────
        input.status = "indexed".into();
        if let Err(e) = db.insert_indexed_file(&input) {
            progress.warnings.push(ScanWarning {
                path: input.absolute_path.clone(),
                message: format!("Database insert error: {e}"),
                code: "error".into(),
            });
            continue;
        }
        added += 1;
        progress.files_indexed += 1;

        if last_progress_emit.elapsed().as_millis() >= 75 {
            on_progress(&progress);
            last_progress_emit = Instant::now();
        }
    }

    // ── Post-scan reconciliation for missing or inaccessible files ───────────
    let mut missing = 0;
    for old in &existing {
        if !matched_existing_ids.contains(&old.id) {
            let p = Path::new(&old.absolute_path);
            if !p.exists() {
                // File no longer exists on disk: mark missing, preserve record!
                let _ = db.mark_file_missing(old.id);
                missing += 1;
            } else {
                // File still exists on disk, but was not visited (e.g. inaccessible folder)
                let _ = db.touch_indexed_file(old.id, "inaccessible");
            }
        }
    }

    let stats = ScanStats {
        root_id,
        total_scanned: seen_paths.len(),
        added,
        modified,
        missing,
        moved,
        unchanged,
        duration_ms: started.elapsed().as_millis() as u64,
    };
    db.record_scan_results(&stats).map_err(|e| e.to_string())?;

    progress.state = "completed".into();
    progress.stats = Some(stats);
    on_progress(&progress);

    Ok(progress)
}

/// Convenience wrapper for backward compatibility and synchronous callers.
pub fn scan_folder(db: &Database, root_id: i64, folder_path: &str) -> Result<ScanStats, String> {
    let result = scan_folder_with_progress(db, root_id, folder_path, None, None, None, |_| {})?;
    result
        .stats
        .ok_or_else(|| "Scan finished without returning stats".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("libone_scan_test_{}_{}_{}", name, std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn test_db(name: &str) -> (Database, PathBuf) {
        let p = std::env::temp_dir().join(format!(
            "libone_db_test_{}_{}_{}",
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
    fn test_is_excluded_dir_name() {
        assert!(is_excluded_dir_name(".git"));
        assert!(is_excluded_dir_name("NODE_MODULES"));
        assert!(is_excluded_dir_name("target"));
        assert!(is_excluded_dir_name("__pycache__"));
        assert!(is_excluded_dir_name(".venv"));
        assert!(!is_excluded_dir_name("my_documents"));
        assert!(!is_excluded_dir_name("reports"));
    }

    #[test]
    fn test_scan_discovers_regular_files_and_skips_excluded() {
        let test_dir = temp_test_dir("exclusions");
        let (db, db_dir) = test_db("exclusions");

        fs::write(test_dir.join("file1.txt"), b"Hello World").unwrap();
        fs::write(test_dir.join("file2.pdf"), b"%PDF-1.4").unwrap();

        let sub = test_dir.join("subfolder");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("file3.docx"), b"docx content").unwrap();

        let git_dir = test_dir.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("config"), b"git config").unwrap();

        let node_modules = test_dir.join("node_modules");
        fs::create_dir_all(&node_modules).unwrap();
        fs::write(node_modules.join("package.json"), b"{}").unwrap();

        let root = db.add_scan_root(test_dir.to_str().unwrap(), "Test Root").unwrap();

        let progress = scan_folder_with_progress(
            &db,
            root.id,
            test_dir.to_str().unwrap(),
            Some(&db_dir),
            None,
            None,
            |_| {},
        )
        .unwrap();

        assert_eq!(progress.state, "completed");
        let stats = progress.stats.unwrap();
        assert_eq!(stats.total_scanned, 3);
        assert_eq!(stats.added, 3);
        assert_eq!(stats.missing, 0);

        let files = db.get_files_for_root(root.id).unwrap();
        assert_eq!(files.len(), 3);

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_modification_detection() {
        let test_dir = temp_test_dir("mod_detect");
        let (db, db_dir) = test_db("mod_detect");

        let f1 = test_dir.join("report.txt");
        fs::write(&f1, b"Initial version of document").unwrap();

        let root = db.add_scan_root(test_dir.to_str().unwrap(), "Mod Root").unwrap();

        // 1. Initial scan: indexed
        let p1 = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();
        assert_eq!(p1.stats.unwrap().added, 1);

        let files_before = db.get_files_for_root(root.id).unwrap();
        assert_eq!(files_before[0].status, "indexed");

        // 2. Modify document contents & mtime
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&f1, b"Updated version with much more content added here").unwrap();

        // 3. Second scan: detects change
        let p2 = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();
        let s2 = p2.stats.unwrap();
        assert_eq!(s2.modified, 1);
        assert_eq!(s2.unchanged, 0);

        let files_after = db.get_files_for_root(root.id).unwrap();
        assert_eq!(files_after[0].status, "changed");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_deletion_detection() {
        let test_dir = temp_test_dir("del_detect");
        let (db, db_dir) = test_db("del_detect");

        let f1 = test_dir.join("preserved.txt");
        let f2 = test_dir.join("to_delete.txt");
        fs::write(&f1, b"Keep this").unwrap();
        fs::write(&f2, b"Delete this").unwrap();

        let root = db.add_scan_root(test_dir.to_str().unwrap(), "Del Root").unwrap();

        // Initial scan
        let _ = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();

        // Delete f2
        fs::remove_file(&f2).unwrap();

        // Second scan: detects missing file
        let p2 = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();
        let s2 = p2.stats.unwrap();
        assert_eq!(s2.missing, 1);
        assert_eq!(s2.unchanged, 1);

        // Verification: records are NOT deleted from SQLite
        let files = db.get_files_for_root(root.id).unwrap();
        assert_eq!(files.len(), 2);
        let deleted_record = files.iter().find(|f| f.file_name == "to_delete.txt").unwrap();
        assert_eq!(deleted_record.status, "missing");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_rename_detection() {
        let test_dir = temp_test_dir("rename_detect");
        let (db, db_dir) = test_db("rename_detect");

        let orig_path = test_dir.join("draft_v1.txt");
        fs::write(&orig_path, b"Project Draft Content").unwrap();

        let root = db.add_scan_root(test_dir.to_str().unwrap(), "Rename Root").unwrap();

        // First scan
        let p1 = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();
        assert_eq!(p1.stats.unwrap().added, 1);

        let files_initial = db.get_files_for_root(root.id).unwrap();
        let initial_id = files_initial[0].id;

        // Rename file on disk inside the root
        let new_path = test_dir.join("final_v2.txt");
        fs::rename(&orig_path, &new_path).unwrap();

        // Second scan: recognizes rename
        let p2 = scan_folder_with_progress(&db, root.id, test_dir.to_str().unwrap(), None, None, None, |_| {}).unwrap();
        let s2 = p2.stats.unwrap();

        // Exactly one file moved, zero missing
        assert_eq!(s2.moved, 1);
        assert_eq!(s2.missing, 0);

        let files_after = db.get_files_for_root(root.id).unwrap();
        assert_eq!(files_after.len(), 1);
        assert_eq!(files_after[0].id, initial_id, "Must preserve existing database record ID");
        assert_eq!(files_after[0].file_name, "final_v2.txt");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_cancellation_preserves_partial_progress() {
        let test_dir = temp_test_dir("cancel_test");
        let (db, db_dir) = test_db("cancel_test");

        for i in 0..10 {
            fs::write(test_dir.join(format!("doc_{i}.txt")), b"some data").unwrap();
        }

        let root = db.add_scan_root(test_dir.to_str().unwrap(), "Cancel Root").unwrap();

        let cancel_token = AtomicBool::new(true);

        let progress = scan_folder_with_progress(
            &db,
            root.id,
            test_dir.to_str().unwrap(),
            None,
            None,
            Some(&cancel_token),
            |_| {},
        )
        .unwrap();

        assert_eq!(progress.state, "cancelled");
        let root_record = db.get_scan_root(root.id).unwrap().unwrap();
        assert_eq!(root_record.last_scan_status, "cancelled");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&db_dir);
    }

    #[test]
    fn test_optional_streaming_sha256() {
        let test_dir = temp_test_dir("hash_test");
        let file = test_dir.join("hasheme.txt");
        fs::write(&file, b"LibOne safe streaming hashing").unwrap();

        let hash = compute_file_sha256(&file).unwrap();
        assert_eq!(hash.len(), 64);

        // Verify SHA-256 against standard computation:
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"LibOne safe streaming hashing");
        let expected = format!("{:x}", hasher.finalize());
        assert_eq!(hash, expected);

        let _ = fs::remove_dir_all(&test_dir);
    }
}
