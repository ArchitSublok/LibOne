import React, { useState } from "react";
import {
  X,
  ExternalLink,
  FolderOpen,
  Copy,
  Check,
  AlertTriangle,
  Hash,
  ShieldCheck,
  RefreshCw,
} from "lucide-react";
import { FileDetail } from "../types";
import { computeFileHash, openOriginalFile, showInFileExplorer } from "../services/api";

interface Props {
  detail: FileDetail | null;
  isOpen: boolean;
  onClose: () => void;
  onFileUpdated?: () => void;
}

export const FileDetailModal: React.FC<Props> = ({
  detail,
  isOpen,
  onClose,
  onFileUpdated,
}) => {
  const [copiedPath, setCopiedPath] = useState(false);
  const [copiedHash, setCopiedHash] = useState(false);
  const [isComputingHash, setIsComputingHash] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [currentHash, setCurrentHash] = useState<string | null>(
    detail?.file.content_hash_nullable ?? null
  );

  if (!isOpen || !detail) return null;

  const { file, root_display_name, root_absolute_path, exists_on_disk } = detail;

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  const formatDate = (iso: string | null): string => {
    if (!iso) return "—";
    try {
      return new Date(iso).toLocaleString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  const handleCopyPath = () => {
    navigator.clipboard.writeText(file.absolute_path);
    setCopiedPath(true);
    setTimeout(() => setCopiedPath(false), 2000);
  };

  const handleCopyHash = () => {
    if (!currentHash) return;
    navigator.clipboard.writeText(currentHash);
    setCopiedHash(true);
    setTimeout(() => setCopiedHash(false), 2000);
  };

  const handleOpenFile = async () => {
    setActionError(null);
    try {
      await openOriginalFile(file.id);
    } catch (err: any) {
      setActionError(err?.message || String(err));
      if (onFileUpdated) onFileUpdated();
    }
  };

  const handleShowInExplorer = async () => {
    setActionError(null);
    try {
      await showInFileExplorer(file.id);
    } catch (err: any) {
      setActionError(err?.message || String(err));
      if (onFileUpdated) onFileUpdated();
    }
  };

  const handleComputeHash = async () => {
    setIsComputingHash(true);
    setActionError(null);
    try {
      const hash = await computeFileHash(file.id);
      setCurrentHash(hash);
      if (onFileUpdated) onFileUpdated();
    } catch (err: any) {
      setActionError(err?.message || String(err));
    } finally {
      setIsComputingHash(false);
    }
  };

  const isMissing = !exists_on_disk || file.status === "missing";

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-card"
        style={{ maxWidth: 640 }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div className="logo-badge" style={{ width: 34, height: 34, fontSize: "0.8rem" }}>
              {file.extension.toUpperCase() || "DOC"}
            </div>
            <div>
              <h3 style={{ margin: 0, fontSize: "1.05rem", wordBreak: "break-all" }}>
                {file.file_name}
              </h3>
              <div style={{ fontSize: "0.78rem", color: "var(--text-muted)", marginTop: 2 }}>
                Library Record #{file.id}
              </div>
            </div>
          </div>
          <button className="icon-button" onClick={onClose} title="Close">
            <X size={18} />
          </button>
        </div>

        {/* Unavailable file warning banner */}
        {isMissing && (
          <div
            style={{
              margin: "16px 20px 0",
              padding: "10px 14px",
              background: "rgba(239, 68, 68, 0.12)",
              border: "1px solid rgba(239, 68, 68, 0.3)",
              borderRadius: 6,
              color: "#fca5a5",
              fontSize: "0.83rem",
              display: "flex",
              alignItems: "flex-start",
              gap: 10,
            }}
          >
            <AlertTriangle size={18} style={{ flexShrink: 0, marginTop: 2, color: "#f87171" }} />
            <div>
              <strong>Original File Unavailable</strong>
              <div style={{ marginTop: 2, color: "var(--text-secondary)", fontSize: "0.8rem" }}>
                This file no longer exists at its original path on disk. LibOne preserves your historical metadata, but the document cannot be opened.
              </div>
            </div>
          </div>
        )}

        {/* Action Error Banner */}
        {actionError && (
          <div
            style={{
              margin: "12px 20px 0",
              padding: "10px 14px",
              background: "rgba(245, 158, 11, 0.12)",
              border: "1px solid rgba(245, 158, 11, 0.3)",
              borderRadius: 6,
              color: "#fbbf24",
              fontSize: "0.82rem",
              display: "flex",
              alignItems: "center",
              gap: 8,
            }}
          >
            <AlertTriangle size={16} style={{ flexShrink: 0 }} />
            <span>{actionError}</span>
          </div>
        )}

        {/* Metadata Details Body */}
        <div className="modal-body" style={{ maxHeight: "65vh", overflowY: "auto", padding: "16px 20px" }}>
          {/* Path box */}
          <div style={{ marginBottom: 16 }}>
            <div style={{ fontSize: "0.75rem", fontWeight: 600, color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 6 }}>
              Original Absolute Path
            </div>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "8px 12px",
                background: "rgba(0, 0, 0, 0.3)",
                border: "1px solid rgba(255, 255, 255, 0.08)",
                borderRadius: 6,
                fontFamily: "monospace",
                fontSize: "0.8rem",
                color: "var(--text-primary)",
                wordBreak: "break-all",
              }}
            >
              <span>{file.absolute_path}</span>
              <button
                className="icon-button"
                onClick={handleCopyPath}
                title="Copy Path"
                style={{ marginLeft: 8, flexShrink: 0 }}
              >
                {copiedPath ? <Check size={14} color="#10b981" /> : <Copy size={14} />}
              </button>
            </div>
          </div>

          {/* Grid of properties */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "1fr 1fr",
              gap: "12px 16px",
              marginBottom: 16,
            }}
          >
            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                Scan Root
              </span>
              <span style={{ fontSize: "0.85rem", fontWeight: 500 }} title={root_absolute_path}>
                {root_display_name}
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                Indexing Status
              </span>
              <span style={{ display: "inline-block" }}>
                {file.status === "indexed" && <span className="badge badge-active">Indexed</span>}
                {file.status === "changed" && <span className="badge badge-modified">Changed</span>}
                {file.status === "moved" && <span className="badge badge-moved">Moved</span>}
                {file.status === "missing" && <span className="badge badge-missing">Missing</span>}
                {file.status === "inaccessible" && <span className="badge badge-modified">Inaccessible</span>}
                {file.status === "error" && <span className="badge badge-missing">Error</span>}
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                File Size
              </span>
              <span style={{ fontSize: "0.85rem" }}>
                {formatBytes(file.size_bytes)}{" "}
                <span style={{ color: "var(--text-muted)", fontSize: "0.75rem" }}>
                  ({file.size_bytes.toLocaleString()} bytes)
                </span>
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                Format / Extension
              </span>
              <span style={{ fontSize: "0.85rem", textTransform: "uppercase" }}>
                {file.extension || "No extension"}
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                Last Modified
              </span>
              <span style={{ fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                {formatDate(file.modified_at)}
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                First Indexed
              </span>
              <span style={{ fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                {formatDate(file.discovered_at)}
              </span>
            </div>

            <div>
              <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                Last Verified
              </span>
              <span style={{ fontSize: "0.82rem", color: "var(--text-secondary)" }}>
                {formatDate(file.last_seen_at)}
              </span>
            </div>

            {file.file_id_nullable && (
              <div>
                <span className="detail-label" style={{ display: "block", fontSize: "0.75rem", color: "var(--text-muted)", marginBottom: 2 }}>
                  Windows File ID
                </span>
                <span style={{ fontSize: "0.8rem", fontFamily: "monospace", color: "#60a5fa" }}>
                  {file.file_id_nullable}
                </span>
              </div>
            )}
          </div>

          {/* Content Hash Box */}
          <div style={{ marginTop: 8, padding: "12px", background: "rgba(255, 255, 255, 0.02)", borderRadius: 6, border: "1px solid rgba(255, 255, 255, 0.06)" }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 6 }}>
              <span style={{ fontSize: "0.75rem", fontWeight: 600, color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.05em", display: "flex", alignItems: "center", gap: 6 }}>
                <Hash size={13} /> SHA-256 Content Hash
              </span>
              {!currentHash && !isMissing && (
                <button
                  className="btn btn-secondary"
                  style={{ padding: "3px 8px", fontSize: "0.72rem" }}
                  onClick={handleComputeHash}
                  disabled={isComputingHash}
                >
                  {isComputingHash ? (
                    <>
                      <RefreshCw size={11} className="spin" /> Streaming…
                    </>
                  ) : (
                    "Compute Hash Now"
                  )}
                </button>
              )}
            </div>

            {currentHash ? (
              <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", fontFamily: "monospace", fontSize: "0.76rem", color: "var(--text-secondary)", wordBreak: "break-all", background: "rgba(0,0,0,0.25)", padding: "6px 10px", borderRadius: 4 }}>
                <span>{currentHash}</span>
                <button className="icon-button" onClick={handleCopyHash} title="Copy SHA-256 Hash" style={{ marginLeft: 6, flexShrink: 0 }}>
                  {copiedHash ? <Check size={13} color="#10b981" /> : <Copy size={13} />}
                </button>
              </div>
            ) : (
              <div style={{ fontSize: "0.76rem", color: "var(--text-muted)", fontStyle: "italic" }}>
                Not calculated yet. (Hashes are computed on demand to avoid unnecessary disk I/O).
              </div>
            )}
          </div>

          {/* Privacy Note */}
          <div style={{ marginTop: 14, fontSize: "0.73rem", color: "var(--text-muted)", display: "flex", alignItems: "center", gap: 6 }}>
            <ShieldCheck size={13} color="#10b981" />
            <span>Document contents are never read into memory or stored in SQLite. Only local filesystem metadata is displayed.</span>
          </div>
        </div>

        {/* Modal Footer Actions */}
        <div className="modal-footer" style={{ display: "flex", justifyContent: "space-between", padding: "14px 20px" }}>
          <button className="btn btn-secondary" onClick={onClose}>
            Close
          </button>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              className="btn btn-secondary"
              onClick={handleShowInExplorer}
              title="Locate original file in Windows File Explorer"
            >
              <FolderOpen size={14} /> Show in Explorer
            </button>
            <button
              className="btn btn-primary"
              onClick={handleOpenFile}
              disabled={isMissing}
              title={isMissing ? "Cannot open unavailable file" : "Open with Windows default application"}
            >
              <ExternalLink size={14} /> Open Original
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
