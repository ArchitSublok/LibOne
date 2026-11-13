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
    pub file_id_nullable: Option<String>,
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
    pub file_id_nullable: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedFiles {
    pub files: Vec<IndexedFile>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileQueryParams {
    pub query: Option<String>,
    pub root_id: Option<i64>,
    pub extension: Option<String>,
    pub status: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDetail {
    pub file: IndexedFile,
    pub root_display_name: String,
    pub root_absolute_path: String,
    pub exists_on_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherRootStatus {
    pub root_id: i64,
    pub path: String,
    pub active: bool,
    pub last_event_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatcherOverallStatus {
    pub is_watching: bool,
    pub watched_roots_count: usize,
    pub is_coalescing: bool,
    pub is_refreshing: bool,
    pub last_refreshed_at: Option<String>,
    pub root_statuses: Vec<WatcherRootStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanOptions {
    /// Whether to compute SHA-256 hashes during the scan. Default is false (only on demand).
    #[serde(default)]
    pub compute_hashes: bool,
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

/// Non-fatal warning emitted during scanning (e.g., locked file, symlink skipped, unreadable directory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanWarning {
    pub path: String,
    pub message: String,
    pub code: String,
}

/// Live incremental progress emitted to the frontend during scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub root_id: i64,
    pub current_root: String,
    pub current_directory: String,
    pub directories_visited: usize,
    pub files_discovered: usize,
    pub files_indexed: usize,
    pub warnings: Vec<ScanWarning>,
    pub state: String, // "idle", "running", "completed", "cancelled", "error"
    pub stats: Option<ScanStats>,
    pub error_message: Option<String>,
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
