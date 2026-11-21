import React, { useState, useEffect } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  CheckCircle2,
  Copy,
  EyeOff,
  FileText,
  HardDrive,
  Info,
  RefreshCw,
  ShieldCheck,
  Star,
  X,
  XCircle,
} from "lucide-react";
import { DuplicateGroupDetail, FileComparisonItem } from "../types";
import {
  approveDuplicateGroup,
  dismissDuplicateGroup,
  getDuplicateGroupDetail,
  markDuplicatePreferred,
  rejectDuplicateGroup,
} from "../services/api";

interface Props {
  groupId: number;
  onBack: () => void;
  onGroupChanged?: () => void;
}

const matchTypeLabels: Record<string, string> = {
  file_identity: "NTFS File Identity",
  exact_hash: "Identical Content Hash (SHA-256)",
  name_and_size: "Same Filename & Byte Size",
};

const statusColors: Record<string, string> = {
  pending: "badge-warning",
  approved: "badge-success",
  rejected: "badge-error",
  dismissed: "badge-secondary",
};

const fileStatusLabels: Record<string, { label: string; className: string }> = {
  indexed: { label: "Indexed", className: "file-status--indexed" },
  changed: { label: "Changed", className: "file-status--changed" },
  missing: { label: "Missing", className: "file-status--missing" },
  inaccessible: { label: "Inaccessible", className: "file-status--inaccessible" },
  error: { label: "Error", className: "file-status--error" },
};

export const DuplicateGroupDetailView: React.FC<Props> = ({
  groupId,
  onBack,
  onGroupChanged,
}) => {
  const [detail, setDetail] = useState<DuplicateGroupDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionSuccess, setActionSuccess] = useState<string | null>(null);
  const [processing, setProcessing] = useState(false);

  const loadDetail = async () => {
    setLoading(true);
    setError(null);
    try {
      const d = await getDuplicateGroupDetail(groupId);
      setDetail(d);
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDetail();
  }, [groupId]);

  const handleMarkPreferred = async (fileId: number) => {
    setProcessing(true);
    setError(null);
    setActionSuccess(null);
    try {
      const updated = await markDuplicatePreferred(groupId, fileId);
      setDetail(updated);
      setActionSuccess(
        `File #${fileId} marked as preferred/canonical. Only metadata was changed — original files remain untouched.`
      );
      onGroupChanged?.();
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setProcessing(false);
    }
  };

  const handleApprove = async () => {
    setProcessing(true);
    setError(null);
    setActionSuccess(null);
    try {
      await approveDuplicateGroup(groupId);
      await loadDetail();
      setActionSuccess("Duplicate group approved. Files tagged in metadata only — no files were deleted or moved.");
      onGroupChanged?.();
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setProcessing(false);
    }
  };

  const handleReject = async () => {
    setProcessing(true);
    setError(null);
    try {
      await rejectDuplicateGroup(groupId);
      await loadDetail();
      setActionSuccess("Duplicate group rejected.");
      onGroupChanged?.();
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setProcessing(false);
    }
  };

  const handleDismiss = async () => {
    setProcessing(true);
    setError(null);
    try {
      await dismissDuplicateGroup(groupId);
      await loadDetail();
      setActionSuccess("Duplicate group dismissed.");
      onGroupChanged?.();
    } catch (e: any) {
      setError(e?.message || String(e));
    } finally {
      setProcessing(false);
    }
  };

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
        year: "numeric",
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
    } catch {
      return iso;
    }
  };

  const getFileWarnings = (item: FileComparisonItem): string[] => {
    const warnings: string[] = [];
    if (!item.exists_on_disk) warnings.push("File does not exist on disk");
    if (item.file.status === "missing") warnings.push("Marked as missing in library");
    if (item.file.status === "inaccessible") warnings.push("File is inaccessible");
    if (!item.root_enabled) warnings.push("Scan root is disabled");
    return warnings;
  };

  const canMarkPreferred = (item: FileComparisonItem): boolean => {
    return (
      item.exists_on_disk &&
      item.root_enabled &&
      item.file.status !== "missing" &&
      item.file.status !== "inaccessible" &&
      !item.is_canonical
    );
  };

  if (loading) {
    return (
      <div className="dup-detail-container">
        <div className="dup-detail__loading">
          <RefreshCw size={24} className="spin" />
          <p>Loading duplicate group detail…</p>
        </div>
      </div>
    );
  }

  if (error && !detail) {
    return (
      <div className="dup-detail-container">
        <div className="dup-detail__header">
          <button type="button" className="btn btn-secondary btn-sm" onClick={onBack}>
            <ArrowLeft size={14} /> Back
          </button>
        </div>
        <div className="alert-banner alert-banner--error">
          <AlertTriangle size={16} />
          <span>{error}</span>
        </div>
      </div>
    );
  }

  if (!detail) return null;

  const { group, files } = detail;
  const matchLabel = matchTypeLabels[group.match_type] || group.match_type;
  const statusBadge = statusColors[group.status] || "badge-secondary";

  return (
    <div className="dup-detail-container">
      {/* ── Header ──────────────────────────────────────────────────────────── */}
      <div className="dup-detail__header">
        <div className="dup-detail__header-left">
          <button type="button" className="btn btn-secondary btn-sm" onClick={onBack}>
            <ArrowLeft size={14} /> <span>Back to Suggestions</span>
          </button>
          <div className="dup-detail__header-info">
            <h2 className="dup-detail__title">
              <Copy size={18} /> Duplicate Group #{group.id}
            </h2>
            <div className="dup-detail__meta-row">
              <span className={`badge ${statusBadge}`}>{group.status.toUpperCase()}</span>
              <span className="dup-detail__match-type">
                <HardDrive size={13} /> {matchLabel}
              </span>
              <span className="dup-detail__confidence">
                {Math.round(group.confidence * 100)}% confidence
              </span>
              <span className="dup-detail__date">Created {formatDate(group.created_at)}</span>
            </div>
          </div>
        </div>

        {group.status === "pending" && (
          <div className="dup-detail__header-actions">
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={handleApprove}
              disabled={processing}
              title="Approve: tags files in metadata without altering originals"
            >
              <Check size={13} /> {processing ? "Processing…" : "Approve Group"}
            </button>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={handleReject}
              disabled={processing}
            >
              <X size={13} /> Reject
            </button>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={handleDismiss}
              disabled={processing}
            >
              <EyeOff size={13} /> Dismiss
            </button>
          </div>
        )}
      </div>

      {/* ── Safety Guarantee ─────────────────────────────────────────────────── */}
      <div className="safety-guarantee-banner" role="status">
        <ShieldCheck size={18} className="safety-guarantee-banner__icon" />
        <div className="safety-guarantee-banner__content">
          <strong>LibOne changes only its local metadata. Original files remain untouched.</strong>
          <p>
            Approving or marking a preferred file only updates tags and notes in LibOne's database.
            No files are deleted, moved, renamed, or modified on disk.
          </p>
        </div>
      </div>

      {/* ── Alerts ───────────────────────────────────────────────────────────── */}
      {error && (
        <div className="alert-banner alert-banner--error" role="alert">
          <AlertTriangle size={16} />
          <span className="alert-banner__message">{error}</span>
          <button type="button" className="btn-icon" onClick={() => setError(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      {actionSuccess && (
        <div className="alert-banner alert-banner--success" role="status">
          <CheckCircle2 size={16} />
          <span className="alert-banner__message">{actionSuccess}</span>
          <button type="button" className="btn-icon" onClick={() => setActionSuccess(null)}>
            <X size={14} />
          </button>
        </div>
      )}

      {/* ── File Comparison Table ─────────────────────────────────────────────── */}
      <div className="dup-detail__section">
        <h3 className="dup-detail__section-title">
          <FileText size={16} /> File Comparison ({files.length} files)
        </h3>

        {files.length === 0 ? (
          <div className="dup-detail__empty">
            <Info size={16} />
            <span>No file records found for this duplicate group.</span>
          </div>
        ) : (
          <div className="dup-comparison-table-wrapper">
            <table className="dup-comparison-table">
              <thead>
                <tr>
                  <th className="col-preferred">Preferred</th>
                  <th className="col-name">Name</th>
                  <th className="col-path">Path</th>
                  <th className="col-size">Size</th>
                  <th className="col-modified">Modified</th>
                  <th className="col-hash">Content Hash</th>
                  <th className="col-status">Status</th>
                  <th className="col-action">Action</th>
                </tr>
              </thead>
              <tbody>
                {files.map((item) => {
                  const warnings = getFileWarnings(item);
                  const hasWarning = warnings.length > 0;
                  const fStatus = fileStatusLabels[item.file.status] || {
                    label: item.file.status,
                    className: "",
                  };

                  return (
                    <tr
                      key={item.file.id}
                      className={`dup-comparison-row ${
                        item.is_canonical ? "dup-comparison-row--canonical" : ""
                      } ${hasWarning ? "dup-comparison-row--warning" : ""}`}
                    >
                      <td className="col-preferred">
                        {item.is_canonical ? (
                          <span className="canonical-badge" title="Preferred / Canonical file">
                            <Star size={14} fill="currentColor" /> Preferred
                          </span>
                        ) : (
                          <span className="not-canonical">—</span>
                        )}
                      </td>
                      <td className="col-name">
                        <div className="file-name-cell">
                          <FileText size={14} />
                          <span title={item.file.file_name}>{item.file.file_name}</span>
                        </div>
                      </td>
                      <td className="col-path">
                        <span className="file-path-cell" title={item.file.absolute_path}>
                          {item.file.absolute_path}
                        </span>
                        <span className="file-root-label">
                          Root: {item.root_display_name}
                          {!item.root_enabled && (
                            <span className="root-disabled-badge"> (disabled)</span>
                          )}
                        </span>
                      </td>
                      <td className="col-size">{formatBytes(item.file.size_bytes)}</td>
                      <td className="col-modified">{formatDate(item.file.modified_at)}</td>
                      <td className="col-hash">
                        <span
                          className="hash-cell"
                          title={item.file.content_hash_nullable || "No hash computed"}
                        >
                          {item.file.content_hash_nullable
                            ? item.file.content_hash_nullable.substring(0, 12) + "…"
                            : "—"}
                        </span>
                      </td>
                      <td className="col-status">
                        <div className="file-status-cell">
                          <span className={`file-status-pill ${fStatus.className}`}>
                            {fStatus.label}
                          </span>
                          {!item.exists_on_disk && item.file.status !== "missing" && (
                            <span className="file-status-pill file-status--missing">
                              Not on disk
                            </span>
                          )}
                        </div>
                        {hasWarning && (
                          <div className="file-warnings">
                            {warnings.map((w, i) => (
                              <span key={i} className="file-warning-item">
                                <AlertTriangle size={11} /> {w}
                              </span>
                            ))}
                          </div>
                        )}
                      </td>
                      <td className="col-action">
                        {canMarkPreferred(item) ? (
                          <button
                            type="button"
                            className="btn btn-primary btn-sm"
                            onClick={() => handleMarkPreferred(item.file.id)}
                            disabled={processing}
                            title="Mark this file as the preferred/canonical version (metadata only)"
                          >
                            <Star size={12} /> Mark Preferred
                          </button>
                        ) : item.is_canonical ? (
                          <span className="action-label--canonical">
                            <CheckCircle2 size={12} /> Current preferred
                          </span>
                        ) : hasWarning ? (
                          <span className="action-label--unavailable">
                            <XCircle size={12} /> Unavailable
                          </span>
                        ) : null}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* ── Explanation ────────────────────────────────────────────────────────── */}
      <div className="dup-detail__explanation">
        <Info size={14} />
        <div>
          <strong>What does "Mark Preferred" do?</strong>
          <p>
            Marking a file as preferred adds a "canonical" tag to that file and a "duplicate" tag
            to all files in this group. A private note is added to non-preferred files referencing
            the canonical version. This is a metadata-only operation — no files are deleted, moved,
            or changed on disk.
          </p>
          <p>
            Files that are missing from disk, inaccessible, or in a disabled scan root cannot be
            marked as preferred.
          </p>
        </div>
      </div>
    </div>
  );
};
