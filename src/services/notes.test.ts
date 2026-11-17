import { describe, expect, it } from "vitest";
import {
  createFileNote,
  deleteFileNote,
  getFileNote,
  listFileNotes,
  updateFileNote,
} from "./api";

describe("file notes API (local mock store)", () => {
  it("creates, lists, edits, and deletes a note", async () => {
    const created = await createFileNote(2, "  Frontend private note  ");

    try {
      expect(created.file_id).toBe(2);
      expect(created.note_text).toBe("Frontend private note");
      expect(created.created_at).toBe(created.updated_at);

      const listed = await listFileNotes(2);
      expect(listed.some((note) => note.id === created.id)).toBe(true);

      const updated = await updateFileNote(
        created.id,
        "  Frontend private note edited  "
      );
      expect(updated.note_text).toBe("Frontend private note edited");
      expect(new Date(updated.updated_at).getTime()).toBeGreaterThanOrEqual(
        new Date(updated.created_at).getTime()
      );

      const reloaded = await listFileNotes(2);
      expect(
        reloaded.find((note) => note.id === created.id)?.note_text
      ).toBe("Frontend private note edited");
      expect((await getFileNote(created.id)).note_text).toBe(
        "Frontend private note edited"
      );
    } finally {
      await deleteFileNote(created.id);
    }

    const afterDelete = await listFileNotes(2);
    expect(afterDelete.some((note) => note.id === created.id)).toBe(false);
  });

  it("supports multiple notes per file and keeps them isolated per file", async () => {
    const first = await createFileNote(2, "First note");
    const second = await createFileNote(2, "Second note");
    const otherFile = await createFileNote(3, "Note on another file");

    try {
      const notes = await listFileNotes(2);
      const ids = notes.map((note) => note.id);
      expect(ids).toContain(first.id);
      expect(ids).toContain(second.id);

      const otherNotes = await listFileNotes(3);
      expect(otherNotes.some((note) => note.id === otherFile.id)).toBe(true);
      expect(otherNotes.some((note) => note.id === first.id)).toBe(false);
    } finally {
      await deleteFileNote(first.id);
      await deleteFileNote(second.id);
      await deleteFileNote(otherFile.id);
    }
  });

  it("rejects empty text and invalid file or note IDs", async () => {
    await expect(createFileNote(2, "   ")).rejects.toThrow(
      "Note content cannot be empty"
    );
    await expect(createFileNote(999_999, "note")).rejects.toThrow(
      "not found in library"
    );
    await expect(listFileNotes(999_999)).rejects.toThrow("not found in library");
    await expect(getFileNote(999_999)).rejects.toThrow("not found");
    await expect(updateFileNote(999_999, "note")).rejects.toThrow("not found");
    await expect(deleteFileNote(999_999)).rejects.toThrow("not found");

    const note = await createFileNote(2, "Valid note");
    try {
      await expect(updateFileNote(note.id, "\n\t ")).rejects.toThrow(
        "Note content cannot be empty"
      );
    } finally {
      await deleteFileNote(note.id);
    }
  });

  it("keeps notes for a file marked missing and persists them across reads", async () => {
    // Mock file #5 is marked missing. Notes are library metadata and must not
    // require the original file to be present.
    const created = await createFileNote(5, "Historical note for a missing file");

    try {
      const firstRead = await listFileNotes(5);
      expect(
        firstRead.find((note) => note.id === created.id)?.note_text
      ).toBe("Historical note for a missing file");

      const secondRead = await listFileNotes(5);
      expect(secondRead.find((note) => note.id === created.id)).toEqual(created);
    } finally {
      await deleteFileNote(created.id);
    }
  });
});
