import React, { useState, useEffect, useMemo } from "react";
import {
  FolderPlus,
  RefreshCw,
  Download,
  ExternalLink,
  Trash2,
  Folder,
  FileText,
  Search,
  CheckCircle2,
  AlertTriangle,
  MoveRight,
  ShieldCheck,
  HardDrive,
  Copy,
  Check,
  X,
} from "lucide-react";
import {
  AppInfo,
  IndexedFile,
  LibraryExport,
  ScanRoot,
  ScanStats,
} from "./types";
import {
  addScanRoot,
  chooseFolder,
  exportLibraryMetadata,
  getAppInfo,
  listIndexedFiles,
  listScanRoots,
  openFileInDefaultApp,
  promptSaveExportPath,
  removeScanRoot,
  scanAllRoots,
  scanRoot,
} from "./services/api";
import "./App.css";

export const App: React.FC = () => {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [roots, setRoots] = useState<ScanRoot[]>([]);
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null);
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [isScanning, setIsScanning] = useState(false);
  const [scanMessage, setScanMessage] = useState<string | null>(null);
  const [lastStats, setLastStats] = useState<ScanStats[] | null>(null);

  // Export modal state
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportData, setExportData] = useState<LibraryExport | null>(null);
  const [copiedExport, setCopiedExport] = useState(false);
  const [exportStatusText, setExportStatusText] = useState<string | null>(null);

  // Initialize
  useEffect(() => {
    loadAppInfo();
    loadRoots();
  }, []);

  // Reload files when root selection, query, or filter changes
  useEffect(() => {
    loadFiles();
  }, [selectedRootId, searchQuery, statusFilter]);

  const loadAppInfo = async () => {
    try {
      const info = await getAppInfo();
      setAppInfo(info);
    } catch (e) {
      console.error("Failed to load app info", e);
    }
  };

  const loadRoots = async () => {
    try {
      const list = await listScanRoots();
      setRoots(list);
    } catch (e) {
      console.error("Failed to list scan roots", e);
    }
  };

  const loadFiles = async () => {
    try {
      const list = await listIndexedFiles({
        query: searchQuery,
        rootId: selectedRootId || undefined,
        statusFilter: statusFilter !== "all" ? statusFilter : undefined,
      });
      setFiles(list);
    } catch (e) {
      console.error("Failed to list files", e);
    }
  };

  const handleAddFolder = async () => {
    try {
      const folderPath = await chooseFolder();
      if (!folderPath) return;

      setScanMessage(`Adding and indexing ${folderPath}...`);
      const newRoot = await addScanRoot(folderPath);
      await loadRoots();
      setSelectedRootId(newRoot.id);

      // Immediately trigger scan on the new folder
      setIsScanning(true);
      const stats = await scanRoot(newRoot.id);
      setLastStats([stats]);
      setScanMessage(
        `Indexed ${stats.total_scanned} files (${stats.added} added, ${stats.modified} modified, ${stats.moved} moved)`
      );
      await loadRoots();
      await loadFiles();
    } catch (e: any) {
      alert(`Error adding folder: ${e?.message || e}`);
    } finally {
      setIsScanning(false);
    }
  };

  const handleRemoveRoot = async (e: React.MouseEvent, rootId: number) => {
    e.stopPropagation();
    if (!window.confirm("Remove this folder from LibOne index? Original files on disk will NOT be touched.")) {
      return;
    }
    try {
      await removeScanRoot(rootId);
      if (selectedRootId === rootId) {
        setSelectedRootId(null);
      }
      await loadRoots();
      await loadFiles();
    } catch (e: any) {
      alert(`Failed to remove folder: ${e?.message || e}`);
    }
  };

  const handleScanRoot = async (e: React.MouseEvent, rootId: number) => {
    e.stopPropagation();
    setIsScanning(true);
    setScanMessage("Scanning folder...");
    try {
      const stats = await scanRoot(rootId);
      setLastStats([stats]);
      setScanMessage(
        `Scan complete: ${stats.total_scanned} files scanned (${stats.added} new, ${stats.modified} modified, ${stats.moved} moved, ${stats.missing} missing)`
      );
      await loadRoots();
      await loadFiles();
    } catch (e: any) {
      alert(`Scan failed: ${e?.message || e}`);
    } finally {
      setIsScanning(false);
    }
  };

  const handleScanAll = async () => {
    if (roots.length === 0) {
      alert("No folders have been added yet. Click 'Add Folder' to select a directory.");
      return;
    }
    setIsScanning(true);
    setScanMessage("Scanning all folders in place...");
    try {
      const statsList = await scanAllRoots();
      setLastStats(statsList);
      const totalScanned = statsList.reduce((acc, s) => acc + s.total_scanned, 0);
      const totalAdded = statsList.reduce((acc, s) => acc + s.added, 0);
      const totalModified = statsList.reduce((acc, s) => acc + s.modified, 0);
      const totalMoved = statsList.reduce((acc, s) => acc + s.moved, 0);
      const totalMissing = statsList.reduce((acc, s) => acc + s.missing, 0);

      setScanMessage(
        `Full scan complete: ${totalScanned} files (${totalAdded} added, ${totalModified} modified, ${totalMoved} moved, ${totalMissing} missing)`
      );
      await loadRoots();
      await loadFiles();
    } catch (e: any) {
      alert(`Scan all failed: ${e?.message || e}`);
    } finally {
      setIsScanning(false);
    }
  };

  const handleOpenFile = async (filePath: string) => {
    try {
      await openFileInDefaultApp(filePath);
    } catch (e: any) {
      alert(`Could not open file: ${e?.message || e}`);
    }
  };

  const handleOpenExportModal = async () => {
    try {
      const data = await exportLibraryMetadata();
      setExportData(data);
      setCopiedExport(false);
      setExportStatusText(null);
      setExportModalOpen(true);
    } catch (e: any) {
      alert(`Export error: ${e?.message || e}`);
    }
  };

  const handleSaveExportToFile = async () => {
    try {
      const savePath = await promptSaveExportPath();
      if (!savePath) return;

      await exportLibraryMetadata(savePath);
      setExportStatusText(`Metadata successfully saved to: ${savePath}`);
    } catch (e: any) {
      alert(`Failed to save export file: ${e?.message || e}`);
    }
  };

  const handleCopyExportJson = () => {
    if (!exportData) return;
    navigator.clipboard.writeText(JSON.stringify(exportData, null, 2));
    setCopiedExport(true);
    setTimeout(() => setCopiedExport(false), 2500);
  };

  // Helper formatting
  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  const formatDate = (ms: number): string => {
    if (!ms) return "Unknown";
    return new Date(ms).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const getExtensionClass = (ext: string): string => {
    const e = ext.toLowerCase();
    if (e === "pdf") return "ext-pdf";
    if (e === "docx" || e === "doc") return "ext-docx";
    if (e === "xlsx" || e === "xls" || e === "csv") return "ext-xlsx";
    if (e === "txt" || e === "md") return "ext-txt";
    return "";
  };

  const totalFilesCount = useMemo(() => {
    return roots.reduce((acc, r) => acc + (r.file_count || 0), 0);
  }, [roots]);

  return (
    <div className="app-container">
      {/* Top Header */}
      <header className="app-header">
        <div className="brand-section">
          <div className="logo-badge">L1</div>
          <div>
            <div className="brand-title">LibOne</div>
            <div className="brand-subtitle">Private, local-first document library</div>
          </div>
          <span className="safety-pill">
            <ShieldCheck size={14} /> Local-first • In-place indexing • Safe by default
          </span>
        </div>

        <div className="header-actions">
          <button
            className="btn btn-secondary"
            onClick={handleScanAll}
            disabled={isScanning}
            title="Scan all included folders for additions, modifications, moves, and missing files"
          >
            <RefreshCw size={15} className={isScanning ? "spin" : ""} />
            {isScanning ? "Scanning..." : "Scan All"}
          </button>

          <button
            className="btn btn-secondary"
            onClick={handleOpenExportModal}
            title="Export library metadata to JSON"
          >
            <Download size={15} />
            Export JSON
          </button>

          <button
            className="btn btn-primary"
            onClick={handleAddFolder}
            title="Choose folder to index in LibOne"
          >
            <FolderPlus size={15} />
            Add Folder
          </button>
        </div>
      </header>

      {/* Main Body */}
      <div className="app-main">
        {/* Left Sidebar: Scan Roots */}
        <aside className="sidebar">
          <div className="sidebar-header">
            <span className="sidebar-title">Scan Roots ({roots.length})</span>
            <button
              className="btn btn-secondary"
              style={{ padding: "4px 8px", fontSize: "0.75rem" }}
              onClick={handleAddFolder}
            >
              + Add
            </button>
          </div>

          <div className="roots-list">
            <div
              className={`root-card ${selectedRootId === null ? "active" : ""}`}
              onClick={() => setSelectedRootId(null)}
            >
              <div className="root-top">
                <span className="root-label">All Documents</span>
                <span className="badge badge-active">{totalFilesCount} files</span>
              </div>
              <div className="root-path">Aggregate across all roots</div>
            </div>

            {roots.map((root) => (
              <div
                key={root.id}
                className={`root-card ${selectedRootId === root.id ? "active" : ""}`}
                onClick={() => setSelectedRootId(root.id)}
              >
                <div className="root-top">
                  <span className="root-label">{root.label}</span>
                  <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
                    <button
                      className="btn btn-danger-ghost"
                      onClick={(e) => handleScanRoot(e, root.id)}
                      title="Scan this folder"
                      disabled={isScanning}
                    >
                      <RefreshCw size={12} />
                    </button>
                    <button
                      className="btn btn-danger-ghost"
                      onClick={(e) => handleRemoveRoot(e, root.id)}
                      title="Remove folder from index"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                </div>
                <div className="root-path" title={root.path}>
                  {root.path}
                </div>
                <div className="root-meta">
                  <span>{root.file_count} files</span>
                  <span>
                    {root.last_scanned_at
                      ? `Scanned ${new Date(root.last_scanned_at).toLocaleTimeString()}`
                      : "Not scanned yet"}
                  </span>
                </div>
              </div>
            ))}

            {roots.length === 0 && (
              <div className="empty-state" style={{ padding: "24px 12px" }}>
                <Folder size={32} className="empty-state-icon" />
                <p style={{ fontSize: "0.82rem" }}>No folders added yet.</p>
                <button className="btn btn-primary" onClick={handleAddFolder}>
                  Add your first folder
                </button>
              </div>
            )}
          </div>

          {/* Sidebar Footer with Local Data Location */}
          <div className="sidebar-footer">
            <div className="app-meta-box">
              <div style={{ display: "flex", alignItems: "center", gap: 5, marginBottom: 3 }}>
                <HardDrive size={12} color="#60a5fa" />
                <strong style={{ color: "#cbd5e1" }}>Local Data Location</strong>
              </div>
              <div>{appInfo?.db_path || "App Data: %APPDATA%\\com.libone.app\\libone.db"}</div>
            </div>
          </div>
        </aside>

        {/* Content Area: Files */}
        <main className="content-area">
          {/* Content Toolbar */}
          <div className="content-toolbar">
            <div className="search-bar-row">
              <div className="search-input-wrapper">
                <Search size={16} className="search-icon" />
                <input
                  type="text"
                  className="search-input"
                  placeholder="Search file names, extensions, or folder paths..."
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
              </div>
            </div>

            <div className="filters-row">
              <div className="status-filter-pills">
                <button
                  className={`filter-pill ${statusFilter === "all" ? "active" : ""}`}
                  onClick={() => setStatusFilter("all")}
                >
                  All Statuses
                </button>
                <button
                  className={`filter-pill ${statusFilter === "active" ? "active" : ""}`}
                  onClick={() => setStatusFilter("active")}
                >
                  Active
                </button>
                <button
                  className={`filter-pill ${statusFilter === "modified" ? "active" : ""}`}
                  onClick={() => setStatusFilter("modified")}
                >
                  Modified
                </button>
                <button
                  className={`filter-pill ${statusFilter === "moved" ? "active" : ""}`}
                  onClick={() => setStatusFilter("moved")}
                >
                  Moved
                </button>
                <button
                  className={`filter-pill ${statusFilter === "missing" ? "active" : ""}`}
                  onClick={() => setStatusFilter("missing")}
                >
                  Missing
                </button>
              </div>

              <div className="result-count-label">
                Showing {files.length} indexed document{files.length === 1 ? "" : "s"}
              </div>
            </div>
          </div>

          {/* Change Detection Notification Banner */}
          {scanMessage && (
            <div className="stats-banner">
              <div className="stats-badges-group">
                <CheckCircle2 size={15} color="#10b981" />
                <span>{scanMessage}</span>
                {lastStats && lastStats.length > 0 && (
                  <span style={{ fontSize: "0.72rem", color: "var(--text-muted)", marginLeft: 6 }}>
                    ({lastStats.reduce((a, s) => a + s.duration_ms, 0)}ms elapsed)
                  </span>
                )}
              </div>
              <button
                className="btn btn-secondary"
                style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                onClick={() => {
                  setScanMessage(null);
                  setLastStats(null);
                }}
              >
                Dismiss
              </button>
            </div>
          )}

          {/* File List Table */}
          <div className="file-table-container">
            {files.length > 0 ? (
              <table className="file-table">
                <thead>
                  <tr>
                    <th>Document</th>
                    <th>Status</th>
                    <th>Size</th>
                    <th>Modified Date</th>
                    <th>Action</th>
                  </tr>
                </thead>
                <tbody>
                  {files.map((file) => (
                    <tr key={file.id} className="file-row">
                      <td className="file-cell">
                        <div className="file-name-cell">
                          <span className={`file-ext-badge ${getExtensionClass(file.extension)}`}>
                            {file.extension || "FILE"}
                          </span>
                          <div>
                            <div>{file.filename}</div>
                            <div className="path-subtext" title={file.path}>
                              {file.path}
                            </div>
                          </div>
                        </div>
                      </td>
                      <td className="file-cell">
                        {file.status === "active" && (
                          <span className="badge badge-active">Active</span>
                        )}
                        {file.status === "modified" && (
                          <span className="badge badge-modified" title="File modified on disk">
                            <AlertTriangle size={12} /> Modified
                          </span>
                        )}
                        {file.status === "moved" && (
                          <span className="badge badge-moved" title="Identified file move">
                            <MoveRight size={12} /> Moved
                          </span>
                        )}
                        {file.status === "missing" && (
                          <span className="badge badge-missing" title="No longer found at original path">
                            Missing
                          </span>
                        )}
                      </td>
                      <td className="file-cell" style={{ color: "var(--text-secondary)" }}>
                        {formatBytes(file.size_bytes)}
                      </td>
                      <td className="file-cell" style={{ color: "var(--text-secondary)" }}>
                        {formatDate(file.modified_at_ms)}
                      </td>
                      <td className="file-cell">
                        <button
                          className="btn btn-secondary"
                          style={{ padding: "5px 10px", fontSize: "0.76rem" }}
                          onClick={() => handleOpenFile(file.path)}
                          disabled={file.status === "missing"}
                          title="Open original file in default Windows application"
                        >
                          <ExternalLink size={13} />
                          Open
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="empty-state">
                <FileText className="empty-state-icon" />
                <h3>No documents found</h3>
                <p>
                  {searchQuery || statusFilter !== "all"
                    ? "Try adjusting your search query or status filter."
                    : roots.length === 0
                    ? "Add a folder to begin indexing documents."
                    : "This folder contains no files or has not been scanned yet."}
                </p>
                {roots.length === 0 && (
                  <button className="btn btn-primary" onClick={handleAddFolder}>
                    <FolderPlus size={15} /> Add Folder
                  </button>
                )}
              </div>
            )}
          </div>
        </main>
      </div>

      {/* Export Modal */}
      {exportModalOpen && exportData && (
        <div className="modal-overlay" onClick={() => setExportModalOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Download size={18} color="#38bdf8" />
                <h3 style={{ fontSize: "1.05rem" }}>Safe Library Metadata Export</h3>
              </div>
              <button
                className="btn btn-danger-ghost"
                onClick={() => setExportModalOpen(false)}
              >
                <X size={18} />
              </button>
            </div>

            <div className="modal-body">
              <p style={{ fontSize: "0.84rem", color: "var(--text-secondary)" }}>
                LibOne guarantees your library metadata remains 100% portable. You can export all scan roots, file records, and timestamps to a standard JSON format.
              </p>

              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 1fr 1fr",
                  gap: 12,
                  background: "var(--bg-card)",
                  padding: 12,
                  borderRadius: 8,
                  fontSize: "0.82rem",
                }}
              >
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Roots</div>
                  <strong style={{ fontSize: "1.1rem" }}>{exportData.summary.total_roots}</strong>
                </div>
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Files</div>
                  <strong style={{ fontSize: "1.1rem" }}>{exportData.summary.total_files}</strong>
                </div>
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Size</div>
                  <strong style={{ fontSize: "1.1rem" }}>
                    {formatBytes(exportData.summary.total_size_bytes)}
                  </strong>
                </div>
              </div>

              {exportStatusText && (
                <div style={{ color: "#34d399", fontSize: "0.82rem", fontWeight: 500 }}>
                  ✓ {exportStatusText}
                </div>
              )}

              <div>
                <div style={{ fontSize: "0.78rem", color: "var(--text-muted)", marginBottom: 6 }}>
                  JSON Preview:
                </div>
                <div className="json-preview">
                  {JSON.stringify(exportData, null, 2).slice(0, 3000)}
                  {JSON.stringify(exportData, null, 2).length > 3000 ? "\n... (truncated preview)" : ""}
                </div>
              </div>
            </div>

            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={handleCopyExportJson}
              >
                {copiedExport ? <Check size={14} color="#34d399" /> : <Copy size={14} />}
                {copiedExport ? "Copied JSON" : "Copy to Clipboard"}
              </button>

              <button
                className="btn btn-primary"
                onClick={handleSaveExportToFile}
              >
                <Download size={14} />
                Save to File (.json)
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
