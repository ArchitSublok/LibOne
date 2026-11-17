import React, { useState, useEffect } from "react";
import { X, Tag as TagIcon, AlertCircle, RefreshCw, Check } from "lucide-react";
import { Tag } from "../types";
import { createTag, updateTag } from "../services/api";

interface Props {
  isOpen: boolean;
  tag: Tag | null; // null => creating new, non-null => renaming/editing
  onClose: () => void;
  onSaved: (saved: Tag) => void;
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

export const TagModal: React.FC<Props> = ({ isOpen, tag, onClose, onSaved }) => {
  const [name, setName] = useState("");
  const [color, setColor] = useState<string | null>(COLOR_PRESETS[3].value);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (tag) {
      setName(tag.name);
      setColor(tag.color_nullable || COLOR_PRESETS[3].value);
    } else {
      setName("");
      setColor(COLOR_PRESETS[3].value);
    }
    setError(null);
    setLoading(false);
  }, [tag, isOpen]);

  if (!isOpen) return null;

  const isEditing = tag !== null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Tag name cannot be empty.");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      let saved: Tag;
      if (tag) {
        saved = await updateTag(tag.id, trimmed, color || null);
      } else {
        saved = await createTag(trimmed, color || null);
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
      <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 440 }}>
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
              <TagIcon size={16} />
            </div>
            <h3 style={{ margin: 0, fontSize: "1.05rem" }}>
              {isEditing ? "Rename Tag" : "Create New Tag"}
            </h3>
          </div>
          <button className="modal-icon-btn" onClick={onClose} type="button" aria-label="Close dialog">
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
                Tag Name *
              </label>
              <input
                type="text"
                className="search-input"
                style={{ width: "100%", padding: "8px 12px", borderRadius: "var(--radius-sm)" }}
                placeholder="e.g. receipt, urgent, work"
                value={name}
                onChange={(e) => setName(e.target.value)}
                autoFocus
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
                Color
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
                      aria-label={p.label}
                      aria-pressed={isSelected}
                      style={{
                        width: 26,
                        height: 26,
                        borderRadius: "50%",
                        background: p.value,
                        border: isSelected ? "2px solid #ffffff" : "2px solid transparent",
                        boxShadow: isSelected ? "0 0 0 2px " + p.value : "none",
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
              <strong>Safety Note:</strong> Tags are stored only in LibOne's local
              database. Your original files on disk are never moved, renamed, or
              modified.
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
                "Create Tag"
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
