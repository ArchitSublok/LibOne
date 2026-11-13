import React, { useState } from "react";
import {
  FolderPlus,
  RefreshCw,
  Trash2,
  Folder,
  FolderX,
  Power,
  PowerOff,
  HardDrive,
  AlertCircle,
} from "lucide-react";
import { ScanRoot, ScanStats } from "../types";
import {
  addScanRoot,
  chooseFolder,
  enableScanRoot,
  removeScanRoot,
  scanRoot,
} from "../services/api";

// ── Types ────────────────────────────────────────────────────────────────────

interface Props {
  roots: ScanRoot[];
  selectedRootId: number | null;
  totalFileCount: number;
  appDataDbPath: string | undefined;
  isScanning: boolean;
  onSelectRoot: (id: number | null) => void;
  onRootsChanged: () => Promise<void>;
  onScanComplete: (stats: ScanStats) => void;
  onScanningChange: (scanning: boolean) => void;
  onOpenScanScreen?: (rootId: number) => void;
}

type LoadState = "idle" | "loading" | "error";

// ── Helpers ──────────────────────────────────────────────────────────────────

function statusLabel(root: ScanRoot): string {
  if (!root.enabled) return "Disabled";
  switch (root.last_scan_status) {
    case "never_scanned": return "Never scanned";
    case "running":       return "Scanning…";
    case "completed":
      if (root.last_scan_completed_at) {
        const d = new Date(root.last_scan_completed_at);
        return `Scanned ${d.toLocaleDateString(undefined, { month: "short", day: "numeric" })} ${d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
      }
      return "Completed";
    case "error": return "Scan error";
    default:      return root.last_scan_status;
  }
}

function StatusDot({ root }: { root: ScanRoot }) {
  if (!root.enabled)
    return <span className="status-dot status-dot--disabled" title="Disabled" />;
  if (root.last_scan_status === "running")
    return <span className="status-dot status-dot--scanning" title="Scanning" />;
  if (root.last_scan_status === "error")
    return <span className="status-dot status-dot--error" title="Scan error" />;
  if (root.last_scan_status === "completed")
    return <span className="status-dot status-dot--ok" title="Up to date" />;
  return <span className="status-dot status-dot--idle" title="Never scanned" />;
}

// ── Confirm-remove dialog (inline) ───────────────────────────────────────────

interface ConfirmRemoveProps {
  root: ScanRoot;
  onConfirm: () => void;
  onCancel: () => void;
}

function ConfirmRemoveDialog({ root, onConfirm, onCancel }: ConfirmRemoveProps) {
  return (
    <div className="confirm-overlay" role="dialog" aria-modal="true" aria-label="Confirm removal">
      <div className="confirm-box">
        <div className="confirm-icon">
          <Trash2 size={22} color="#f87171" />
        </div>
        <h4 className="confirm-title">Remove from LibOne?</h4>
        <p className="confirm-body">
          <strong>{root.display_name}</strong>
          <br />
          <span className="path-mono">{root.absolute_path}</span>
        </p>
        <p className="confirm-note">
          LibOne will only remove its local index records. <br />
          <strong>Your actual files and folders will not be touched.</strong>
        </p>
        <div className="confirm-actions">
          <button className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn btn-danger" onClick={onConfirm} autoFocus>
            <Trash2 size={13} /> Remove from index
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function ScanRootsPanel({
  roots,
  selectedRootId,
  totalFileCount,
  appDataDbPath,
  isScanning,
  onSelectRoot,
  onRootsChanged,
  onScanComplete,
  onScanningChange,
  onOpenScanScreen,
}: Props) {
  const [addState, setAddState] = useState<LoadState>("idle");
  const [addError, setAddError] = useState<string | null>(null);
  const [scanningId, setScanningId] = useState<number | null>(null);
  const [confirmRemoveRoot, setConfirmRemoveRoot] = useState<ScanRoot | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // ── Add folder flow ─────────────────────────────────────────────────────

  const handleAddFolder = async () => {
    setAddError(null);
    setActionError(null);

    // 1. Open native picker — no path is transmitted until the user confirms.
    let folderPath: string | null;
    try {
      folderPath = await chooseFolder();
    } catch (e: any) {
      setAddError(`Could not open folder picker: ${e?.message ?? e}`);
      return;
    }
    if (!folderPath) return; // user cancelled

    setAddState("loading");
    try {
      await addScanRoot(folderPath);
      await onRootsChanged();
      setAddState("idle");
    } catch (e: any) {
      setAddState("error");
      setAddError(e?.message ?? String(e));
    }
  };

  // ── Scan ─────────────────────────────────────────────────────────────────

  const handleScan = async (e: React.MouseEvent, root: ScanRoot) => {
    e.stopPropagation();
    if (onOpenScanScreen) {
      onOpenScanScreen(root.id);
      return;
    }
    setActionError(null);
    setScanningId(root.id);
    onScanningChange(true);
    try {
      const stats = await scanRoot(root.id);
      onScanComplete(stats);
      await onRootsChanged();
    } catch (e: any) {
      setActionError(`Scan failed for "${root.display_name}": ${e?.message ?? e}`);
    } finally {
      setScanningId(null);
      onScanningChange(false);
    }
  };

  // ── Disable / enable ──────────────────────────────────────────────────────

  const handleToggleEnabled = async (e: React.MouseEvent, root: ScanRoot) => {
    e.stopPropagation();
    setActionError(null);
    try {
      if (root.enabled) {
        await removeScanRoot(root.id);
      } else {
        await enableScanRoot(root.id);
      }
      await onRootsChanged();
    } catch (e: any) {
      setActionError(`Failed to toggle "${root.display_name}": ${e?.message ?? e}`);
    }
  };

  // ── Remove (with confirm) ─────────────────────────────────────────────────

  const handleRemoveClick = (e: React.MouseEvent, root: ScanRoot) => {
    e.stopPropagation();
    setActionError(null);
    setConfirmRemoveRoot(root);
  };

  const handleConfirmRemove = async () => {
    if (!confirmRemoveRoot) return;
    const root = confirmRemoveRoot;
    setConfirmRemoveRoot(null);
    try {
      await removeScanRoot(root.id);
      if (selectedRootId === root.id) onSelectRoot(null);
      await onRootsChanged();
    } catch (e: any) {
      setActionError(`Remove failed: ${e?.message ?? e}`);
    }
  };

  // ── Render ────────────────────────────────────────────────────────────────

  const isAddLoading = addState === "loading";

  return (
    <aside className="sidebar">
      {/* Confirm-remove overlay */}
      {confirmRemoveRoot && (
        <ConfirmRemoveDialog
          root={confirmRemoveRoot}
          onConfirm={handleConfirmRemove}
          onCancel={() => setConfirmRemoveRoot(null)}
        />
      )}

      {/* Header */}
      <div className="sidebar-header">
        <span className="sidebar-title">
          <Folder size={14} style={{ marginRight: 5, verticalAlign: "middle" }} />
          Folders ({roots.length})
        </span>
        <button
          className="btn btn-primary"
          style={{ padding: "4px 10px", fontSize: "0.75rem" }}
          onClick={handleAddFolder}
          disabled={isAddLoading || isScanning}
          title="Choose a folder to add to LibOne"
        >
          {isAddLoading ? (
            <RefreshCw size={12} className="spin" />
          ) : (
            <FolderPlus size={12} />
          )}
          {isAddLoading ? "Adding…" : "Add"}
        </button>
      </div>

      {/* Add-error banner */}
      {addState === "error" && addError && (
        <div className="inline-error" role="alert">
          <AlertCircle size={13} />
          <span>{addError}</span>
          <button
            className="inline-error__dismiss"
            onClick={() => { setAddState("idle"); setAddError(null); }}
            aria-label="Dismiss error"
          >
            ×
          </button>
        </div>
      )}

      {/* Action-error banner (scan / toggle / remove) */}
      {actionError && (
        <div className="inline-error" role="alert">
          <AlertCircle size={13} />
          <span>{actionError}</span>
          <button
            className="inline-error__dismiss"
            onClick={() => setActionError(null)}
            aria-label="Dismiss error"
          >
            ×
          </button>
        </div>
      )}

      {/* Root list */}
      <div className="roots-list">
        {/* "All Documents" aggregate entry */}
        <div
          className={`root-card ${selectedRootId === null ? "active" : ""}`}
          onClick={() => onSelectRoot(null)}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => e.key === "Enter" && onSelectRoot(null)}
          aria-pressed={selectedRootId === null}
        >
          <div className="root-top">
            <span className="root-label">All Documents</span>
            <span className="badge badge-active">{totalFileCount} files</span>
          </div>
          <div className="root-path">Aggregate across all folders</div>
        </div>

        {/* Individual roots */}
        {roots.map((root) => {
          const thisScanning = scanningId === root.id;
          return (
            <div
              key={root.id}
              className={`root-card ${selectedRootId === root.id ? "active" : ""} ${!root.enabled ? "root-card--disabled" : ""}`}
              onClick={() => onSelectRoot(root.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === "Enter" && onSelectRoot(root.id)}
              aria-pressed={selectedRootId === root.id}
            >
              <div className="root-top">
                <span className="root-label" title={root.display_name}>
                  <StatusDot root={root} />
                  {root.display_name}
                </span>

                {/* Action buttons */}
                <div className="root-actions" onClick={(e) => e.stopPropagation()}>
                  {/* Scan */}
                  {root.enabled && (
                    <button
                      className="root-action-btn"
                      onClick={(e) => handleScan(e, root)}
                      disabled={thisScanning || isScanning}
                      title="Scan this folder now"
                      aria-label={`Scan ${root.display_name}`}
                    >
                      <RefreshCw size={12} className={thisScanning ? "spin" : ""} />
                    </button>
                  )}

                  {/* Enable / Disable toggle */}
                  <button
                    className="root-action-btn"
                    onClick={(e) => handleToggleEnabled(e, root)}
                    disabled={thisScanning || isScanning}
                    title={root.enabled ? "Disable this folder (stops indexing; your original files remain untouched)" : "Re-enable indexing for this folder"}
                    aria-label={root.enabled ? `Disable ${root.display_name} in index` : `Enable ${root.display_name} in index`}
                  >
                    {root.enabled ? <PowerOff size={12} /> : <Power size={12} />}
                  </button>

                  {/* Remove */}
                  <button
                    className="root-action-btn root-action-btn--danger"
                    onClick={(e) => handleRemoveClick(e, root)}
                    disabled={thisScanning || isScanning}
                    title="Remove from LibOne index (original files are untouched)"
                    aria-label={`Remove ${root.display_name}`}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              </div>

              <div className="root-path" title={root.absolute_path}>
                {root.absolute_path}
              </div>

              <div className="root-meta">
                <span>
                  {root.enabled ? (
                    <>{root.file_count} file{root.file_count !== 1 ? "s" : ""}</>
                  ) : (
                    <>
                      <FolderX size={11} style={{ verticalAlign: "middle", marginRight: 2 }} />
                      Disabled
                    </>
                  )}
                </span>
                <span className="root-scan-status">{statusLabel(root)}</span>
              </div>
            </div>
          );
        })}

        {/* Empty state */}
        {roots.length === 0 && addState !== "loading" && (
          <div className="empty-state" style={{ padding: "28px 16px" }}>
            <Folder size={36} className="empty-state-icon" />
            <h4 style={{ margin: "10px 0 6px", fontSize: "0.95rem" }}>No folders added yet</h4>
            <p style={{ fontSize: "0.82rem", marginBottom: 14 }}>
              LibOne indexes your files in place — your documents stay exactly where they are.
            </p>
            <button
              className="btn btn-primary"
              onClick={handleAddFolder}
              disabled={isAddLoading}
            >
              <FolderPlus size={14} /> Add your first folder
            </button>
          </div>
        )}

        {/* Loading skeleton while adding */}
        {isAddLoading && (
          <div className="root-card root-card--skeleton" aria-busy="true">
            <div className="skeleton-line skeleton-line--wide" />
            <div className="skeleton-line skeleton-line--narrow" />
          </div>
        )}
      </div>

      {/* Footer: local data location */}
      <div className="sidebar-footer">
        <div className="app-meta-box">
          <div style={{ display: "flex", alignItems: "center", gap: 5, marginBottom: 3 }}>
            <HardDrive size={12} color="#60a5fa" />
            <strong style={{ color: "#cbd5e1" }}>Local Data Location</strong>
          </div>
          <div className="path-mono" title={appDataDbPath}>
            {appDataDbPath || "%APPDATA%\\com.libone.app\\libone.db"}
          </div>
        </div>
      </div>
    </aside>
  );
}
