import React from "react";
import { AlertCircle, RefreshCw, ShieldCheck, Trash2 } from "lucide-react";

interface Props {
  isOpen: boolean;
  kind: "collection" | "tag";
  itemName: string;
  busy?: boolean;
  error?: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Confirmation dialog for deleting a LibOne collection or tag.
 * It intentionally spells out that only library metadata is removed and the
 * original files on disk are left untouched.
 */
export const ConfirmDeleteDialog: React.FC<Props> = ({
  isOpen,
  kind,
  itemName,
  busy = false,
  error = null,
  onConfirm,
  onCancel,
}) => {
  if (!isOpen) return null;

  const label = kind === "collection" ? "collection" : "tag";

  return (
    <div
      className="confirm-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={`Confirm ${label} deletion`}
    >
      <div className="confirm-box">
        <div className="confirm-icon">
          <Trash2 size={22} color="#f87171" />
        </div>
        <h4 className="confirm-title">Delete {label}?</h4>
        <p className="confirm-body">
          <strong>{itemName}</strong>
        </p>

        {error && (
          <div className="inline-error" role="alert" style={{ width: "100%", margin: "4px 0" }}>
            <AlertCircle size={13} />
            <span>{error}</span>
          </div>
        )}

        <p className="confirm-note">
          This removes the {label} and its file links from LibOne's local
          database only.
          <br />
          <strong>Your original files on disk will not be moved, renamed, or deleted.</strong>
          <br />
          <span
            style={{ display: "inline-flex", alignItems: "center", gap: 5, marginTop: 4 }}
          >
            <ShieldCheck size={13} color="#34d399" />
            Documents stay in their current folders.
          </span>
        </p>

        <div className="confirm-actions">
          <button className="btn btn-secondary" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button
            className="btn btn-danger"
            onClick={onConfirm}
            disabled={busy}
            autoFocus
          >
            {busy ? (
              <>
                <RefreshCw size={13} className="spin" /> Deleting…
              </>
            ) : (
              <>
                <Trash2 size={13} /> Delete {label}
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
