import React, { useState, useEffect } from "react";
import {
  Play,
  Square,
  RefreshCw,
  CheckCircle2,
  AlertTriangle,
  Folder,
  FileCheck,
  FileSearch,
  Clock,
  ArrowLeft,
  ShieldAlert,
  Info,
} from "lucide-react";
import { ScanProgress, ScanRoot } from "../types";
import {
  cancelScan,
  getScanProgress,
  listenScanProgress,
  startScan,
} from "../services/api";

interface Props {
  roots: ScanRoot[];
  initialRootId?: number | null;
  onClose: () => void;
  onScanCompleted: () => void;
}

export const ScanScreen: React.FC<Props> = ({
  roots,
  initialRootId,
  onClose,
  onScanCompleted,
}) => {
  const enabledRoots = roots.filter((r) => r.enabled);

  const [selectedRootId, setSelectedRootId] = useState<number>(() => {
    if (initialRootId && enabledRoots.some((r) => r.id === initialRootId)) {
      return initialRootId;
    }
    return enabledRoots.length > 0 ? enabledRoots[0].id : 0;
  });

  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [warningFilter, setWarningFilter] = useState<string>("all");
  const [isStarting, setIsStarting] = useState<boolean>(false);
  const [generalError, setGeneralError] = useState<string | null>(null);
  const [computeHashes, setComputeHashes] = useState<boolean>(false);

  // Subscribe to live scan events
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listenScanProgress((newProgress) => {
      if (newProgress.root_id === selectedRootId) {
        setProgress(newProgress);
        if (newProgress.state === "completed") {
          onScanCompleted();
        }
      }
    }).then((fn) => {
      unlisten = fn;
    });

    // Check existing progress if any
    if (selectedRootId > 0) {
      getScanProgress(selectedRootId).then((existing) => {
        if (existing) {
          setProgress(existing);
        }
      });
    }

    return () => {
      if (unlisten) unlisten();
    };
  }, [selectedRootId]);

  const selectedRoot = enabledRoots.find((r) => r.id === selectedRootId);

  const isScanning = progress?.state === "running" || isStarting;

  const handleStartScan = async () => {
    if (!selectedRootId) return;
    setGeneralError(null);
    setIsStarting(true);
    try {
      const initial = await startScan(selectedRootId, { compute_hashes: computeHashes });
      setProgress(initial);
    } catch (err: any) {
      setGeneralError(err?.message || String(err));
    } finally {
      setIsStarting(false);
    }
  };

  const handleCancelScan = async () => {
    if (!selectedRootId) return;
    try {
      await cancelScan(selectedRootId);
    } catch (err: any) {
      console.error("Cancel scan failed:", err);
    }
  };

  // Warnings filtering
  const warnings = progress?.warnings || [];
  const filteredWarnings =
    warningFilter === "all"
      ? warnings
      : warnings.filter((w) => w.code === warningFilter);

  const warningCodes = Array.from(new Set(warnings.map((w) => w.code)));

  return (
    <div className="scan-screen-container">
      {/* Top Navigation Bar */}
      <div className="scan-header">
        <button className="btn btn-secondary" onClick={onClose}>
          <ArrowLeft size={15} /> Back to Library
        </button>
        <div className="scan-header-title">
          <h2>Local Document Scanner</h2>
          <span className="scan-header-subtitle">
            Recursive in-place indexing with incremental change detection
          </span>
        </div>
      </div>

      <div className="scan-content-grid">
        {/* Left Column: Root Selection & Action Controls */}
        <div className="scan-controls-card">
          <h3 className="section-title">Select Scan Root</h3>
          <p className="section-description">
            Choose an explicitly included directory. Symlinks, junctions, and dependency folders are excluded automatically.
          </p>

          <div className="root-picker-group">
            {enabledRoots.length > 0 ? (
              <select
                className="select-dropdown"
                value={selectedRootId}
                disabled={isScanning}
                onChange={(e) => {
                  const id = Number(e.target.value);
                  setSelectedRootId(id);
                  setProgress(null);
                }}
              >
                {enabledRoots.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.display_name} ({r.file_count} files)
                  </option>
                ))}
              </select>
            ) : (
              <div className="empty-warning">
                No enabled folders available. Add or enable a folder in the sidebar first.
              </div>
            )}

            {selectedRoot && (
              <div className="selected-root-detail">
                <div className="root-detail-row">
                  <span className="detail-label">Display Name:</span>
                  <span className="detail-value">{selectedRoot.display_name}</span>
                </div>
                <div className="root-detail-row">
                  <span className="detail-label">Absolute Path:</span>
                  <span className="detail-value path-mono" title={selectedRoot.absolute_path}>
                    {selectedRoot.absolute_path}
                  </span>
                </div>
                <div className="root-detail-row">
                  <span className="detail-label">Last Status:</span>
                  <span className="detail-value">
                    {selectedRoot.last_scan_status}
                  </span>
                </div>
              </div>
            )}
          </div>

          {generalError && (
            <div className="inline-error" style={{ margin: "14px 0" }}>
              <AlertTriangle size={15} />
              <span>{generalError}</span>
            </div>
          )}

          <div style={{ margin: "14px 0", padding: "10px 12px", background: "rgba(255, 255, 255, 0.03)", borderRadius: 6, border: "1px solid rgba(255, 255, 255, 0.08)" }}>
            <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: "0.85rem", cursor: isScanning ? "default" : "pointer", userSelect: "none" }}>
              <input
                type="checkbox"
                checked={computeHashes}
                disabled={isScanning}
                onChange={(e) => setComputeHashes(e.target.checked)}
              />
              <span style={{ fontWeight: 500 }}>Compute SHA-256 Hashes</span>
            </label>
            <div style={{ fontSize: "0.75rem", color: "#94a3b8", marginTop: 4, marginLeft: 22, lineHeight: 1.4 }}>
              Uses a 64 KB streaming buffer; never loads full documents into memory. Off by default for maximum scan performance.
            </div>
          </div>

          <div className="scan-action-buttons">
            {!isScanning ? (
              <button
                className="btn btn-primary btn-large"
                onClick={handleStartScan}
                disabled={!selectedRootId || enabledRoots.length === 0}
              >
                <Play size={16} /> Start Scan
              </button>
            ) : (
              <button
                className="btn btn-danger btn-large"
                onClick={handleCancelScan}
              >
                <Square size={16} /> Cancel Scan
              </button>
            )}
          </div>

          <div className="safety-guarantee-box">
            <Info size={14} color="#60a5fa" />
            <div className="safety-text">
              <strong>LibOne Safety Guarantee:</strong> Files are indexed in-place. LibOne never moves, modifies, uploads, or deletes your documents.
            </div>
          </div>
        </div>

        {/* Right Column: Live Progress, Metrics, and Warnings */}
        <div className="scan-status-panel">
          {/* Progress Header Status */}
          <div className="status-overview-card">
            <div className="status-header">
              <div className="status-title-group">
                <span className="status-label">Scan State</span>
                <div className="status-badge-row">
                  {progress?.state === "running" && (
                    <span className="scan-badge scan-badge--running">
                      <RefreshCw size={13} className="spin" /> Scanning…
                    </span>
                  )}
                  {progress?.state === "completed" && (
                    <span className="scan-badge scan-badge--completed">
                      <CheckCircle2 size={13} /> Completed
                    </span>
                  )}
                  {progress?.state === "cancelled" && (
                    <span className="scan-badge scan-badge--cancelled">
                      <Square size={13} /> Cancelled (Partial Progress Saved)
                    </span>
                  )}
                  {(!progress || progress.state === "idle") && (
                    <span className="scan-badge scan-badge--idle">
                      Ready to Scan
                    </span>
                  )}
                </div>
              </div>

              {progress?.current_directory && (
                <div className="current-dir-display">
                  <span className="dir-label">Current Directory:</span>
                  <span className="dir-path path-mono" title={progress.current_directory}>
                    {progress.current_directory}
                  </span>
                </div>
              )}
            </div>

            {/* Live Metrics Grid */}
            <div className="metrics-grid">
              <div className="metric-box">
                <div className="metric-icon">
                  <Folder size={18} color="#60a5fa" />
                </div>
                <div className="metric-number">
                  {progress?.directories_visited || 0}
                </div>
                <div className="metric-label">Directories Visited</div>
              </div>

              <div className="metric-box">
                <div className="metric-icon">
                  <FileSearch size={18} color="#34d399" />
                </div>
                <div className="metric-number">
                  {progress?.files_discovered || 0}
                </div>
                <div className="metric-label">Files Discovered</div>
              </div>

              <div className="metric-box">
                <div className="metric-icon">
                  <FileCheck size={18} color="#a78bfa" />
                </div>
                <div className="metric-number">
                  {progress?.files_indexed || 0}
                </div>
                <div className="metric-label">Files Indexed</div>
              </div>

              <div className="metric-box">
                <div className="metric-icon">
                  <AlertTriangle size={18} color="#f59e0b" />
                </div>
                <div className="metric-number">
                  {progress?.warnings.length || 0}
                </div>
                <div className="metric-label">Warnings</div>
              </div>
            </div>
          </div>

          {/* Scan Results Summary (when completed / finished) */}
          {progress?.stats && (
            <div className="summary-card">
              <h4 className="summary-title">
                <CheckCircle2 size={16} color="#10b981" /> Scan Summary Results
              </h4>
              <div className="summary-metrics">
                <div className="summary-metric">
                  <span className="sm-label">Total Scanned:</span>
                  <span className="sm-val">{progress.stats.total_scanned}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Added (New):</span>
                  <span className="sm-val font-success">+{progress.stats.added}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Modified:</span>
                  <span className="sm-val font-warning">{progress.stats.modified}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Moved / Renamed:</span>
                  <span className="sm-val" style={{ color: "#38bdf8" }}>{progress.stats.moved}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Unchanged:</span>
                  <span className="sm-val">{progress.stats.unchanged}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Marked Missing:</span>
                  <span className="sm-val font-danger">{progress.stats.missing}</span>
                </div>
                <div className="summary-metric">
                  <span className="sm-label">Elapsed Time:</span>
                  <span className="sm-val">
                    <Clock size={12} style={{ display: "inline", marginRight: 4 }} />
                    {progress.stats.duration_ms} ms
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* Non-fatal Warnings Area */}
          <div className="warnings-section">
            <div className="warnings-header">
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <ShieldAlert size={16} color="#f59e0b" />
                <h4 style={{ margin: 0, fontSize: "0.95rem" }}>
                  Non-Fatal Warnings ({warnings.length})
                </h4>
              </div>

              {warningCodes.length > 1 && (
                <div className="warning-filter-pills">
                  <button
                    className={`pill ${warningFilter === "all" ? "active" : ""}`}
                    onClick={() => setWarningFilter("all")}
                  >
                    All
                  </button>
                  {warningCodes.map((code) => (
                    <button
                      key={code}
                      className={`pill ${warningFilter === code ? "active" : ""}`}
                      onClick={() => setWarningFilter(code)}
                    >
                      {code}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {filteredWarnings.length > 0 ? (
              <div className="warnings-list">
                {filteredWarnings.map((w, idx) => (
                  <div key={idx} className="warning-row">
                    <span className={`warning-code-badge badge-${w.code}`}>
                      {w.code}
                    </span>
                    <div className="warning-content">
                      <div className="warning-path path-mono" title={w.path}>
                        {w.path}
                      </div>
                      <div className="warning-message">{w.message}</div>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="warnings-empty">
                <CheckCircle2 size={24} color="#10b981" />
                <span>No warnings recorded. All files and directories traversed cleanly.</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
