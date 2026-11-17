import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FileNotesSection } from "./FileNotesSection";
import * as api from "../services/api";
import type { FileNote } from "../types";

vi.mock("../services/api", () => ({
  listFileNotes: vi.fn(),
  createFileNote: vi.fn(),
  updateFileNote: vi.fn(),
  deleteFileNote: vi.fn(),
}));

const listFileNotes = vi.mocked(api.listFileNotes);
const createFileNote = vi.mocked(api.createFileNote);
const updateFileNote = vi.mocked(api.updateFileNote);
const deleteFileNote = vi.mocked(api.deleteFileNote);

const makeNote = (overrides: Partial<FileNote> = {}): FileNote => ({
  id: 10,
  file_id: 1,
  note_text: "Existing note",
  created_at: "2026-11-01T10:00:00Z",
  updated_at: "2026-11-01T10:00:00Z",
  ...overrides,
});

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("FileNotesSection", () => {
  it("shows loading then the empty state", async () => {
    const pending = deferred<FileNote[]>();
    listFileNotes.mockReturnValue(pending.promise);

    render(<FileNotesSection fileId={2} />);

    expect(screen.getByText(/loading notes/i)).toBeTruthy();
    expect(
      screen.getByText(/never modify the original file/i)
    ).toBeTruthy();

    pending.resolve([]);

    expect(await screen.findByText(/no private notes yet/i)).toBeTruthy();
  });

  it("creates a note and prevents duplicate submissions", async () => {
    const user = userEvent.setup();
    listFileNotes.mockResolvedValue([]);
    const pending = deferred<FileNote>();
    createFileNote.mockReturnValue(pending.promise);

    render(<FileNotesSection fileId={2} />);
    await screen.findByText(/no private notes yet/i);

    await user.click(screen.getByRole("button", { name: /add note/i }));
    await user.type(
      screen.getByPlaceholderText(/write a private note/i),
      "My private note"
    );

    const saveButton = screen.getByRole("button", { name: /save note/i });
    fireEvent.click(saveButton);
    fireEvent.click(saveButton);

    expect(createFileNote).toHaveBeenCalledTimes(1);
    expect(createFileNote).toHaveBeenCalledWith(2, "My private note");
    expect(
      (screen.getByRole("button", { name: /saving/i }) as HTMLButtonElement)
        .disabled
    ).toBe(true);

    pending.resolve(
      makeNote({
        id: 11,
        file_id: 2,
        note_text: "My private note",
        created_at: "2026-11-02T10:00:00Z",
        updated_at: "2026-11-02T10:00:00Z",
      })
    );

    expect(await screen.findByText("My private note")).toBeTruthy();
    expect(screen.getByText(/note saved in libone/i)).toBeTruthy();
    expect(screen.getByText(/created/i)).toBeTruthy();
    expect(screen.getByText(/updated/i)).toBeTruthy();
  });

  it("edits an existing note and refreshes the updated timestamp", async () => {
    const user = userEvent.setup();
    const note = makeNote();
    listFileNotes.mockResolvedValue([note]);
    updateFileNote.mockResolvedValue(
      makeNote({
        note_text: "Edited text",
        updated_at: "2026-11-03T12:30:00Z",
      })
    );

    render(<FileNotesSection fileId={1} />);
    expect(await screen.findByText("Existing note")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /edit/i }));
    const textarea = screen.getByDisplayValue("Existing note");
    await user.clear(textarea);
    await user.type(textarea, "Edited text");
    await user.click(screen.getByRole("button", { name: /save changes/i }));

    await waitFor(() =>
      expect(updateFileNote).toHaveBeenCalledWith(10, "Edited text")
    );
    expect(await screen.findByText("Edited text")).toBeTruthy();
    expect(screen.getByText("edited")).toBeTruthy();
    expect(screen.getByText(/note updated in libone/i)).toBeTruthy();
  });

  it("requires confirmation before deleting a note", async () => {
    const user = userEvent.setup();
    listFileNotes.mockResolvedValue([makeNote()]);
    deleteFileNote.mockResolvedValue(undefined);

    render(<FileNotesSection fileId={1} />);
    expect(await screen.findByText("Existing note")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /^delete$/i }));
    expect(screen.getByText(/delete this note\?/i)).toBeTruthy();
    expect(
      screen.getByText(/original file is not modified/i)
    ).toBeTruthy();
    expect(deleteFileNote).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: /delete note/i }));

    await waitFor(() => expect(deleteFileNote).toHaveBeenCalledWith(10));
    await waitFor(() =>
      expect(screen.queryByText("Existing note")).toBeNull()
    );
    expect(screen.getByText(/note deleted from libone/i)).toBeTruthy();
  });

  it("shows load errors, retries, and handles invalid file IDs", async () => {
    const user = userEvent.setup();
    listFileNotes
      .mockRejectedValueOnce(
        new Error("Indexed file #999 not found in library.")
      )
      .mockResolvedValueOnce([]);

    render(<FileNotesSection fileId={999} />);

    expect(
      await screen.findByText(/indexed file #999 not found in library/i)
    ).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /retry/i }));

    expect(await screen.findByText(/no private notes yet/i)).toBeTruthy();
    expect(listFileNotes).toHaveBeenCalledTimes(2);
  });

  it("keeps notes visible and editable when the original file is missing", async () => {
    listFileNotes.mockResolvedValue([
      makeNote({ file_id: 5, note_text: "Historical note" }),
    ]);

    render(<FileNotesSection fileId={5} isMissing />);

    expect(await screen.findByText("Historical note")).toBeTruthy();
    expect(
      screen.getByText(/original file is currently unavailable/i)
    ).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: /add note/i }) as HTMLButtonElement)
        .disabled
    ).toBe(false);
    expect(screen.getByRole("button", { name: /edit/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^delete$/i })).toBeTruthy();
  });

  it("protects unsaved edits when cancelling", async () => {
    const user = userEvent.setup();
    const onDirtyChange = vi.fn();
    listFileNotes.mockResolvedValue([makeNote()]);

    render(
      <FileNotesSection fileId={1} onDirtyChange={onDirtyChange} />
    );
    expect(await screen.findByText("Existing note")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /edit/i }));
    await user.type(screen.getByDisplayValue("Existing note"), " changed");

    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));

    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(confirmSpy).toHaveBeenCalledWith(
      expect.stringMatching(/unsaved note changes/i)
    );
    expect(
      screen.getByDisplayValue("Existing note changed")
    ).toBeTruthy();

    confirmSpy.mockReturnValue(true);
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(await screen.findByText("Existing note")).toBeTruthy();
  });

  it("shows saving errors without losing the draft", async () => {
    const user = userEvent.setup();
    listFileNotes.mockResolvedValue([]);
    createFileNote.mockRejectedValue(
      new Error("Indexed file #2 not found in library.")
    );

    render(<FileNotesSection fileId={2} />);
    await screen.findByText(/no private notes yet/i);

    await user.click(screen.getByRole("button", { name: /add note/i }));
    await user.type(
      screen.getByPlaceholderText(/write a private note/i),
      "Draft that should survive"
    );
    await user.click(screen.getByRole("button", { name: /save note/i }));

    expect(
      await screen.findByText(/indexed file #2 not found in library/i)
    ).toBeTruthy();
    expect(
      screen.getByDisplayValue("Draft that should survive")
    ).toBeTruthy();
  });
});
