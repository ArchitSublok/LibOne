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

export interface LibraryExport {
  app: string;
  version: string;
  exported_at: string;
  summary: ExportSummary;
  scan_roots: ScanRoot[];
  files: IndexedFile[];
}
