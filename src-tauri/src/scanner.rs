//! Read-only scanner. Persistence is delegated to `Database`; this module owns no SQL.
use crate::db::Database;
use crate::models::{IndexedFile, NewIndexedFile, ScanStats};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

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

pub fn scan_folder(db: &Database, root_id: i64, folder_path: &str) -> Result<ScanStats, String> {
    let root = Path::new(folder_path);
    if !root.is_dir() {
        return Err(format!(
            "Folder path does not exist or is not a directory: {folder_path}"
        ));
    }
    db.start_scan(root_id).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let existing = db.get_files_for_root(root_id).map_err(|e| e.to_string())?;
    let by_path: HashMap<String, IndexedFile> = existing
        .iter()
        .cloned()
        .map(|f| (f.normalized_path.clone(), f))
        .collect();
    let mut seen = HashSet::new();
    let mut added = 0;
    let mut modified = 0;
    let mut unchanged = 0;
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let path = entry.path();
        let normalized = normalized_path(path);
        seen.insert(normalized.clone());
        let input = NewIndexedFile {
            scan_root_id: root_id,
            absolute_path: path.to_string_lossy().to_string(),
            normalized_path: normalized.clone(),
            file_name: path
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_string(),
            extension: path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_lowercase(),
            size_bytes: metadata.len() as i64,
            modified_at: metadata.modified().ok().and_then(timestamp),
            created_at_if_available: metadata.created().ok().and_then(timestamp),
            content_hash_nullable: None,
            status: "indexed".into(),
        };
        match by_path.get(&normalized) {
            None => {
                db.insert_indexed_file(&input).map_err(|e| e.to_string())?;
                added += 1;
            }
            Some(old)
                if old.size_bytes != input.size_bytes || old.modified_at != input.modified_at =>
            {
                let mut changed = input;
                changed.status = "modified".into();
                db.update_indexed_file(old.id, &changed)
                    .map_err(|e| e.to_string())?;
                modified += 1;
            }
            Some(old) => {
                db.touch_indexed_file(old.id, "indexed")
                    .map_err(|e| e.to_string())?;
                unchanged += 1;
            }
        }
    }
    let mut missing = 0;
    for old in existing {
        if !seen.contains(&old.normalized_path) && !Path::new(&old.absolute_path).exists() {
            db.mark_file_missing(old.id).map_err(|e| e.to_string())?;
            missing += 1;
        }
    }
    let stats = ScanStats {
        root_id,
        total_scanned: seen.len(),
        added,
        modified,
        missing,
        moved: 0,
        unchanged,
        duration_ms: started.elapsed().as_millis() as u64,
    };
    db.record_scan_results(&stats).map_err(|e| e.to_string())?;
    Ok(stats)
}
