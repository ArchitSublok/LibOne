import React, { useState, useEffect } from "react";
import { X, Folder, AlertCircle, RefreshCw, Check } from "lucide-react";
import { Collection } from "../types";
import { createCollection, updateCollection } from "../services/api";

interface Props {
  isOpen: boolean;
  collection: Collection | null; // null => creating new, non-null => renaming/editing
  onClose: () => void;
  onSaved: (saved: Collection) => void;
}

const COLOR_PRESETS = [
  { label: "Blue", value: "#3b82f6" },
  { label: "Emerald", value: "#10b981" },
  { label: "Purple", value: "#8b5cf6" },
  { label: "Amber", value: "#f59e0b" },
  { label: "Rose", value: "#ef4444" },
  { label: "Cyan", value: "#06b6d4" },
  { label: "Pink", value: "#ec4899" },
  { label: "Gray", value: "#64748b" },
];

export const CollectionModal: React.FC<Props> = ({
  isOpen,
  collection,
  onClose,
  onSaved,
}) => {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState<string | null>(COLOR_PRESETS[0].value);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (collection) {
      setName(collection.name);
      setDescription(collection.description_nullable || "");
      setColor(collection.color_nullable || COLOR_PRESETS[0].value);
    } else {
      setName("");
      setDescription("");
      setColor(COLOR_PRESETS[0].value);
    }
    setError(null);
    setLoading(false);
  }, [collection, isOpen]);

  if (!isOpen) return null;

  const isEditing = collection !== null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Collection name cannot be empty.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      let saved: Collection;
      if (isEditing) {
        saved = await updateCollection(
          collection.id,
          trimmed,
          description.trim() || null,
          color || null
        );
      } else {
        saved = await createCollection(
          trimmed,
          description.trim() || null,
          color || null
        );
      }
      onSaved(saved);
      onClose();
    } catch (err: any) {
      setError(err?.message || String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 480 }}>
        <div className="modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div
              className="logo-badge"
              style={{
                width: 32,
                height: 32,
                borderRadius: 8,
                background: color ? color : "linear-gradient(135deg, #2563eb, #06b6d4)",
              }}
            >
              <Folder size={16} />
            </div>
            <h3 style={{ margin: 0, fontSize: "1.05rem" }}>
              {isEditing ? "Rename Collection" : "Create New Collection"}
            </h3>
          </div>
          <button className="btn-icon" onClick={onClose} type="button" aria-label="Close dialog">
            <X size={16} />
          </button>
        </div>

        <form onSubmit={handleSubmit}>
          <div className="modal-body" style={{ padding: "16px 20px" }}>
            {error && (
              <div className="inline-error" role="alert" style={{ marginBottom: 14 }}>
                <AlertCircle size={14} />
                <span>{error}</span>
              </div>
            )}

            <div style={{ marginBottom: 14 }}>
              <label
                style={{
                  display: "block",
                  fontSize: "0.8rem",
                  fontWeight: 600,
                  marginBottom: 6,
                  color: "var(--text-secondary)",
                }}
              >
                Collection Name *
              </label>
              <input
                type="text"
                className="search-input"
                style={{ width: "100%", padding: "8px 12px", borderRadius: "var(--radius-sm)" }}
                placeholder="e.g. Taxes & Finance, Contracts, Receipts"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
                disabled={loading}
              />
            </div>

            <div style={{ marginBottom: 14 }}>
              <label
                style={{
                  display: "block",
                  fontSize: "0.8rem",
                  fontWeight: 600,
                  marginBottom: 6,
                  color: "var(--text-secondary)",
                }}
              >
                Description <span style={{ fontWeight: 400, opacity: 0.7 }}>(optional)</span>
              </label>
              <input
                type="text"
                className="search-input"
                style={{ width: "100%", padding: "8px 12px", borderRadius: "var(--radius-sm)" }}
                placeholder="Brief description of this collection"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                disabled={loading}
              />
            </div>

            <div style={{ marginBottom: 16 }}>
              <label
                style={{
                  display: "block",
                  fontSize: "0.8rem",
                  fontWeight: 600,
                  marginBottom: 8,
                  color: "var(--text-secondary)",
                }}
              >
                Color Tag
              </label>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
                {COLOR_PRESETS.map((p) => {
                  const isSelected = color?.toLowerCase() === p.value.toLowerCase();
                  return (
                    <button
                      key={p.value}
                      type="button"
                      onClick={() => setColor(p.value)}
                      title={p.label}
                      style={{
                        width: 26,
                        height: 26,
                        borderRadius: "50%",
                        background: p.value,
                        border: isSelected
                          ? "2px solid #ffffff"
                          : "2px solid transparent",
                        boxShadow: isSelected
                          ? "0 0 0 2px " + p.value
                          : "none",
                        cursor: "pointer",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        padding: 0,
                      }}
                    >
                      {isSelected && <Check size={14} color="#ffffff" strokeWidth={3} />}
                    </button>
                  );
                })}
              </div>
            </div>

            <div
              style={{
                fontSize: "0.78rem",
                color: "var(--text-muted)",
                background: "rgba(255, 255, 255, 0.03)",
                padding: "8px 12px",
                borderRadius: 6,
                border: "1px solid var(--border-color)",
                lineHeight: 1.4,
              }}
            >
              <strong>Safety Note:</strong> Collections organize your documents in LibOne. Your original files on disk are never moved, renamed, or modified.
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
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onClose}
              disabled={loading}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={loading || !name.trim()}
            >
              {loading ? (
                <>
                  <RefreshCw size={13} className="spin" />
                  Saving…
                </>
              ) : isEditing ? (
                "Save Changes"
              ) : (
                "Create Collection"
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
