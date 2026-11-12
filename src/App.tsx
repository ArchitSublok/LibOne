import React, { useState, useEffect, useMemo } from "react";
import {
  FolderPlus,
  RefreshCw,
  Download,
  ExternalLink,
  FileText,
  Search,
  CheckCircle2,
  AlertTriangle,
  MoveRight,
  ShieldCheck,
  Copy,
  Check,
  X,
  Scan,
} from "lucide-react";
import { AppInfo, IndexedFile, LibraryExport, ScanRoot } from "./types";
import {
  exportLibraryMetadata,
  getAppInfo,
  listIndexedFiles,
  listScanRoots,
  openFileInDefaultApp,
  promptSaveExportPath,
  scanAllRoots,
} from "./services/api";
import { ScanRootsPanel } from "./components/ScanRootsPanel";
import { ScanScreen } from "./components/ScanScreen";
import "./App.css";

export const App: React.FC = () => {
  // ── Core state ────────────────────────────────────────────────────────────
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [roots, setRoots] = useState<ScanRoot[]>([]);
  const [rootsLoading, setRootsLoading] = useState(true);
  const [rootsError, setRootsError] = useState<string | null>(null);

  const [selectedRootId, setSelectedRootId] = useState<number | null>(null);
  const [files, setFiles] = useState<IndexedFile[]>([]);
  const [filesLoading, setFilesLoading] = useState(false);

  const [searchQuery, setSearchQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<string>("all");

  const [isScanning, setIsScanning] = useState(false);
  const [scanBanner, setScanBanner] = useState<{ message: string; durationMs?: number } | null>(null);
  const [scanScreenOpen, setScanScreenOpen] = useState(false);
  const [scanScreenRootId, setScanScreenRootId] = useState<number | null>(null);

  // ── Export modal ──────────────────────────────────────────────────────────
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportData, setExportData] = useState<LibraryExport | null>(null);
  const [copiedExport, setCopiedExport] = useState(false);
  const [exportStatusText, setExportStatusText] = useState<string | null>(null);

  // ── Bootstrap ─────────────────────────────────────────────────────────────
  useEffect(() => {
    getAppInfo()
      .then(setAppInfo)
      .catch((e) => console.error("Failed to load app info", e));
    loadRoots();
  }, []);

  useEffect(() => {
    loadFiles();
  }, [selectedRootId, searchQuery, statusFilter]);

  // ── Data loaders ──────────────────────────────────────────────────────────

  const loadRoots = async () => {
    setRootsLoading(true);
    setRootsError(null);
    try {
      const list = await listScanRoots();
      setRoots(list);
    } catch (e: any) {
      setRootsError(e?.message ?? String(e));
    } finally {
      setRootsLoading(false);
    }
  };

  const loadFiles = async () => {
    setFilesLoading(true);
    try {
      const list = await listIndexedFiles({
        query: searchQuery,
        rootId: selectedRootId ?? undefined,
        statusFilter: statusFilter !== "all" ? statusFilter : undefined,
      });
      setFiles(list);
    } catch (e) {
      console.error("Failed to list files", e);
    } finally {
      setFilesLoading(false);
    }
  };

  // ── Scan all ──────────────────────────────────────────────────────────────

  const handleScanAll = async () => {
    const enabled = roots.filter((r) => r.enabled);
    if (enabled.length === 0) {
      setScanBanner({ message: "No enabled folders to scan. Add a folder first." });
      return;
    }
    setIsScanning(true);
    setScanBanner({ message: "Scanning all enabled folders…" });
    try {
      const statsList = await scanAllRoots();
      const totalScanned = statsList.reduce((a, s) => a + s.total_scanned, 0);
      const totalAdded   = statsList.reduce((a, s) => a + s.added, 0);
      const totalMod     = statsList.reduce((a, s) => a + s.modified, 0);
      const totalMoved   = statsList.reduce((a, s) => a + s.moved, 0);
      const totalMissing = statsList.reduce((a, s) => a + s.missing, 0);
      const totalMs      = statsList.reduce((a, s) => a + s.duration_ms, 0);
      setScanBanner({
        message: `Scan complete — ${totalScanned} files (${totalAdded} added, ${totalMod} modified, ${totalMoved} moved, ${totalMissing} missing)`,
        durationMs: totalMs,
      });
      await loadRoots();
      await loadFiles();
    } catch (e: any) {
      setScanBanner({ message: `Scan all failed: ${e?.message ?? e}` });
    } finally {
      setIsScanning(false);
    }
  };

  // ── File open ─────────────────────────────────────────────────────────────

  const handleOpenFile = async (filePath: string) => {
    try {
      await openFileInDefaultApp(filePath);
    } catch (e: any) {
      alert(`Could not open file: ${e?.message ?? e}`);
    }
  };

  // ── Export ────────────────────────────────────────────────────────────────

  const handleOpenExportModal = async () => {
    try {
      const data = await exportLibraryMetadata();
      setExportData(data);
      setCopiedExport(false);
      setExportStatusText(null);
      setExportModalOpen(true);
    } catch (e: any) {
      alert(`Export error: ${e?.message ?? e}`);
    }
  };

  const handleSaveExportToFile = async () => {
    try {
      const savePath = await promptSaveExportPath();
      if (!savePath) return;
      await exportLibraryMetadata(savePath);
      setExportStatusText(`Saved to: ${savePath}`);
    } catch (e: any) {
      alert(`Failed to save export file: ${e?.message ?? e}`);
    }
  };

  const handleCopyExportJson = () => {
    if (!exportData) return;
    navigator.clipboard.writeText(JSON.stringify(exportData, null, 2));
    setCopiedExport(true);
    setTimeout(() => setCopiedExport(false), 2500);
  };

  // ── Formatting helpers ────────────────────────────────────────────────────

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
  };

  const formatDate = (iso: string | null): string => {
    if (!iso) return "—";
    try {
      return new Date(iso).toLocaleDateString(undefined, {
        year: "numeric", month: "short", day: "numeric",
        hour: "2-digit", minute: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  const getExtensionClass = (ext: string): string => {
    const e = ext.toLowerCase();
    if (e === "pdf") return "ext-pdf";
    if (e === "docx" || e === "doc") return "ext-docx";
    if (e === "xlsx" || e === "xls" || e === "csv") return "ext-xlsx";
    if (e === "txt" || e === "md") return "ext-txt";
    return "";
  };

  // ── Derived values ────────────────────────────────────────────────────────

  const totalFilesCount = useMemo(
    () => roots.reduce((acc, r) => acc + (r.file_count ?? 0), 0),
    [roots]
  );

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="app-container">
      {/* ── Header ───────────────────────────────────────────────────────── */}
      <header className="app-header">
        <div className="brand-section">
          <div className="logo-badge">L1</div>
          <div>
            <div className="brand-title">LibOne</div>
            <div className="brand-subtitle">Private, local-first document library</div>
          </div>
          <span className="safety-pill">
            <ShieldCheck size={14} /> Local-first · In-place indexing · Safe by default
          </span>
        </div>

        <div className="header-actions">
          <button
            className={`btn ${scanScreenOpen ? "btn-primary" : "btn-secondary"}`}
            onClick={() => {
              setScanScreenRootId(selectedRootId);
              setScanScreenOpen(!scanScreenOpen);
            }}
            title="Open the real-time folder scanner screen"
          >
            <Scan size={15} />
            Scanner
          </button>

          <button
            className="btn btn-secondary"
            onClick={handleScanAll}
            disabled={isScanning}
            title="Scan all enabled folders for changes"
          >
            <RefreshCw size={15} className={isScanning ? "spin" : ""} />
            {isScanning ? "Scanning…" : "Scan All"}
          </button>

          <button
            className="btn btn-secondary"
            onClick={handleOpenExportModal}
            title="Export library metadata to JSON"
          >
            <Download size={15} /> Export JSON
          </button>
        </div>
      </header>

      {/* ── Main body ────────────────────────────────────────────────────── */}
      {scanScreenOpen ? (
        <ScanScreen
          roots={roots}
          initialRootId={scanScreenRootId ?? selectedRootId}
          onClose={() => setScanScreenOpen(false)}
          onScanCompleted={() => {
            loadRoots();
            loadFiles();
          }}
        />
      ) : (
        <div className="app-main">
          {/* ── Left sidebar ────────────────────────────────────────────────── */}
          {rootsError ? (
            <aside className="sidebar">
              <div className="sidebar-header">
                <span className="sidebar-title">Folders</span>
              </div>
              <div className="inline-error" style={{ margin: "12px" }} role="alert">
                <AlertTriangle size={13} />
                <span>Failed to load folders: {rootsError}</span>
                <button className="inline-error__dismiss" onClick={loadRoots}>Retry</button>
              </div>
            </aside>
          ) : (
            <ScanRootsPanel
              roots={roots}
              selectedRootId={selectedRootId}
              totalFileCount={totalFilesCount}
              appDataDbPath={appInfo?.db_path}
              isScanning={isScanning}
              onSelectRoot={setSelectedRootId}
              onRootsChanged={async () => { await loadRoots(); await loadFiles(); }}
              onScanComplete={(stats) => {
                setScanBanner({
                  message: `Scan complete — ${stats.total_scanned} files (${stats.added} added, ${stats.modified} modified, ${stats.moved} moved, ${stats.missing} missing)`,
                  durationMs: stats.duration_ms,
                });
              }}
              onScanningChange={setIsScanning}
              onOpenScanScreen={(id) => {
                setScanScreenRootId(id);
                setScanScreenOpen(true);
              }}
            />
          )}

        {/* ── Content area ────────────────────────────────────────────────── */}
        <main className="content-area">
          {/* Search + filters toolbar */}
          <div className="content-toolbar">
            <div className="search-bar-row">
              <div className="search-input-wrapper">
                <Search size={16} className="search-icon" />
                <input
                  type="text"
                  className="search-input"
                  placeholder="Search file names, extensions, or paths…"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                />
              </div>
            </div>

            <div className="filters-row">
              <div className="status-filter-pills">
                {(["all", "indexed", "modified", "moved", "missing"] as const).map((s) => (
                  <button
                    key={s}
                    className={`filter-pill ${statusFilter === s ? "active" : ""}`}
                    onClick={() => setStatusFilter(s)}
                  >
                    {s === "all" ? "All Statuses" : s.charAt(0).toUpperCase() + s.slice(1)}
                  </button>
                ))}
              </div>
              <div className="result-count-label">
                {filesLoading ? "Loading…" : `${files.length} document${files.length !== 1 ? "s" : ""}`}
              </div>
            </div>
          </div>

          {/* Scan result banner */}
          {scanBanner && (
            <div className="stats-banner">
              <div className="stats-badges-group">
                <CheckCircle2 size={15} color="#10b981" />
                <span>{scanBanner.message}</span>
                {scanBanner.durationMs != null && (
                  <span style={{ fontSize: "0.72rem", color: "var(--text-muted)", marginLeft: 6 }}>
                    ({scanBanner.durationMs}ms)
                  </span>
                )}
              </div>
              <button
                className="btn btn-secondary"
                style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                onClick={() => setScanBanner(null)}
              >
                Dismiss
              </button>
            </div>
          )}

          {/* File table */}
          <div className="file-table-container">
            {filesLoading ? (
              <div className="loading-state" aria-busy="true">
                <RefreshCw size={22} className="spin" style={{ color: "var(--text-muted)" }} />
                <p style={{ color: "var(--text-muted)", marginTop: 10 }}>Loading files…</p>
              </div>
            ) : files.length > 0 ? (
              <table className="file-table">
                <thead>
                  <tr>
                    <th>Document</th>
                    <th>Status</th>
                    <th>Size</th>
                    <th>Modified</th>
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
                            <div>{file.file_name}</div>
                            <div className="path-subtext" title={file.absolute_path}>
                              {file.absolute_path}
                            </div>
                          </div>
                        </div>
                      </td>
                      <td className="file-cell">
                        {file.status === "indexed" && (
                          <span className="badge badge-active">Indexed</span>
                        )}
                        {file.status === "modified" && (
                          <span className="badge badge-modified" title="Content modified on disk">
                            <AlertTriangle size={12} /> Modified
                          </span>
                        )}
                        {file.status === "moved" && (
                          <span className="badge badge-moved" title="File detected as moved">
                            <MoveRight size={12} /> Moved
                          </span>
                        )}
                        {file.status === "missing" && (
                          <span className="badge badge-missing" title="No longer at original path">
                            Missing
                          </span>
                        )}
                      </td>
                      <td className="file-cell" style={{ color: "var(--text-secondary)" }}>
                        {formatBytes(file.size_bytes)}
                      </td>
                      <td className="file-cell" style={{ color: "var(--text-secondary)" }}>
                        {formatDate(file.modified_at)}
                      </td>
                      <td className="file-cell">
                        <button
                          className="btn btn-secondary"
                          style={{ padding: "5px 10px", fontSize: "0.76rem" }}
                          onClick={() => handleOpenFile(file.absolute_path)}
                          disabled={file.status === "missing"}
                          title="Open in Windows default application"
                        >
                          <ExternalLink size={13} /> Open
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
                    ? "Add a folder to begin indexing your documents."
                    : "This folder has not been scanned yet, or contains no files."}
                </p>
                {roots.length === 0 && !rootsLoading && (
                  <button className="btn btn-primary" onClick={() => {}}>
                    <FolderPlus size={15} /> Add Folder
                  </button>
                )}
              </div>
            )}
          </div>
        </main>
      </div>
      )}

      {/* ── Export modal ─────────────────────────────────────────────────── */}
      {exportModalOpen && exportData && (
        <div className="modal-overlay" onClick={() => setExportModalOpen(false)}>
          <div className="modal-content" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Download size={18} color="#38bdf8" />
                <h3 style={{ fontSize: "1.05rem" }}>Safe Library Metadata Export</h3>
              </div>
              <button className="btn btn-danger-ghost" onClick={() => setExportModalOpen(false)}>
                <X size={18} />
              </button>
            </div>

            <div className="modal-body">
              <p style={{ fontSize: "0.84rem", color: "var(--text-secondary)" }}>
                LibOne exports only index metadata — file paths, sizes, and timestamps.
                Document contents are never stored or exported.
              </p>

              <div
                style={{
                  display: "grid", gridTemplateColumns: "1fr 1fr 1fr",
                  gap: 12, background: "var(--bg-card)", padding: 12,
                  borderRadius: 8, fontSize: "0.82rem",
                }}
              >
                <div>
                  <div style={{ color: "var(--text-muted)" }}>Total Folders</div>
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
                  {JSON.stringify(exportData, null, 2).length > 3000 ? "\n… (truncated preview)" : ""}
                </div>
              </div>
            </div>

            <div className="modal-footer">
              <button className="btn btn-secondary" onClick={handleCopyExportJson}>
                {copiedExport ? <Check size={14} color="#34d399" /> : <Copy size={14} />}
                {copiedExport ? "Copied JSON" : "Copy to Clipboard"}
              </button>
              <button className="btn btn-primary" onClick={handleSaveExportToFile}>
                <Download size={14} /> Save to File (.json)
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default App;
