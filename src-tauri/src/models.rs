use serde::{Deserialize, Serialize};

/// A user-approved directory LibOne indexes in place. Disabling it never changes the folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRoot {
    pub id: i64,
    pub absolute_path: String,
    pub display_name: String,
    pub enabled: bool,
    pub created_at: String,
    pub last_scan_started_at: Option<String>,
    pub last_scan_completed_at: Option<String>,
    pub last_scan_status: String,
    pub file_count: i64,
}

/// Local metadata for an original file. Document contents are never stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub id: i64,
    pub scan_root_id: i64,
    pub absolute_path: String,
    pub normalized_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: i64,
    pub modified_at: Option<String>,
    pub created_at_if_available: Option<String>,
    pub discovered_at: String,
    pub last_seen_at: String,
    pub content_hash_nullable: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct NewIndexedFile {
    pub scan_root_id: i64,
    pub absolute_path: String,
    pub normalized_path: String,
    pub file_name: String,
    pub extension: String,
    pub size_bytes: i64,
    pub modified_at: Option<String>,
    pub created_at_if_available: Option<String>,
    pub content_hash_nullable: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    pub root_id: i64,
    pub total_scanned: usize,
    pub added: usize,
    pub modified: usize,
    pub missing: usize,
    pub moved: usize,
    pub unchanged: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub app_name: String,
    pub version: String,
    pub app_data_dir: String,
    pub db_path: String,
    pub os: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSummary {
    pub total_roots: usize,
    pub total_files: usize,
    pub total_size_bytes: i64,
    pub export_timestamp: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryExport {
    pub app: String,
    pub version: String,
    pub exported_at: String,
    pub summary: ExportSummary,
    pub scan_roots: Vec<ScanRoot>,
    pub files: Vec<IndexedFile>,
}
