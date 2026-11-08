use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Instant;
use chrono::Utc;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::db::Database;
use crate::models::{IndexedFile, ScanStats};

/// Helper to read a partial sha256 hash (up to 64KB) of a file for fast move/rename identification
fn compute_sha256_prefix(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buffer = [0u8; 65536];
    let bytes_read = file.read(&mut buffer).ok()?;
    if bytes_read == 0 {
        return Some("empty".to_string());
    }
    let mut hasher = Sha256::new();
    hasher.update(&buffer[..bytes_read]);
    Some(format!("{:x}", hasher.finalize()))
}

pub fn scan_folder(db: &Database, root_id: i64, folder_path: &str) -> Result<ScanStats, String> {
    let start_time = Instant::now();
    let root_path = Path::new(folder_path);

    if !root_path.exists() || !root_path.is_dir() {
        return Err(format!("Folder path does not exist or is not a directory: {}", folder_path));
    }

    // 1. Fetch current known files from DB for this root
    let existing_files = db
        .get_files_for_root(root_id)
        .map_err(|e| format!("Failed to read existing files from database: {}", e))?;

    let mut existing_by_path: HashMap<String, IndexedFile> = HashMap::new();
    let mut existing_unseen_by_signature: HashMap<(i64, Option<String>), Vec<IndexedFile>> = HashMap::new();

    for f in existing_files {
        existing_by_path.insert(f.path.clone(), f.clone());
        existing_unseen_by_signature
            .entry((f.size_bytes, f.sha256_prefix.clone()))
            .or_default()
            .push(f);
    }

    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut added_count = 0;
    let mut modified_count = 0;
    let mut moved_count = 0;
    let mut unchanged_count = 0;
    let mut total_scanned = 0;

    let now_iso = Utc::now().to_rfc3339();

    // Open connection for batch writing
    let mut conn = Connection::open(db.get_path())
        .map_err(|e| format!("Failed to open DB connection: {}", e))?;

    let tx = conn
        .transaction()
        .map_err(|e| format!("Failed to start transaction: {}", e))?;

    // Prepare statements
    {
        let mut insert_stmt = tx
            .prepare(
                "INSERT INTO files (
                    scan_root_id, path, filename, extension, size_bytes,
                    modified_at_ms, sha256_prefix, status, first_seen_at, last_seen_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .map_err(|e| format!("Failed to prepare insert: {}", e))?;

        let mut update_stmt = tx
            .prepare(
                "UPDATE files SET
                    filename = ?1, extension = ?2, size_bytes = ?3,
                    modified_at_ms = ?4, sha256_prefix = ?5, status = ?6,
                    last_seen_at = ?7
                 WHERE id = ?8",
            )
            .map_err(|e| format!("Failed to prepare update: {}", e))?;

        let mut move_stmt = tx
            .prepare(
                "UPDATE files SET
                    path = ?1, filename = ?2, extension = ?3, status = 'moved',
                    last_seen_at = ?4
                 WHERE id = ?5",
            )
            .map_err(|e| format!("Failed to prepare move: {}", e))?;

        // 2. Walk directory recursively
        for entry in WalkDir::new(root_path).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            total_scanned += 1;
            let full_path_str = match path.to_str() {
                Some(p) => p.to_string(),
                None => continue,
            };

            seen_paths.insert(full_path_str.clone());

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size_bytes = metadata.len() as i64;
            let modified_at_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            // Case A: File path was already recorded
            if let Some(existing) = existing_by_path.get(&full_path_str) {
                if existing.modified_at_ms != modified_at_ms || existing.size_bytes != size_bytes {
                    // File modified
                    let hash_prefix = compute_sha256_prefix(path);
                    update_stmt
                        .execute(params![
                            filename,
                            extension,
                            size_bytes,
                            modified_at_ms,
                            hash_prefix,
                            "modified",
                            now_iso,
                            existing.id
                        ])
                        .map_err(|e| format!("Failed to update modified file: {}", e))?;
                    modified_count += 1;
                } else {
                    // Unchanged
                    if existing.status == "missing" || existing.status == "moved" {
                        // Restored to active
                        update_stmt
                            .execute(params![
                                filename,
                                extension,
                                size_bytes,
                                modified_at_ms,
                                existing.sha256_prefix,
                                "active",
                                now_iso,
                                existing.id
                            ])
                            .map_err(|e| format!("Failed to re-activate file: {}", e))?;
                    } else {
                        // Purely update last seen
                        tx.execute(
                            "UPDATE files SET last_seen_at = ?1 WHERE id = ?2",
                            params![now_iso, existing.id],
                        )
                        .map_err(|e| format!("Failed to touch last_seen: {}", e))?;
                    }
                    unchanged_count += 1;
                }
            } else {
                // Case B: New path discovered. Check if it matches an existing file that moved.
                let hash_prefix = compute_sha256_prefix(path);
                let sig_key = (size_bytes, hash_prefix.clone());

                let mut matched_moved_id: Option<i64> = None;
                if let Some(candidates) = existing_unseen_by_signature.get_mut(&sig_key) {
                    // Find a candidate whose old path no longer exists on disk
                    let candidate_idx = candidates.iter().position(|c| !Path::new(&c.path).exists());
                    if let Some(idx) = candidate_idx {
                        let candidate = candidates.remove(idx);
                        matched_moved_id = Some(candidate.id);
                    }
                }

                if let Some(moved_id) = matched_moved_id {
                    // Record identified move
                    move_stmt
                        .execute(params![full_path_str, filename, extension, now_iso, moved_id])
                        .map_err(|e| format!("Failed to record file move: {}", e))?;
                    moved_count += 1;
                } else {
                    // Fresh addition
                    insert_stmt
                        .execute(params![
                            root_id,
                            full_path_str,
                            filename,
                            extension,
                            size_bytes,
                            modified_at_ms,
                            hash_prefix,
                            "active",
                            now_iso,
                            now_iso
                        ])
                        .map_err(|e| format!("Failed to insert new file: {}", e))?;
                    added_count += 1;
                }
            }
        }

        // 3. Detect missing files: previously recorded files in this root not seen in this scan
        let mut missing_count = 0;
        let mut mark_missing_stmt = tx
            .prepare("UPDATE files SET status = 'missing', last_seen_at = ?1 WHERE id = ?2")
            .map_err(|e| format!("Failed to prepare mark missing: {}", e))?;

        for (old_path, existing) in &existing_by_path {
            if !seen_paths.contains(old_path) {
                // Verify it really doesn't exist on disk
                if !Path::new(old_path).exists() && existing.status != "moved" {
                    mark_missing_stmt
                        .execute(params![now_iso, existing.id])
                        .map_err(|e| format!("Failed to mark missing: {}", e))?;
                    missing_count += 1;
                }
            }
        }
    }

    tx.commit()
        .map_err(|e| format!("Failed to commit scan transaction: {}", e))?;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    let stats = ScanStats {
        root_id,
        total_scanned,
        added: added_count,
        modified: modified_count,
        missing: missing_count,
        moved: moved_count,
        unchanged: unchanged_count,
        duration_ms,
    };

    let _ = db.record_scan_results(&stats);

    Ok(stats)
}
