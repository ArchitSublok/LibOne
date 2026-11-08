use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRoot {
    pub id: i64,
    pub path: String,
    pub label: String,
    pub added_at: String,
    pub last_scanned_at: Option<String>,
    pub file_count: i64,
    pub status: String, // "ready", "scanning", "error"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedFile {
    pub id: i64,
    pub scan_root_id: i64,
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: i64,
    pub modified_at_ms: i64,
    pub sha256_prefix: Option<String>,
    pub status: String, // "active", "modified", "missing", "moved"
    pub first_seen_at: String,
    pub last_seen_at: String,
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
