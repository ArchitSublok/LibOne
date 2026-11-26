// All field names here must exactly match the serde-serialised Rust structs.

export interface ScanRoot {
  id: number;
  /** Canonical, absolute path as stored (e.g. C:\Users\Alice\Documents). */
  absolute_path: string;
  /** User-visible name shown in the sidebar. */
  display_name: string;
  /** Whether LibOne is currently tracking this root. */
  enabled: boolean;
  created_at: string;
  last_scan_started_at: string | null;
  last_scan_completed_at: string | null;
  /** "never_scanned" | "running" | "completed" | "cancelled" | "error" */
  last_scan_status: string;
  /** Count of non-missing indexed files. */
  file_count: number;
}

export interface IndexedFile {
  id: number;
  scan_root_id: number;
  absolute_path: string;
  normalized_path: string;
  file_name: string;
  extension: string;
  size_bytes: number;
  modified_at: string | null;
  created_at_if_available: string | null;
  discovered_at: string;
  last_seen_at: string;
  content_hash_nullable: string | null;
  file_id_nullable: string | null;
  /** "indexed" | "changed" | "missing" | "inaccessible" | "error" */
  status: string;
}

export interface Collection {
  id: number;
  name: string;
  description_nullable: string | null;
  color_nullable: string | null;
  created_at: string;
  updated_at: string;
}

export interface Tag {
  id: number;
  name: string;
  color_nullable: string | null;
  created_at: string;
}

export interface FileCollectionLink {
  file_id: number;
  collection_id: number;
  created_at: string;
}

export interface FileTagLink {
  file_id: number;
  tag_id: number;
  created_at: string;
}

export interface FileNote {
  id: number;
  file_id: number;
  note_text: string;
  created_at: string;
  updated_at: string;
}

export interface PaginatedFiles {
  files: IndexedFile[];
  total: number;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface FileQueryParams {
  query?: string | null;
  root_id?: number | null;
  extension?: string | null;
  status?: string | null;
  collection_id?: number | null;
  tag_ids?: number[] | null;
  has_notes?: boolean | null;
  sort_by?: "name" | "path" | "extension" | "size" | "modified" | "status" | null;
  sort_order?: "asc" | "desc" | null;
  page?: number | null;
  page_size?: number | null;
}

export interface FileDetail {
  file: IndexedFile;
  root_display_name: string;
  root_absolute_path: string;
  exists_on_disk: boolean;
}

export interface WatcherRootStatus {
  root_id: number;
  path: string;
  active: boolean;
  last_event_at: string | null;
  error_message: string | null;
}

export interface WatcherOverallStatus {
  is_watching: boolean;
  watched_roots_count: number;
  is_coalescing: boolean;
  is_refreshing: boolean;
  last_refreshed_at: string | null;
  root_statuses: WatcherRootStatus[];
}

export interface ScanOptions {
  compute_hashes: boolean;
}

export interface ScanStats {
  root_id: number;
  total_scanned: number;
  added: number;
  modified: number;
  missing: number;
  moved: number;
  unchanged: number;
  duration_ms: number;
}

export interface ScanWarning {
  path: string;
  message: string;
  code: string;
}

export interface ScanProgress {
  root_id: number;
  current_root: string;
  current_directory: string;
  directories_visited: number;
  files_discovered: number;
  files_indexed: number;
  warnings: ScanWarning[];
  state: "idle" | "running" | "completed" | "cancelled" | "error";
  stats: ScanStats | null;
  error_message: string | null;
}

export interface AppInfo {
  app_name: string;
  version: string;
  app_data_dir: string;
  db_path: string;
  os: string;
}

export interface ExportSummary {
  total_roots: number;
  total_files: number;
  total_size_bytes: number;
  export_timestamp: string;
}

export interface SuggestionFileLink {
  suggestion_id: number;
  file_id: number;
}

export interface DuplicateGroupFileLink {
  group_id: number;
  file_id: number;
}

export interface LibraryExport {
  format_version: string;
  app: string;
  version: string;
  exported_at: string;
  summary: ExportSummary;
  scan_roots: ScanRoot[];
  files: IndexedFile[];
  collections: Collection[];
  tags: Tag[];
  file_collections: FileCollectionLink[];
  file_tags: FileTagLink[];
  file_notes: FileNote[];
  organization_suggestions: OrganizationSuggestion[];
  suggestion_files: SuggestionFileLink[];
  duplicate_groups: DuplicateGroup[];
  duplicate_group_files: DuplicateGroupFileLink[];
}

// ── Phase 3: Suggestions & Duplicates ────────────────────────────────────────

export interface OrganizationSuggestion {
  id: number;
  suggestion_type: "duplicate_files" | "likely_version" | "suggest_collection" | "suggest_tag" | "stale_file" | string;
  title: string;
  rationale: string;
  confidence: number;
  status: "pending" | "approved" | "rejected" | "dismissed" | string;
  created_at: string;
  reviewed_at: string | null;
  affected_file_ids: number[];
  suggested_tag_id: number | null;
  suggested_tag_name: string | null;
  suggested_collection_id: number | null;
  suggested_collection_name: string | null;
}

export interface DuplicateGroup {
  id: number;
  match_type: "file_identity" | "exact_hash" | "name_and_size" | string;
  confidence: number;
  created_at: string;
  status: "pending" | "approved" | "rejected" | "dismissed" | string;
  file_ids: number[];
}

export interface FileComparisonItem {
  file: IndexedFile;
  root_display_name: string;
  root_enabled: boolean;
  exists_on_disk: boolean;
  is_canonical: boolean;
}

export interface DuplicateGroupDetail {
  group: DuplicateGroup;
  files: FileComparisonItem[];
}

export interface BulkSuggestionResult {
  approved_ids: number[];
  rejected_ids: number[];
  failed_ids: number[];
  errors: string[];
}

export interface LocalAiStatus {
  is_available: boolean;
  provider_name: string;
  status_message: string;
  model_name: string | null;
  allows_network: boolean;
}

// ── Phase 4: Backup, Restore & Integrity Diagnostics ────────────────────────

export interface BackupSummary {
  schema_version: number;
  total_roots: number;
  total_files: number;
  total_collections: number;
  total_tags: number;
  total_notes: number;
  total_suggestions: number;
  total_duplicate_groups: number;
}

export interface BackupResult {
  backup_path: string;
  file_size_bytes: number;
  sha256_hash: string;
  created_at: string;
  summary: BackupSummary;
}

export interface BackupValidationResult {
  is_valid: boolean;
  schema_version: number;
  summary: BackupSummary;
  file_size_bytes: number;
  sha256_hash: string;
  warnings: string[];
}

export interface RestoreResult {
  success: boolean;
  restored_schema_version: number;
  safety_backup_path: string;
  summary: BackupSummary;
}

export interface ForeignKeyViolation {
  table_name: string;
  rowid: number;
  referenced_table: string;
  foreign_key_index: number;
}

export interface IntegrityIssue {
  severity: "info" | "warning" | "error";
  category: "sqlite" | "schema" | "foreign_key" | "paths" | "missing_files";
  message: string;
  repair_guidance: string;
}

export interface DatabaseIntegrityReport {
  is_healthy: boolean;
  schema_version: number;
  integrity_check_passed: boolean;
  foreign_key_check_passed: boolean;
  missing_tables: string[];
  foreign_key_violations: ForeignKeyViolation[];
  invalid_paths_count: number;
  missing_files_count: number;
  total_roots: number;
  total_files: number;
  total_collections: number;
  total_tags: number;
  total_notes: number;
  issues: IntegrityIssue[];
}

export interface DiskSpaceInfo {
  available_bytes: number;
  total_bytes: number;
  free_percentage: number;
  is_low_space: boolean;
  warning_message: string | null;
}

export interface BackupHealthInfo {
  safety_backups_count: number;
  latest_safety_backup_time: string | null;
  latest_safety_backup_path: string | null;
  status: string;
}

export interface ScanHealthInfo {
  last_scan_started_at: string | null;
  last_scan_completed_at: string | null;
  last_scan_status: string | null;
  enabled_roots_count: number;
  disabled_roots_count: number;
  total_roots_count: number;
}

export interface FileCountsHealthInfo {
  indexed_files_count: number;
  missing_files_count: number;
  inaccessible_files_count: number;
  error_files_count: number;
  total_tracked_files: number;
}

export interface DiagnosticsReport {
  app_version: string;
  schema_version: number;
  db_path: string;
  app_data_dir: string;
  os: string;
  scan_health: ScanHealthInfo;
  file_counts: FileCountsHealthInfo;
  watcher_health: WatcherOverallStatus;
  backup_health: BackupHealthInfo;
  integrity_report: DatabaseIntegrityReport;
  disk_space: DiskSpaceInfo | null;
  generated_at: string;
}

