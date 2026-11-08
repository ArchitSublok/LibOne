export interface ScanRoot {
  id: number;
  path: string;
  label: string;
  added_at: string;
  last_scanned_at: string | null;
  file_count: number;
  status: "ready" | "scanning" | "error" | string;
}

export interface IndexedFile {
  id: number;
  scan_root_id: number;
  path: string;
  filename: string;
  extension: string;
  size_bytes: number;
  modified_at_ms: number;
  sha256_prefix: string | null;
  status: "active" | "modified" | "missing" | "moved" | string;
  first_seen_at: string;
  last_seen_at: string;
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
