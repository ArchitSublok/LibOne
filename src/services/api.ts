import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  AppInfo,
  Collection,
  FileCollectionLink,
  FileDetail,
  FileNote,
  FileQueryParams,
  FileTagLink,
  IndexedFile,
  LibraryExport,
  PaginatedFiles,
  ScanOptions,
  ScanProgress,
  ScanRoot,
  ScanStats,
  Tag,
  WatcherOverallStatus,
} from "../types";

// ── Environment detection ────────────────────────────────────────────────────

export const isTauriEnvironment = (): boolean =>
  typeof window !== "undefined" &&
  ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);

// ── Mock state for browser dev-preview ───────────────────────────────────────

let mockRoots: ScanRoot[] = [
  {
    id: 1,
    absolute_path: "C:\\Users\\User\\Documents",
    display_name: "Documents",
    enabled: true,
    created_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_scan_started_at: new Date(Date.now() - 3700000).toISOString(),
    last_scan_completed_at: new Date(Date.now() - 3600000).toISOString(),
    last_scan_status: "completed",
    file_count: 3,
  },
  {
    id: 2,
    absolute_path: "C:\\Users\\User\\Downloads",
    display_name: "Downloads",
    enabled: false,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    last_scan_started_at: null,
    last_scan_completed_at: null,
    last_scan_status: "never_scanned",
    file_count: 2,
  },
];

let mockFiles: IndexedFile[] = [
  {
    id: 1,
    scan_root_id: 1,
    absolute_path: "C:\\Users\\User\\Documents\\Tax_Return_2025.pdf",
    normalized_path: "c:\\users\\user\\documents\\tax_return_2025.pdf",
    file_name: "Tax_Return_2025.pdf",
    extension: "pdf",
    size_bytes: 1420500,
    modified_at: new Date(Date.now() - 86400000 * 10).toISOString(),
    created_at_if_available: null,
    discovered_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
    content_hash_nullable: "e3b0c44298fc1c14",
    file_id_nullable: null,
    status: "indexed",
  },
  {
    id: 2,
    scan_root_id: 1,
    absolute_path: "C:\\Users\\User\\Documents\\Home_Insurance_Policy.pdf",
    normalized_path: "c:\\users\\user\\documents\\home_insurance_policy.pdf",
    file_name: "Home_Insurance_Policy.pdf",
    extension: "pdf",
    size_bytes: 852000,
    modified_at: new Date(Date.now() - 86400000 * 4).toISOString(),
    created_at_if_available: null,
    discovered_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
    content_hash_nullable: "a591a6d40bf42040",
    file_id_nullable: null,
    status: "indexed",
  },
  {
    id: 3,
    scan_root_id: 1,
    absolute_path: "C:\\Users\\User\\Documents\\Medical_Record_Summary.docx",
    normalized_path: "c:\\users\\user\\documents\\medical_record_summary.docx",
    file_name: "Medical_Record_Summary.docx",
    extension: "docx",
    size_bytes: 340100,
    modified_at: new Date(Date.now() - 86400000 * 1).toISOString(),
    created_at_if_available: null,
    discovered_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
    content_hash_nullable: null,
    file_id_nullable: null,
    status: "modified",
  },
  {
    id: 4,
    scan_root_id: 2,
    absolute_path: "C:\\Users\\User\\Downloads\\Q3_Utility_Bill.pdf",
    normalized_path: "c:\\users\\user\\downloads\\q3_utility_bill.pdf",
    file_name: "Q3_Utility_Bill.pdf",
    extension: "pdf",
    size_bytes: 420900,
    modified_at: new Date(Date.now() - 86400000 * 3).toISOString(),
    created_at_if_available: null,
    discovered_at: new Date(Date.now() - 86400000).toISOString(),
    last_seen_at: new Date().toISOString(),
    content_hash_nullable: "7d793037a0760186",
    file_id_nullable: null,
    status: "moved",
  },
  {
    id: 5,
    scan_root_id: 2,
    absolute_path: "C:\\Users\\User\\Downloads\\Contract_Draft_v1.docx",
    normalized_path: "c:\\users\\user\\downloads\\contract_draft_v1.docx",
    file_name: "Contract_Draft_v1.docx",
    extension: "docx",
    size_bytes: 182000,
    modified_at: new Date(Date.now() - 86400000 * 5).toISOString(),
    created_at_if_available: null,
    discovered_at: new Date(Date.now() - 86400000).toISOString(),
    last_seen_at: new Date(Date.now() - 86400000).toISOString(),
    content_hash_nullable: null,
    file_id_nullable: null,
    status: "missing",
  },
];

let mockIdCounter = 100;

let mockCollections: Collection[] = [
  {
    id: 1,
    name: "Taxes & Finance",
    description_nullable: "Financial documents and tax filings",
    color_nullable: "#10b981",
    created_at: new Date(Date.now() - 86400000 * 5).toISOString(),
    updated_at: new Date(Date.now() - 86400000 * 5).toISOString(),
  },
  {
    id: 2,
    name: "Legal",
    description_nullable: "Contracts and legal agreements",
    color_nullable: "#6366f1",
    created_at: new Date(Date.now() - 86400000 * 3).toISOString(),
    updated_at: new Date(Date.now() - 86400000 * 3).toISOString(),
  },
];

let mockTags: Tag[] = [
  {
    id: 1,
    name: "important",
    color_nullable: "#ef4444",
    created_at: new Date(Date.now() - 86400000 * 5).toISOString(),
  },
  {
    id: 2,
    name: "review",
    color_nullable: "#f59e0b",
    created_at: new Date(Date.now() - 86400000 * 4).toISOString(),
  },
  {
    id: 3,
    name: "archive",
    color_nullable: "#6b7280",
    created_at: new Date(Date.now() - 86400000 * 2).toISOString(),
  },
];

let mockFileCollections: FileCollectionLink[] = [
  {
    file_id: 1,
    collection_id: 1,
    created_at: new Date(Date.now() - 86400000 * 2).toISOString(),
  },
];

let mockFileTags: FileTagLink[] = [
  {
    file_id: 1,
    tag_id: 1,
    created_at: new Date(Date.now() - 86400000 * 2).toISOString(),
  },
];

let mockNotes: FileNote[] = [
  {
    id: 1,
    file_id: 1,
    note_text: "Final submitted copy for fiscal year 2025.",
    created_at: new Date(Date.now() - 86400000 * 1).toISOString(),
    updated_at: new Date(Date.now() - 86400000 * 1).toISOString(),
  },
];

// ── API functions ─────────────────────────────────────────────────────────────

export async function getAppInfo(): Promise<AppInfo> {
  if (isTauriEnvironment()) {
    return await invoke<AppInfo>("get_app_info");
  }
  return {
    app_name: "LibOne (Dev Preview)",
    version: "0.1.0",
    app_data_dir: "C:\\Users\\User\\AppData\\Roaming\\com.libone.app",
    db_path: "C:\\Users\\User\\AppData\\Roaming\\com.libone.app\\libone.db",
    os: "windows",
  };
}

/** Open the native OS folder-picker dialog and return one or more selected paths. */
export async function chooseFolders(): Promise<string[]> {
  if (isTauriEnvironment()) {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: true,
        title: "Select folder(s) to include in LibOne",
      });
      if (Array.isArray(selected)) {
        return selected.filter((p): p is string => typeof p === "string");
      }
      if (typeof selected === "string") {
        return [selected];
      }
      return [];
    } catch (err) {
      console.warn("Folder dialog error:", err);
      const manual = window.prompt(
        "Enter the absolute folder path to add (e.g. C:\\Users\\Name\\Documents):"
      );
      return manual ? [manual] : [];
    }
  }
  const manual = window.prompt(
    "Enter folder path to include (e.g. C:\\Users\\User\\Documents):"
  );
  return manual ? [manual] : [];
}

/** Open the native OS folder-picker dialog for a single selection. */
export async function chooseFolder(): Promise<string | null> {
  const folders = await chooseFolders();
  return folders.length > 0 ? folders[0] : null;
}

/**
 * Ask the backend to validate and persist a new scan root.
 */
export async function addScanRoot(
  path: string,
  displayName?: string
): Promise<ScanRoot> {
  if (isTauriEnvironment()) {
    return await invoke<ScanRoot>("add_scan_root", {
      path,
      displayName: displayName ?? null,
    });
  }

  const existing = mockRoots.find(
    (r) => r.absolute_path.toLowerCase() === path.toLowerCase()
  );
  if (existing && existing.enabled) {
    throw new Error(
      "A scan root already exists for this folder. Remove the existing root first."
    );
  }
  if (existing && !existing.enabled) {
    existing.enabled = true;
    if (displayName) existing.display_name = displayName;
    return { ...existing };
  }

  mockIdCounter++;
  const newRoot: ScanRoot = {
    id: mockIdCounter,
    absolute_path: path,
    display_name: displayName || path.split("\\").pop() || path,
    enabled: true,
    created_at: new Date().toISOString(),
    last_scan_started_at: null,
    last_scan_completed_at: null,
    last_scan_status: "never_scanned",
    file_count: 0,
  };
  mockRoots = [newRoot, ...mockRoots];
  return newRoot;
}

/** Soft-remove: disables the root in LibOne. Original files are untouched. */
export async function removeScanRoot(rootId: number): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("remove_scan_root", { rootId });
    return;
  }
  const root = mockRoots.find((r) => r.id === rootId);
  if (root) root.enabled = false;
}

/** Re-enable a previously disabled scan root. */
export async function enableScanRoot(
  rootId: number,
  displayName?: string
): Promise<ScanRoot> {
  if (isTauriEnvironment()) {
    return await invoke<ScanRoot>("enable_scan_root", {
      rootId,
      displayName: displayName ?? null,
    });
  }
  const root = mockRoots.find((r) => r.id === rootId);
  if (!root) throw new Error(`Scan root ${rootId} not found`);
  root.enabled = true;
  if (displayName) root.display_name = displayName;
  return { ...root };
}

export async function listScanRoots(): Promise<ScanRoot[]> {
  if (isTauriEnvironment()) {
    return await invoke<ScanRoot[]>("list_scan_roots");
  }
  return [...mockRoots];
}

// ── Real-time incremental scanning API ────────────────────────────────────────

/** Start scanning in background with progress events. */
export async function startScan(
  rootId: number,
  options?: ScanOptions
): Promise<ScanProgress> {
  if (isTauriEnvironment()) {
    return await invoke<ScanProgress>("start_scan", {
      rootId,
      options: options ?? null,
    });
  }

  const root = mockRoots.find((r) => r.id === rootId);
  return {
    root_id: rootId,
    current_root: root?.absolute_path || "C:\\Documents",
    current_directory: root?.absolute_path || "C:\\Documents",
    directories_visited: 2,
    files_discovered: 5,
    files_indexed: 5,
    warnings: [
      {
        path: `${root?.absolute_path || "C:\\Documents"}\\.git`,
        message: "Symbolic link or excluded directory skipped",
        code: "symlink_skipped",
      },
    ],
    state: "completed",
    stats: {
      root_id: rootId,
      total_scanned: 5,
      added: 0,
      modified: 1,
      missing: 0,
      moved: 0,
      unchanged: 4,
      duration_ms: 85,
    },
    error_message: null,
  };
}

/** Request safe cancellation of an active scan. */
export async function cancelScan(rootId: number): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("cancel_scan", { rootId });
  }
}

/** Retrieve the current or last progress report for a root. */
export async function getScanProgress(rootId: number): Promise<ScanProgress | null> {
  if (isTauriEnvironment()) {
    return await invoke<ScanProgress | null>("get_scan_progress", { rootId });
  }
  return null;
}

/** Subscribe to live `scan-progress` events from the Tauri backend. */
export async function listenScanProgress(
  callback: (progress: ScanProgress) => void
): Promise<UnlistenFn> {
  if (isTauriEnvironment()) {
    return await listen<ScanProgress>("scan-progress", (event) => {
      callback(event.payload);
    });
  }
  return () => {};
}

/** Synchronous scan fallback / legacy helper. */
export async function scanRoot(rootId: number): Promise<ScanStats> {
  if (isTauriEnvironment()) {
    return await invoke<ScanStats>("scan_root", { rootId });
  }
  const root = mockRoots.find((r) => r.id === rootId);
  if (root) {
    root.last_scan_completed_at = new Date().toISOString();
    root.last_scan_status = "completed";
  }
  return {
    root_id: rootId,
    total_scanned: 5,
    added: 0,
    modified: 1,
    missing: 1,
    moved: 0,
    unchanged: 3,
    duration_ms: 45,
  };
}

export async function scanAllRoots(): Promise<ScanStats[]> {
  if (isTauriEnvironment()) {
    return await invoke<ScanStats[]>("scan_all_roots");
  }
  return mockRoots
    .filter((r) => r.enabled)
    .map((r) => ({
      root_id: r.id,
      total_scanned: 5,
      added: 0,
      modified: 1,
      missing: 1,
      moved: 0,
      unchanged: 3,
      duration_ms: 32,
    }));
}

export async function listIndexedFiles(options?: {
  query?: string;
  rootId?: number;
  statusFilter?: string;
  limit?: number;
  offset?: number;
}): Promise<IndexedFile[]> {
  if (isTauriEnvironment()) {
    return await invoke<IndexedFile[]>("list_indexed_files", {
      query: options?.query ?? null,
      rootId: options?.rootId ?? null,
      statusFilter: options?.statusFilter ?? null,
      limit: options?.limit ?? null,
      offset: options?.offset ?? null,
    });
  }

  let result = [...mockFiles];
  if (options?.rootId) result = result.filter((f) => f.scan_root_id === options.rootId);
  if (options?.statusFilter && options.statusFilter !== "all")
    result = result.filter((f) => f.status === options.statusFilter);
  if (options?.query) {
    const q = options.query.toLowerCase();
    result = result.filter(
      (f) =>
        f.file_name.toLowerCase().includes(q) ||
        f.absolute_path.toLowerCase().includes(q) ||
        f.extension.toLowerCase().includes(q)
    );
  }
  return result;
}

export async function queryIndexedFiles(
  params: FileQueryParams
): Promise<PaginatedFiles> {
  if (isTauriEnvironment()) {
    return await invoke<PaginatedFiles>("query_indexed_files", { params });
  }

  // Mock implementation for browser dev preview
  let result = [...mockFiles];
  if (params.root_id) result = result.filter((f) => f.scan_root_id === params.root_id);
  if (params.status && params.status !== "all")
    result = result.filter((f) => f.status === params.status);
  if (params.extension && params.extension !== "all")
    result = result.filter((f) => f.extension.toLowerCase() === params.extension?.toLowerCase());
  if (params.query) {
    const q = params.query.toLowerCase();
    result = result.filter(
      (f) =>
        f.file_name.toLowerCase().includes(q) ||
        f.absolute_path.toLowerCase().includes(q) ||
        f.extension.toLowerCase().includes(q)
    );
  }
  if (params.collection_id) {
    const fileIdsInCol = new Set(
      mockFileCollections
        .filter((fc) => fc.collection_id === params.collection_id)
        .map((fc) => fc.file_id)
    );
    result = result.filter((f) => fileIdsInCol.has(f.id));
  }
  if (params.tag_ids && params.tag_ids.length > 0) {
    for (const tagId of params.tag_ids) {
      const fileIdsWithTag = new Set(
        mockFileTags
          .filter((ft) => ft.tag_id === tagId)
          .map((ft) => ft.file_id)
      );
      result = result.filter((f) => fileIdsWithTag.has(f.id));
    }
  }
  if (params.has_notes !== undefined && params.has_notes !== null) {
    const filesWithNotes = new Set(mockNotes.map((n) => n.file_id));
    if (params.has_notes) {
      result = result.filter((f) => filesWithNotes.has(f.id));
    } else {
      result = result.filter((f) => !filesWithNotes.has(f.id));
    }
  }

  // Sorting
  const sortBy = params.sort_by || "name";
  const sortOrder = params.sort_order || "asc";
  result.sort((a, b) => {
    let cmp = 0;
    if (sortBy === "name") cmp = a.file_name.localeCompare(b.file_name);
    else if (sortBy === "path") cmp = a.normalized_path.localeCompare(b.normalized_path);
    else if (sortBy === "extension") cmp = a.extension.localeCompare(b.extension);
    else if (sortBy === "size") cmp = a.size_bytes - b.size_bytes;
    else if (sortBy === "modified") cmp = (a.modified_at || "").localeCompare(b.modified_at || "");
    else if (sortBy === "status") cmp = a.status.localeCompare(b.status);
    return sortOrder === "desc" ? -cmp : cmp;
  });

  const total = result.length;
  const page = Math.max(1, params.page || 1);
  const pageSize = Math.min(200, Math.max(1, params.page_size || 25));
  const offset = (page - 1) * pageSize;
  const files = result.slice(offset, offset + pageSize);
  const totalPages = total === 0 ? 1 : Math.ceil(total / pageSize);

  return {
    files,
    total,
    page,
    page_size: pageSize,
    total_pages: totalPages,
  };
}

export async function getIndexedFileDetail(fileId: number): Promise<FileDetail> {
  if (isTauriEnvironment()) {
    return await invoke<FileDetail>("get_indexed_file_detail", { fileId });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found.`);
  const root = mockRoots.find((r) => r.id === file.scan_root_id);
  return {
    file,
    root_display_name: root?.display_name || "Unknown Root",
    root_absolute_path: root?.absolute_path || "C:\\",
    exists_on_disk: file.status !== "missing",
  };
}

export async function getLibraryExtensions(): Promise<string[]> {
  if (isTauriEnvironment()) {
    return await invoke<string[]>("get_library_extensions");
  }
  const set = new Set(mockFiles.map((f) => f.extension).filter(Boolean));
  return Array.from(set).sort();
}

/** Open original file using Windows registered default application. */
export async function openOriginalFile(fileId: number): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("open_original_file", { fileId });
    return;
  }
  const file = mockFiles.find((f) => f.id === fileId);
  alert(`[Dev Mode] Would open original file #${fileId} with default app:\n${file?.absolute_path}`);
}

/** Reveal original file in Windows File Explorer. */
export async function showInFileExplorer(fileId: number): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("show_in_file_explorer", { fileId });
    return;
  }
  const file = mockFiles.find((f) => f.id === fileId);
  alert(`[Dev Mode] Would reveal file in File Explorer:\n${file?.absolute_path}`);
}

export async function openFileInDefaultApp(filePath: string): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("open_file_in_default_app", { filePath });
    return;
  }
  alert(`[Dev Mode] Would open with Windows default app:\n${filePath}`);
}

export async function exportLibraryMetadata(
  targetFilePath?: string
): Promise<LibraryExport> {
  if (isTauriEnvironment()) {
    return await invoke<LibraryExport>("export_library_metadata", {
      targetFilePath: targetFilePath ?? null,
    });
  }

  const totalSize = mockFiles.reduce((acc, f) => acc + f.size_bytes, 0);
  return {
    format_version: "1.0.0",
    app: "LibOne",
    version: "0.1.0",
    exported_at: new Date().toISOString(),
    summary: {
      total_roots: mockRoots.length,
      total_files: mockFiles.length,
      total_size_bytes: totalSize,
      export_timestamp: new Date().toISOString(),
    },
    scan_roots: [...mockRoots],
    files: [...mockFiles],
  };
}

export async function promptSaveExportPath(): Promise<string | null> {
  if (isTauriEnvironment()) {
    try {
      return await saveDialog({
        title: "Export LibOne Library Metadata",
        defaultPath: "libone-library-export.json",
        filters: [{ name: "JSON File", extensions: ["json"] }],
      });
    } catch (e) {
      console.warn("Save dialog failed:", e);
    }
  }
  return null;
}

/** Compute SHA-256 hash on demand for a single file using streaming I/O. */
export async function computeFileHash(fileId: number): Promise<string> {
  if (isTauriEnvironment()) {
    return await invoke<string>("compute_file_hash", { fileId });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  const hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
  if (file) {
    file.content_hash_nullable = hash;
  }
  return hash;
}

// ── Watcher API & Event Subscriptions ─────────────────────────────────────────

export async function getWatcherStatus(): Promise<WatcherOverallStatus> {
  if (isTauriEnvironment()) {
    return await invoke<WatcherOverallStatus>("get_watcher_status");
  }
  return {
    is_watching: true,
    watched_roots_count: mockRoots.filter((r) => r.enabled).length,
    is_coalescing: false,
    is_refreshing: false,
    last_refreshed_at: new Date().toISOString(),
    root_statuses: mockRoots.map((r) => ({
      root_id: r.id,
      path: r.absolute_path,
      active: r.enabled,
      last_event_at: null,
      error_message: null,
    })),
  };
}

export async function listenWatcherStatus(
  callback: (status: WatcherOverallStatus) => void
): Promise<UnlistenFn> {
  if (isTauriEnvironment()) {
    return await listen<WatcherOverallStatus>("watcher-status", (event) => {
      callback(event.payload);
    });
  }
  return () => {};
}

export async function listenLibraryUpdated(
  callback: () => void
): Promise<UnlistenFn> {
  if (isTauriEnvironment()) {
    return await listen("library-updated", () => {
      callback();
    });
  }
  return () => {};
}

// ── Phase 2: Collections API ─────────────────────────────────────────────────

export async function createCollection(
  name: string,
  description?: string | null,
  color?: string | null
): Promise<Collection> {
  if (isTauriEnvironment()) {
    return await invoke<Collection>("create_collection", {
      name,
      description: description ?? null,
      color: color ?? null,
    });
  }
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Collection name cannot be empty.");
  if (mockCollections.some((c) => c.name.toLowerCase() === trimmed.toLowerCase())) {
    throw new Error(`A collection with the name '${trimmed}' already exists.`);
  }
  const now = new Date().toISOString();
  const col: Collection = {
    id: ++mockIdCounter,
    name: trimmed,
    description_nullable: description?.trim() || null,
    color_nullable: color?.trim() || null,
    created_at: now,
    updated_at: now,
  };
  mockCollections.push(col);
  return col;
}

export async function getCollection(id: number): Promise<Collection> {
  if (isTauriEnvironment()) {
    return await invoke<Collection>("get_collection", { id });
  }
  const col = mockCollections.find((c) => c.id === id);
  if (!col) throw new Error(`Collection #${id} not found.`);
  return col;
}

export async function listCollections(): Promise<Collection[]> {
  if (isTauriEnvironment()) {
    return await invoke<Collection[]>("list_collections");
  }
  return [...mockCollections].sort((a, b) => a.name.localeCompare(b.name));
}

export async function updateCollection(
  id: number,
  name: string,
  description?: string | null,
  color?: string | null
): Promise<Collection> {
  if (isTauriEnvironment()) {
    return await invoke<Collection>("update_collection", {
      id,
      name,
      description: description ?? null,
      color: color ?? null,
    });
  }
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Collection name cannot be empty.");
  const col = mockCollections.find((c) => c.id === id);
  if (!col) throw new Error(`Collection #${id} not found.`);
  const duplicate = mockCollections.find(
    (c) => c.id !== id && c.name.toLowerCase() === trimmed.toLowerCase()
  );
  if (duplicate) {
    throw new Error(`A collection with the name '${trimmed}' already exists.`);
  }
  col.name = trimmed;
  col.description_nullable = description?.trim() || null;
  col.color_nullable = color?.trim() || null;
  col.updated_at = new Date().toISOString();
  return col;
}

export async function deleteCollection(id: number): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("delete_collection", { id });
  }
  const idx = mockCollections.findIndex((c) => c.id === id);
  if (idx === -1) throw new Error(`Collection #${id} not found.`);
  mockCollections.splice(idx, 1);
  mockFileCollections = mockFileCollections.filter((fc) => fc.collection_id !== id);
}

// ── Phase 2: Tags API ────────────────────────────────────────────────────────

export async function createTag(
  name: string,
  color?: string | null
): Promise<Tag> {
  if (isTauriEnvironment()) {
    return await invoke<Tag>("create_tag", {
      name,
      color: color ?? null,
    });
  }
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Tag name cannot be empty.");
  if (mockTags.some((t) => t.name.toLowerCase() === trimmed.toLowerCase())) {
    throw new Error(`A tag with the name '${trimmed}' already exists.`);
  }
  const tag: Tag = {
    id: ++mockIdCounter,
    name: trimmed,
    color_nullable: color?.trim() || null,
    created_at: new Date().toISOString(),
  };
  mockTags.push(tag);
  return tag;
}

export async function getTag(id: number): Promise<Tag> {
  if (isTauriEnvironment()) {
    return await invoke<Tag>("get_tag", { id });
  }
  const tag = mockTags.find((t) => t.id === id);
  if (!tag) throw new Error(`Tag #${id} not found.`);
  return tag;
}

export async function listTags(): Promise<Tag[]> {
  if (isTauriEnvironment()) {
    return await invoke<Tag[]>("list_tags");
  }
  return [...mockTags].sort((a, b) => a.name.localeCompare(b.name));
}

export async function updateTag(
  id: number,
  name: string,
  color?: string | null
): Promise<Tag> {
  if (isTauriEnvironment()) {
    return await invoke<Tag>("update_tag", {
      id,
      name,
      color: color ?? null,
    });
  }
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Tag name cannot be empty.");
  const tag = mockTags.find((t) => t.id === id);
  if (!tag) throw new Error(`Tag #${id} not found.`);
  const duplicate = mockTags.find(
    (t) => t.id !== id && t.name.toLowerCase() === trimmed.toLowerCase()
  );
  if (duplicate) {
    throw new Error(`A tag with the name '${trimmed}' already exists.`);
  }
  tag.name = trimmed;
  tag.color_nullable = color?.trim() || null;
  return tag;
}

export async function deleteTag(id: number): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("delete_tag", { id });
  }
  const idx = mockTags.findIndex((t) => t.id === id);
  if (idx === -1) throw new Error(`Tag #${id} not found.`);
  mockTags.splice(idx, 1);
  mockFileTags = mockFileTags.filter((ft) => ft.tag_id !== id);
}

// ── Phase 2: File-Collection Links API ───────────────────────────────────────

export async function addFileToCollection(
  fileId: number,
  collectionId: number
): Promise<FileCollectionLink> {
  if (isTauriEnvironment()) {
    return await invoke<FileCollectionLink>("add_file_to_collection", {
      fileId,
      collectionId,
    });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  const col = mockCollections.find((c) => c.id === collectionId);
  if (!col) throw new Error(`Collection #${collectionId} not found.`);
  const existing = mockFileCollections.find(
    (fc) => fc.file_id === fileId && fc.collection_id === collectionId
  );
  if (existing) return existing;
  const link: FileCollectionLink = {
    file_id: fileId,
    collection_id: collectionId,
    created_at: new Date().toISOString(),
  };
  mockFileCollections.push(link);
  return link;
}

export async function removeFileFromCollection(
  fileId: number,
  collectionId: number
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("remove_file_from_collection", {
      fileId,
      collectionId,
    });
  }
  mockFileCollections = mockFileCollections.filter(
    (fc) => !(fc.file_id === fileId && fc.collection_id === collectionId)
  );
}

export async function listFileCollections(fileId: number): Promise<Collection[]> {
  if (isTauriEnvironment()) {
    return await invoke<Collection[]>("list_file_collections", { fileId });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  const colIds = new Set(
    mockFileCollections
      .filter((fc) => fc.file_id === fileId)
      .map((fc) => fc.collection_id)
  );
  return mockCollections.filter((c) => colIds.has(c.id));
}

export async function bulkAddFileCollections(
  fileId: number,
  collectionIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("bulk_add_file_collections", {
      fileId,
      collectionIds,
    });
  }
  for (const cid of collectionIds) {
    await addFileToCollection(fileId, cid);
  }
}

export async function bulkRemoveFileCollections(
  fileId: number,
  collectionIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("bulk_remove_file_collections", {
      fileId,
      collectionIds,
    });
  }
  const removeSet = new Set(collectionIds);
  mockFileCollections = mockFileCollections.filter(
    (fc) => !(fc.file_id === fileId && removeSet.has(fc.collection_id))
  );
}

export async function setFileCollections(
  fileId: number,
  collectionIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("set_file_collections", {
      fileId,
      collectionIds,
    });
  }
  mockFileCollections = mockFileCollections.filter((fc) => fc.file_id !== fileId);
  for (const cid of collectionIds) {
    await addFileToCollection(fileId, cid);
  }
}

// ── Phase 2: File-Tag Links API ──────────────────────────────────────────────

export async function addFileToTag(
  fileId: number,
  tagId: number
): Promise<FileTagLink> {
  if (isTauriEnvironment()) {
    return await invoke<FileTagLink>("add_file_to_tag", {
      fileId,
      tagId,
    });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  const tag = mockTags.find((t) => t.id === tagId);
  if (!tag) throw new Error(`Tag #${tagId} not found.`);
  const existing = mockFileTags.find(
    (ft) => ft.file_id === fileId && ft.tag_id === tagId
  );
  if (existing) return existing;
  const link: FileTagLink = {
    file_id: fileId,
    tag_id: tagId,
    created_at: new Date().toISOString(),
  };
  mockFileTags.push(link);
  return link;
}

export async function removeFileFromTag(
  fileId: number,
  tagId: number
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("remove_file_from_tag", {
      fileId,
      tagId,
    });
  }
  mockFileTags = mockFileTags.filter(
    (ft) => !(ft.file_id === fileId && ft.tag_id === tagId)
  );
}

export async function listFileTags(fileId: number): Promise<Tag[]> {
  if (isTauriEnvironment()) {
    return await invoke<Tag[]>("list_file_tags", { fileId });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  const tagIds = new Set(
    mockFileTags
      .filter((ft) => ft.file_id === fileId)
      .map((ft) => ft.tag_id)
  );
  return mockTags.filter((t) => tagIds.has(t.id));
}

export async function bulkAddFileTags(
  fileId: number,
  tagIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("bulk_add_file_tags", {
      fileId,
      tagIds,
    });
  }
  for (const tid of tagIds) {
    await addFileToTag(fileId, tid);
  }
}

export async function bulkRemoveFileTags(
  fileId: number,
  tagIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("bulk_remove_file_tags", {
      fileId,
      tagIds,
    });
  }
  const removeSet = new Set(tagIds);
  mockFileTags = mockFileTags.filter(
    (ft) => !(ft.file_id === fileId && removeSet.has(ft.tag_id))
  );
}

export async function setFileTags(
  fileId: number,
  tagIds: number[]
): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("set_file_tags", {
      fileId,
      tagIds,
    });
  }
  mockFileTags = mockFileTags.filter((ft) => ft.file_id !== fileId);
  for (const tid of tagIds) {
    await addFileToTag(fileId, tid);
  }
}

// ── Phase 2: File Notes API ──────────────────────────────────────────────────

export async function createFileNote(
  fileId: number,
  noteText: string
): Promise<FileNote> {
  if (isTauriEnvironment()) {
    return await invoke<FileNote>("create_file_note", {
      fileId,
      noteText,
    });
  }
  const trimmed = noteText.trim();
  if (!trimmed) throw new Error("Note content cannot be empty.");
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  const now = new Date().toISOString();
  const note: FileNote = {
    id: ++mockIdCounter,
    file_id: fileId,
    note_text: trimmed,
    created_at: now,
    updated_at: now,
  };
  mockNotes.push(note);
  return note;
}

export async function getFileNote(noteId: number): Promise<FileNote> {
  if (isTauriEnvironment()) {
    return await invoke<FileNote>("get_file_note", { noteId });
  }
  const note = mockNotes.find((n) => n.id === noteId);
  if (!note) throw new Error(`Note #${noteId} not found.`);
  return note;
}

export async function listFileNotes(fileId: number): Promise<FileNote[]> {
  if (isTauriEnvironment()) {
    return await invoke<FileNote[]>("list_file_notes", { fileId });
  }
  const file = mockFiles.find((f) => f.id === fileId);
  if (!file) throw new Error(`Indexed file #${fileId} not found in library.`);
  return mockNotes
    .filter((n) => n.file_id === fileId)
    .sort((a, b) => b.created_at.localeCompare(a.created_at));
}

export async function updateFileNote(
  noteId: number,
  noteText: string
): Promise<FileNote> {
  if (isTauriEnvironment()) {
    return await invoke<FileNote>("update_file_note", {
      noteId,
      noteText,
    });
  }
  const trimmed = noteText.trim();
  if (!trimmed) throw new Error("Note content cannot be empty.");
  const note = mockNotes.find((n) => n.id === noteId);
  if (!note) throw new Error(`Note #${noteId} not found.`);
  note.note_text = trimmed;
  note.updated_at = new Date().toISOString();
  return note;
}

export async function deleteFileNote(noteId: number): Promise<void> {
  if (isTauriEnvironment()) {
    return await invoke<void>("delete_file_note", { noteId });
  }
  const idx = mockNotes.findIndex((n) => n.id === noteId);
  if (idx === -1) throw new Error(`Note #${noteId} not found.`);
  mockNotes.splice(idx, 1);
}


