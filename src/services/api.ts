import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { AppInfo, IndexedFile, LibraryExport, ScanRoot, ScanStats } from "../types";

export const isTauriEnvironment = (): boolean => {
  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
};

// Fallback in-memory state for development preview in a standard browser
let mockRoots: ScanRoot[] = [
  {
    id: 1,
    path: "C:\\Users\\User\\Documents",
    label: "Documents",
    added_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_scanned_at: new Date(Date.now() - 3600000).toISOString(),
    file_count: 3,
    status: "ready",
  },
  {
    id: 2,
    path: "C:\\Users\\User\\Downloads",
    label: "Downloads",
    added_at: new Date(Date.now() - 86400000).toISOString(),
    last_scanned_at: new Date(Date.now() - 1800000).toISOString(),
    file_count: 2,
    status: "ready",
  },
];

let mockFiles: IndexedFile[] = [
  {
    id: 1,
    scan_root_id: 1,
    path: "C:\\Users\\User\\Documents\\Tax_Return_2025.pdf",
    filename: "Tax_Return_2025.pdf",
    extension: "pdf",
    size_bytes: 1420500,
    modified_at_ms: Date.now() - 86400000 * 10,
    sha256_prefix: "e3b0c44298fc1c14",
    status: "active",
    first_seen_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
  },
  {
    id: 2,
    scan_root_id: 1,
    path: "C:\\Users\\User\\Documents\\Home_Insurance_Policy.pdf",
    filename: "Home_Insurance_Policy.pdf",
    extension: "pdf",
    size_bytes: 852000,
    modified_at_ms: Date.now() - 86400000 * 4,
    sha256_prefix: "a591a6d40bf42040",
    status: "active",
    first_seen_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
  },
  {
    id: 3,
    scan_root_id: 1,
    path: "C:\\Users\\User\\Documents\\Medical_Record_Summary.docx",
    filename: "Medical_Record_Summary.docx",
    extension: "docx",
    size_bytes: 340100,
    modified_at_ms: Date.now() - 86400000 * 1,
    sha256_prefix: "b10a8db164e07541",
    status: "modified",
    first_seen_at: new Date(Date.now() - 86400000 * 2).toISOString(),
    last_seen_at: new Date().toISOString(),
  },
  {
    id: 4,
    scan_root_id: 2,
    path: "C:\\Users\\User\\Downloads\\Q3_Utility_Bill.pdf",
    filename: "Q3_Utility_Bill.pdf",
    extension: "pdf",
    size_bytes: 420900,
    modified_at_ms: Date.now() - 86400000 * 3,
    sha256_prefix: "7d793037a0760186",
    status: "moved",
    first_seen_at: new Date(Date.now() - 86400000).toISOString(),
    last_seen_at: new Date().toISOString(),
  },
  {
    id: 5,
    scan_root_id: 2,
    path: "C:\\Users\\User\\Downloads\\Contract_Draft_v1.docx",
    filename: "Contract_Draft_v1.docx",
    extension: "docx",
    size_bytes: 182000,
    modified_at_ms: Date.now() - 86400000 * 5,
    sha256_prefix: "4b227777d4dd1fc6",
    status: "missing",
    first_seen_at: new Date(Date.now() - 86400000).toISOString(),
    last_seen_at: new Date(Date.now() - 86400000).toISOString(),
  },
];

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

export async function chooseFolder(): Promise<string | null> {
  if (isTauriEnvironment()) {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "Select folder to include in LibOne",
      });
      if (typeof selected === "string") return selected;
      if (Array.isArray(selected) && (selected as unknown[]).length > 0) {
        return String((selected as unknown[])[0]);
      }
      return null;
    } catch (err) {
      console.warn("Folder dialog error:", err);
      // Fallback prompt
      return window.prompt("Enter the absolute folder path to add (e.g. C:\\Users\\Name\\Documents):");
    }
  }
  return window.prompt("Enter folder path to include (e.g. C:\\Users\\User\\Documents):");
}

export async function addScanRoot(path: string, label?: string): Promise<ScanRoot> {
  if (isTauriEnvironment()) {
    return await invoke<ScanRoot>("add_scan_root", { path, label });
  }

  const existing = mockRoots.find((r) => r.path.toLowerCase() === path.toLowerCase());
  if (existing) return existing;

  const newRoot: ScanRoot = {
    id: Date.now(),
    path,
    label: label || path.split("\\").pop() || path,
    added_at: new Date().toISOString(),
    last_scanned_at: null,
    file_count: 0,
    status: "ready",
  };
  mockRoots = [newRoot, ...mockRoots];
  return newRoot;
}

export async function removeScanRoot(rootId: number): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("remove_scan_root", { rootId });
    return;
  }
  mockRoots = mockRoots.filter((r) => r.id !== rootId);
  mockFiles = mockFiles.filter((f) => f.scan_root_id !== rootId);
}

export async function listScanRoots(): Promise<ScanRoot[]> {
  if (isTauriEnvironment()) {
    return await invoke<ScanRoot[]>("list_scan_roots");
  }
  return [...mockRoots];
}

export async function scanRoot(rootId: number): Promise<ScanStats> {
  if (isTauriEnvironment()) {
    return await invoke<ScanStats>("scan_root", { rootId });
  }

  // Simulate mock scan
  const root = mockRoots.find((r) => r.id === rootId);
  if (root) {
    root.last_scanned_at = new Date().toISOString();
  }
  return {
    root_id: rootId,
    total_scanned: 5,
    added: 0,
    modified: 1,
    missing: 1,
    moved: 1,
    unchanged: 2,
    duration_ms: 45,
  };
}

export async function scanAllRoots(): Promise<ScanStats[]> {
  if (isTauriEnvironment()) {
    return await invoke<ScanStats[]>("scan_all_roots");
  }
  return mockRoots.map((r) => ({
    root_id: r.id,
    total_scanned: 5,
    added: 0,
    modified: 1,
    missing: 1,
    moved: 1,
    unchanged: 2,
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
      query: options?.query || null,
      rootId: options?.rootId || null,
      statusFilter: options?.statusFilter || null,
      limit: options?.limit || null,
      offset: options?.offset || null,
    });
  }

  let result = [...mockFiles];
  if (options?.rootId) {
    result = result.filter((f) => f.scan_root_id === options.rootId);
  }
  if (options?.statusFilter && options.statusFilter !== "all") {
    result = result.filter((f) => f.status === options.statusFilter);
  }
  if (options?.query) {
    const q = options.query.toLowerCase();
    result = result.filter(
      (f) =>
        f.filename.toLowerCase().includes(q) ||
        f.path.toLowerCase().includes(q) ||
        f.extension.toLowerCase().includes(q)
    );
  }
  return result;
}

export async function openFileInDefaultApp(filePath: string): Promise<void> {
  if (isTauriEnvironment()) {
    await invoke<void>("open_file_in_default_app", { filePath });
    return;
  }
  alert(`[Dev Mode] Opening file with Windows default application:\n${filePath}`);
}

export async function exportLibraryMetadata(targetFilePath?: string): Promise<LibraryExport> {
  if (isTauriEnvironment()) {
    return await invoke<LibraryExport>("export_library_metadata", {
      targetFilePath: targetFilePath || null,
    });
  }

  const totalSize = mockFiles.reduce((acc, f) => acc + f.size_bytes, 0);
  return {
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
      const selected = await saveDialog({
        title: "Export LibOne Library Metadata",
        defaultPath: "libone-library-export.json",
        filters: [
          {
            name: "JSON File",
            extensions: ["json"],
          },
        ],
      });
      return selected;
    } catch (e) {
      console.warn("Save dialog failed:", e);
    }
  }
  return null;
}
