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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description_nullable: Option<String>,
    pub color_nullable: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color_nullable: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCollectionLink {
    pub file_id: i64,
    pub collection_id: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTagLink {
    pub file_id: i64,
    pub tag_id: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNote {
    pub id: i64,
    pub file_id: i64,
    pub note_text: String,
    pub created_at: String,
    pub updated_at: String,
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
    pub collection_id: Option<i64>,
    pub tag_ids: Option<Vec<i64>>,
    pub has_notes: Option<bool>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatcherRootStatus {
    pub root_id: i64,
    pub path: String,
    pub active: bool,
    pub last_event_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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

/// Current portable metadata export format version.
pub const EXPORT_FORMAT_VERSION: &str = "1.2.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuggestionFileLink {
    pub suggestion_id: i64,
    pub file_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DuplicateGroupFileLink {
    pub group_id: i64,
    pub file_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryExport {
    pub format_version: String,
    pub app: String,
    pub version: String,
    pub exported_at: String,
    pub summary: ExportSummary,
    pub scan_roots: Vec<ScanRoot>,
    pub files: Vec<IndexedFile>,
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub file_collections: Vec<FileCollectionLink>,
    #[serde(default)]
    pub file_tags: Vec<FileTagLink>,
    #[serde(default)]
    pub file_notes: Vec<FileNote>,
    #[serde(default)]
    pub organization_suggestions: Vec<OrganizationSuggestion>,
    #[serde(default)]
    pub suggestion_files: Vec<SuggestionFileLink>,
    #[serde(default)]
    pub duplicate_groups: Vec<DuplicateGroup>,
    #[serde(default)]
    pub duplicate_group_files: Vec<DuplicateGroupFileLink>,
}

// ── Phase 3: Suggestions & Duplicates ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrganizationSuggestion {
    pub id: i64,
    pub suggestion_type: String,
    pub title: String,
    pub rationale: String,
    pub confidence: f64,
    pub status: String,
    pub created_at: String,
    pub reviewed_at: Option<String>,
    pub affected_file_ids: Vec<i64>,
    pub suggested_tag_id: Option<i64>,
    pub suggested_tag_name: Option<String>,
    pub suggested_collection_id: Option<i64>,
    pub suggested_collection_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DuplicateGroup {
    pub id: i64,
    pub match_type: String,
    pub confidence: f64,
    pub created_at: String,
    pub status: String,
    pub file_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileComparisonItem {
    pub file: IndexedFile,
    pub root_display_name: String,
    pub root_enabled: bool,
    pub exists_on_disk: bool,
    pub is_canonical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DuplicateGroupDetail {
    pub group: DuplicateGroup,
    pub files: Vec<FileComparisonItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BulkSuggestionResult {
    pub approved_ids: Vec<i64>,
    pub rejected_ids: Vec<i64>,
    pub failed_ids: Vec<i64>,
    pub errors: Vec<String>,
}

// ── Phase 4: Backup, Restore & Integrity Diagnostics ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupSummary {
    pub schema_version: i64,
    pub total_roots: usize,
    pub total_files: usize,
    pub total_collections: usize,
    pub total_tags: usize,
    pub total_notes: usize,
    pub total_suggestions: usize,
    pub total_duplicate_groups: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupResult {
    pub backup_path: String,
    pub file_size_bytes: u64,
    pub sha256_hash: String,
    pub created_at: String,
    pub summary: BackupSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupValidationResult {
    pub is_valid: bool,
    pub schema_version: i64,
    pub summary: BackupSummary,
    pub file_size_bytes: u64,
    pub sha256_hash: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestoreResult {
    pub success: bool,
    pub restored_schema_version: i64,
    pub safety_backup_path: String,
    pub summary: BackupSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForeignKeyViolation {
    pub table_name: String,
    pub rowid: i64,
    pub referenced_table: String,
    pub foreign_key_index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntegrityIssue {
    pub severity: String, // "info" | "warning" | "error"
    pub category: String, // "sqlite" | "schema" | "foreign_key" | "paths" | "missing_files"
    pub message: String,
    pub repair_guidance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseIntegrityReport {
    pub is_healthy: bool,
    pub schema_version: i64,
    pub integrity_check_passed: bool,
    pub foreign_key_check_passed: bool,
    pub missing_tables: Vec<String>,
    pub foreign_key_violations: Vec<ForeignKeyViolation>,
    pub invalid_paths_count: usize,
    pub missing_files_count: usize,
    pub total_roots: usize,
    pub total_files: usize,
    pub total_collections: usize,
    pub total_tags: usize,
    pub total_notes: usize,
    pub issues: Vec<IntegrityIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskSpaceInfo {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub free_percentage: f64,
    pub is_low_space: bool,
    pub warning_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupHealthInfo {
    pub safety_backups_count: usize,
    pub latest_safety_backup_time: Option<String>,
    pub latest_safety_backup_path: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanHealthInfo {
    pub last_scan_started_at: Option<String>,
    pub last_scan_completed_at: Option<String>,
    pub last_scan_status: Option<String>,
    pub enabled_roots_count: usize,
    pub disabled_roots_count: usize,
    pub total_roots_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileCountsHealthInfo {
    pub indexed_files_count: usize,
    pub missing_files_count: usize,
    pub inaccessible_files_count: usize,
    pub error_files_count: usize,
    pub total_tracked_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub schema_version: i64,
    pub db_path: String,
    pub app_data_dir: String,
    pub os: String,
    pub scan_health: ScanHealthInfo,
    pub file_counts: FileCountsHealthInfo,
    pub watcher_health: WatcherOverallStatus,
    pub backup_health: BackupHealthInfo,
    pub integrity_report: DatabaseIntegrityReport,
    pub disk_space: Option<DiskSpaceInfo>,
    pub generated_at: String,
}

/// Sanitizes a filesystem path by replacing personal user home folder names
/// with `<redacted>` for privacy-safe logs and diagnostics.
pub fn redact_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }

    // Windows paths: C:\Users\<Username>\...
    let lower = path.to_lowercase();
    if let Some(idx) = lower.find(":\\users\\") {
        let prefix_end = idx + 8; // length of ":\\Users\\"
        let rest = &path[prefix_end..];
        if let Some(slash_idx) = rest.find('\\') {
            let after_user = &rest[slash_idx..];
            return format!("{}<redacted>{}", &path[..prefix_end], after_user);
        } else {
            return format!("{}<redacted>", &path[..prefix_end]);
        }
    }

    // Unix paths: /home/<username>/... or /Users/<username>/...
    for prefix in &["/home/", "/Users/", "/users/"] {
        if let Some(idx) = path.find(prefix) {
            let prefix_end = idx + prefix.len();
            let rest = &path[prefix_end..];
            if let Some(slash_idx) = rest.find('/') {
                let after_user = &rest[slash_idx..];
                return format!("{}<redacted>{}", &path[..prefix_end], after_user);
            } else {
                return format!("{}<redacted>", &path[..prefix_end]);
            }
        }
    }

    path.to_string()
}

/// Produces a copy of the DiagnosticsReport with user-identifying filesystem paths sanitized.
pub fn redact_diagnostics_report(report: &DiagnosticsReport) -> DiagnosticsReport {
    let mut redacted = report.clone();
    redacted.db_path = redact_path(&report.db_path);
    redacted.app_data_dir = redact_path(&report.app_data_dir);
    if let Some(safety_path) = &report.backup_health.latest_safety_backup_path {
        redacted.backup_health.latest_safety_backup_path = Some(redact_path(safety_path));
    }
    redacted.watcher_health.root_statuses = report
        .watcher_health
        .root_statuses
        .iter()
        .map(|r| {
            let mut s = r.clone();
            s.path = redact_path(&r.path);
            s
        })
        .collect();
    redacted
}
