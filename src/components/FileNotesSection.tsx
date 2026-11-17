import React, { useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  AlertTriangle,
  Check,
  Clock,
  FileText,
  Pencil,
  Plus,
  RefreshCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { FileNote } from "../types";
import {
  createFileNote,
  deleteFileNote,
  listFileNotes,
  updateFileNote,
} from "../services/api";

interface Props {
  fileId: number;
  /** True when the indexed file no longer exists on disk. Notes stay editable. */
  isMissing?: boolean;
  /** Lets the containing modal protect unsaved edits before closing. */
  onDirtyChange?: (dirty: boolean) => void;
  /** Optional hook so the parent can refresh derived state (e.g. note counts). */
  onNotesChanged?: () => void;
}

type EditorMode = { kind: "create" } | { kind: "edit"; noteId: number } | null;

const formatTimestamp = (iso: string): string => {
  try {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return date.toLocaleString(undefined, {
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

const errorMessage = (err: unknown): string =>
  err instanceof Error ? err.message : String(err);

/**
 * Private, user-authored notes for a single indexed file.
 *
 * Notes are stored only in LibOne's local SQLite database. This component never
 * reads document contents and never writes to the original file.
 */
export const FileNotesSection: React.FC<Props> = ({
  fileId,
  isMissing = false,
  onDirtyChange,
  onNotesChanged,
}) => {
  const [notes, setNotes] = useState<FileNote[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  const [mode, setMode] = useState<EditorMode>(null);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [pendingDelete, setPendingDelete] = useState<FileNote | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  // Refs synchronously block double-submits that could slip through before a
  // React state update disables the button.
  const saveLock = useRef(false);
  const deleteLock = useRef(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setLoadError(null);
    setMode(null);
    setDraft("");
    setFormError(null);
    setNotice(null);
    setPendingDelete(null);
    setDeleteError(null);
    setNotes([]);

    listFileNotes(fileId)
      .then((loaded) => {
        if (!cancelled) setNotes(loaded);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(errorMessage(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [fileId, reloadKey]);

  const editingNote =
    mode?.kind === "edit"
      ? notes.find((note) => note.id === mode.noteId) ?? null
      : null;

  const isDirty =
    mode?.kind === "create"
      ? draft.trim().length > 0
      : mode?.kind === "edit"
      ? editingNote !== null && draft !== editingNote.note_text
      : false;

  useEffect(() => {
    onDirtyChange?.(isDirty);
  }, [isDirty, onDirtyChange]);

  useEffect(() => {
    return () => {
      onDirtyChange?.(false);
    };
  }, [onDirtyChange]);

  const confirmDiscardIfDirty = (nextNoteId?: number): boolean => {
    if (!isDirty) return true;
    if (mode?.kind === "edit" && nextNoteId === mode.noteId) return true;
    return window.confirm("You have unsaved note changes. Discard them?");
  };

  const beginCreate = () => {
    if (saveLock.current) return;
    if (!confirmDiscardIfDirty()) return;
    setPendingDelete(null);
    setDeleteError(null);
    setMode({ kind: "create" });
    setDraft("");
    setFormError(null);
    setNotice(null);
  };

  const beginEdit = (note: FileNote) => {
    if (saveLock.current) return;
    if (!confirmDiscardIfDirty(note.id)) return;
    setPendingDelete(null);
    setDeleteError(null);
    setMode({ kind: "edit", noteId: note.id });
    setDraft(note.note_text);
    setFormError(null);
    setNotice(null);
  };

  const cancelForm = () => {
    if (isDirty && !window.confirm("Discard unsaved note changes?")) return;
    setMode(null);
    setDraft("");
    setFormError(null);
  };

  const handleSave = async (event?: React.FormEvent) => {
    event?.preventDefault();
    if (saveLock.current) return;

    const trimmed = draft.trim();
    if (!trimmed) {
      setFormError("Note content cannot be empty.");
      return;
    }

    saveLock.current = true;
    setSaving(true);
    setFormError(null);
    setNotice(null);

    try {
      if (mode?.kind === "edit") {
        const updated = await updateFileNote(mode.noteId, trimmed);
        setNotes((current) =>
          current.map((note) => (note.id === updated.id ? updated : note))
        );
        setNotice("Note updated in LibOne's local database.");
      } else {
        const created = await createFileNote(fileId, trimmed);
        setNotes((current) => [created, ...current]);
        setNotice("Note saved in LibOne's local database.");
      }
      setMode(null);
      setDraft("");
      onNotesChanged?.();
    } catch (err) {
      setFormError(errorMessage(err));
    } finally {
      saveLock.current = false;
      setSaving(false);
    }
  };

  const requestDelete = (note: FileNote) => {
    if (saveLock.current || deleteLock.current) return;
    setDeleteError(null);
    setPendingDelete(note);
  };

  const cancelDelete = () => {
    if (deleteLock.current) return;
    setPendingDelete(null);
    setDeleteError(null);
  };

  const confirmDelete = async () => {
    if (!pendingDelete || deleteLock.current) return;
    const target = pendingDelete;
    deleteLock.current = true;
    setDeleting(true);
    setDeleteError(null);

    try {
      await deleteFileNote(target.id);
      setNotes((current) => current.filter((note) => note.id !== target.id));
      if (mode?.kind === "edit" && mode.noteId === target.id) {
        setMode(null);
        setDraft("");
        setFormError(null);
      }
      setPendingDelete(null);
      setNotice("Note deleted from LibOne. The original file was not modified.");
      onNotesChanged?.();
    } catch (err) {
      setDeleteError(errorMessage(err));
    } finally {
      deleteLock.current = false;
      setDeleting(false);
    }
  };

  const renderEditor = (title: string) => (
    <form className="note-editor" onSubmit={handleSave}>
      <label className="note-editor-title" htmlFor={`note-draft-${fileId}`}>
        {title}
      </label>
      <textarea
        id={`note-draft-${fileId}`}
        className="note-textarea"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            handleSave();
          }
        }}
        placeholder="Write a private note about this document…"
        rows={4}
        disabled={saving}
        autoFocus
      />
      {formError && (
        <div className="note-form-error" role="alert">
          <AlertCircle size={13} />
          <span>{formError}</span>
        </div>
      )}
      <div className="note-editor-footer">
        <span className="note-editor-hint">Ctrl+Enter to save</span>
        <div className="note-editor-actions">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={cancelForm}
            disabled={saving}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={saving || !draft.trim()}
          >
            {saving ? (
              <>
                <RefreshCw size={12} className="spin" /> Saving…
              </>
            ) : mode?.kind === "edit" ? (
              "Save Changes"
            ) : (
              "Save Note"
            )}
          </button>
        </div>
      </div>
    </form>
  );

  return (
    <section className="notes-section" aria-label="Private notes">
      <div className="notes-header">
        <div className="notes-heading">
          <FileText size={14} />
          <span>Private Notes</span>
          <span className="notes-count-badge">{notes.length}</span>
        </div>
        <button
          type="button"
          className="btn btn-secondary"
          style={{ padding: "3px 9px", fontSize: "0.74rem" }}
          onClick={beginCreate}
          disabled={loading || saving || mode?.kind === "create"}
          title="Add a private note stored only in LibOne"
        >
          <Plus size={12} /> Add Note
        </button>
      </div>

      <div className="notes-privacy">
        <ShieldCheck size={13} color="#34d399" />
        <span>
          Notes are stored only in LibOne&apos;s local SQLite database. They
          never modify the original file and never include automatically
          extracted document contents.
        </span>
      </div>

      {isMissing && (
        <div className="notes-missing-hint">
          <AlertTriangle size={13} />
          <span>
            The original file is currently unavailable, but your notes remain
            readable and editable.
          </span>
        </div>
      )}

      {notice && (
        <div className="notes-notice" role="status">
          <Check size={13} />
          <span>{notice}</span>
        </div>
      )}

      {mode?.kind === "create" && renderEditor("New note")}

      {loading ? (
        <div className="notes-state" aria-busy="true">
          <RefreshCw size={16} className="spin" />
          <span>Loading notes…</span>
        </div>
      ) : loadError ? (
        <div className="notes-state notes-state--error" role="alert">
          <AlertCircle size={16} />
          <span>{loadError}</span>
          <button
            type="button"
            className="btn btn-secondary notes-retry-btn"
            onClick={() => setReloadKey((key) => key + 1)}
          >
            <RefreshCw size={12} /> Retry
          </button>
        </div>
      ) : notes.length === 0 && mode?.kind !== "create" ? (
        <div className="notes-empty">
          <FileText size={18} />
          <p>No private notes yet.</p>
          <span>
            Use <strong>Add Note</strong> to record something only you can see.
            The original document is never changed.
          </span>
        </div>
      ) : (
        <div className="notes-list">
          {notes.map((note) => {
            const isEditing = mode?.kind === "edit" && mode.noteId === note.id;
            const isConfirmingDelete = pendingDelete?.id === note.id;
            const wasEdited = note.updated_at !== note.created_at;

            return (
              <article
                key={note.id}
                className={`note-card ${isEditing ? "note-card--editing" : ""}`}
              >
                {isEditing ? (
                  renderEditor("Edit note")
                ) : (
                  <>
                    <p className="note-text">{note.note_text}</p>

                    <div className="note-meta">
                      <span title={note.created_at}>
                        <Clock size={11} /> Created{" "}
                        {formatTimestamp(note.created_at)}
                      </span>
                      <span title={note.updated_at}>
                        <Clock size={11} /> Updated{" "}
                        {formatTimestamp(note.updated_at)}
                      </span>
                      {wasEdited && (
                        <span className="note-edited-badge">edited</span>
                      )}
                    </div>

                    {isConfirmingDelete && (
                      <div
                        className="note-delete-confirm"
                        role="alertdialog"
                        aria-label="Confirm note deletion"
                      >
                        <AlertTriangle size={16} color="#f87171" />
                        <div className="note-delete-body">
                          <strong>Delete this note?</strong>
                          <p>
                            This removes the note from LibOne&apos;s local
                            SQLite database only. Your original file is not
                            modified.
                          </p>
                          {deleteError && (
                            <div className="note-form-error" role="alert">
                              <AlertCircle size={12} />
                              <span>{deleteError}</span>
                            </div>
                          )}
                          <div className="note-delete-actions">
                            <button
                              type="button"
                              className="btn btn-secondary"
                              onClick={cancelDelete}
                              disabled={deleting}
                            >
                              Cancel
                            </button>
                            <button
                              type="button"
                              className="btn btn-danger"
                              onClick={confirmDelete}
                              disabled={deleting}
                            >
                              {deleting ? (
                                <>
                                  <RefreshCw size={12} className="spin" />{" "}
                                  Deleting…
                                </>
                              ) : (
                                <>
                                  <Trash2 size={12} /> Delete Note
                                </>
                              )}
                            </button>
                          </div>
                        </div>
                      </div>
                    )}

                    {!isConfirmingDelete && (
                      <div className="note-actions">
                        <button
                          type="button"
                          className="btn btn-secondary"
                          style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                          onClick={() => beginEdit(note)}
                          disabled={saving || deleting}
                        >
                          <Pencil size={11} /> Edit
                        </button>
                        <button
                          type="button"
                          className="btn btn-secondary"
                          style={{ padding: "2px 8px", fontSize: "0.72rem" }}
                          onClick={() => requestDelete(note)}
                          disabled={saving || deleting}
                        >
                          <Trash2 size={11} /> Delete
                        </button>
                      </div>
                    )}
                  </>
                )}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
};
