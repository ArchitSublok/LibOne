import React, { useEffect, useState } from "react";
import {
  AlertCircle,
  Folder,
  RefreshCw,
  ShieldCheck,
  Tag as TagIcon,
  X,
} from "lucide-react";

export interface BulkAssignOption {
  id: number;
  name: string;
  color: string | null;
  count: number;
}

interface Props {
  isOpen: boolean;
  kind: "collection" | "tag";
  action: "add" | "remove";
  fileCount: number;
  options: BulkAssignOption[];
  loading: boolean;
  onClose: () => void;
  onConfirm: (targetId: number) => Promise<void>;
}

/**
 * Picker used by the multi-file action bar to add/remove the selected files
 * from a single collection or tag. All operations only touch LibOne metadata.
 */
export const BulkAssignModal: React.FC<Props> = ({
  isOpen,
  kind,
  action,
  fileCount,
  options,
  loading,
  onClose,
  onConfirm,
}) => {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedId(null);
    setBusy(false);
    setError(null);
  }, [isOpen, kind, action]);

  if (!isOpen) return null;

  const isCollection = kind === "collection";
  const noun = isCollection ? "collection" : "tag";
  const verb = action === "add" ? "Add" : "Remove";
  const Icon = isCollection ? Folder : TagIcon;

  const handleConfirm = async () => {
    if (selectedId === null) {
      setError(`Select a ${noun} first.`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onConfirm(selectedId);
      onClose();
    } catch (err: any) {
      setError(err?.message || String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={busy ? undefined : onClose} role="dialog" aria-modal="true">
      <div
        className="modal-content"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 460 }}
      >
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div
              className="logo-badge"
              style={{ width: 32, height: 32, borderRadius: 8 }}
            >
              <Icon size={16} />
            </div>
            <div>
              <h3 style={{ margin: 0, fontSize: "1.02rem" }}>
                {verb} {fileCount} file{fileCount === 1 ? "" : "s"}{" "}
                {action === "add" ? "to" : "from"} a {noun}
              </h3>
              <div style={{ fontSize: "0.76rem", color: "var(--text-muted)", marginTop: 2 }}>
                Only LibOne metadata is changed.
              </div>
            </div>
          </div>
          <button
            className="modal-icon-btn"
            onClick={onClose}
            type="button"
            aria-label="Close dialog"
            disabled={busy}
          >
            <X size={16} />
          </button>
        </div>

        <div className="modal-body" style={{ padding: "16px 20px" }}>
          {error && (
            <div className="inline-error" role="alert" style={{ marginBottom: 12 }}>
              <AlertCircle size={14} />
              <span>{error}</span>
            </div>
          )}

          {loading ? (
            <div className="bulk-picker-state">
              <RefreshCw size={18} className="spin" />
              <span>Loading {noun}s…</span>
            </div>
          ) : options.length === 0 ? (
            <div className="bulk-picker-state">
              <Icon size={22} />
              <span>
                No {noun}s yet. Create one from the sidebar first.
              </span>
            </div>
          ) : (
            <div className="bulk-picker-list" role="radiogroup" aria-label={`Choose ${noun}`}>
              {options.map((option) => {
                const selected = selectedId === option.id;
                return (
                  <label
                    key={option.id}
                    className={`bulk-picker-item ${selected ? "selected" : ""}`}
                  >
                    <input
                      type="radio"
                      name={`bulk-${kind}-target`}
                      checked={selected}
                      onChange={() => setSelectedId(option.id)}
                      disabled={busy}
                    />
                    <span
                      className="org-color-dot"
                      style={{ background: option.color || "#3b82f6" }}
                    />
                    <span className="bulk-picker-name" title={option.name}>
                      {option.name}
                    </span>
                    <span className="org-count-badge">
                      {option.count} file{option.count === 1 ? "" : "s"}
                    </span>
                  </label>
                );
              })}
            </div>
          )}

          <div
            style={{
              marginTop: 14,
              fontSize: "0.76rem",
              color: "var(--text-muted)",
              background: "rgba(255, 255, 255, 0.03)",
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--border-color)",
              lineHeight: 1.45,
              display: "flex",
              alignItems: "flex-start",
              gap: 6,
            }}
          >
            <ShieldCheck size={13} color="#34d399" style={{ flexShrink: 0, marginTop: 1 }} />
            <span>
              Your original files on disk are never moved, renamed, or modified
              by this action.
            </span>
          </div>
        </div>

        <div
          className="modal-footer"
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 10,
            padding: "12px 20px",
            borderTop: "1px solid var(--border-color)",
          }}
        >
          <button className="btn btn-secondary" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={handleConfirm}
            disabled={busy || loading || options.length === 0 || selectedId === null}
          >
            {busy ? (
              <>
                <RefreshCw size={13} className="spin" /> Applying…
              </>
            ) : (
              `${verb} ${fileCount === 1 ? "file" : "files"}`
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
