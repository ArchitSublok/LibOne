import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, fireEvent } from "@testing-library/react";
import { DuplicateGroupDetailView } from "./DuplicateGroupDetailView";
import * as api from "../services/api";
import type { DuplicateGroupDetail, FileComparisonItem } from "../types";

vi.mock("../services/api", () => ({
  getDuplicateGroupDetail: vi.fn(),
  markDuplicatePreferred: vi.fn(),
  approveDuplicateGroup: vi.fn(),
  rejectDuplicateGroup: vi.fn(),
  dismissDuplicateGroup: vi.fn(),
}));

const getDuplicateGroupDetail = vi.mocked(api.getDuplicateGroupDetail);
const markDuplicatePreferred = vi.mocked(api.markDuplicatePreferred);

const makeComparisonItem = (overrides: Partial<FileComparisonItem> = {}): FileComparisonItem => ({
  file: {
    id: 1,
    scan_root_id: 1,
    absolute_path: "C:\\Docs\\report.pdf",
    normalized_path: "c:\\docs\\report.pdf",
    file_name: "report.pdf",
    extension: "pdf",
    size_bytes: 1024,
    modified_at: "2026-11-20T10:00:00Z",
    created_at_if_available: null,
    discovered_at: "2026-11-20T10:00:00Z",
    last_seen_at: "2026-11-20T10:00:00Z",
    content_hash_nullable: "abc123hash456",
    file_id_nullable: null,
    status: "indexed",
  },
  root_display_name: "Docs Folder",
  root_enabled: true,
  exists_on_disk: true,
  is_canonical: false,
  ...overrides,
});

const makeGroupDetail = (overrides: Partial<DuplicateGroupDetail> = {}): DuplicateGroupDetail => ({
  group: {
    id: 42,
    match_type: "exact_hash",
    confidence: 1.0,
    created_at: "2026-11-20T10:00:00Z",
    status: "pending",
    file_ids: [1, 2],
  },
  files: [
    makeComparisonItem({
      file: {
        id: 1,
        scan_root_id: 1,
        absolute_path: "C:\\Docs\\report.pdf",
        normalized_path: "c:\\docs\\report.pdf",
        file_name: "report.pdf",
        extension: "pdf",
        size_bytes: 1024,
        modified_at: "2026-11-20T10:00:00Z",
        created_at_if_available: null,
        discovered_at: "2026-11-20T10:00:00Z",
        last_seen_at: "2026-11-20T10:00:00Z",
        content_hash_nullable: "abc123hash456",
        file_id_nullable: null,
        status: "indexed",
      },
      is_canonical: false,
    }),
    makeComparisonItem({
      file: {
        id: 2,
        scan_root_id: 1,
        absolute_path: "C:\\Docs\\report_copy.pdf",
        normalized_path: "c:\\docs\\report_copy.pdf",
        file_name: "report_copy.pdf",
        extension: "pdf",
        size_bytes: 1024,
        modified_at: "2026-11-20T10:05:00Z",
        created_at_if_available: null,
        discovered_at: "2026-11-20T10:05:00Z",
        last_seen_at: "2026-11-20T10:05:00Z",
        content_hash_nullable: "abc123hash456",
        file_id_nullable: null,
        status: "indexed",
      },
      is_canonical: false,
    }),
  ],
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("DuplicateGroupDetailView", () => {
  it("renders comparison table with name, path, size, modified date, hash, and status", async () => {
    getDuplicateGroupDetail.mockResolvedValue(makeGroupDetail());

    render(<DuplicateGroupDetailView groupId={42} onBack={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Duplicate Group #42")).toBeTruthy();
    });

    expect(screen.getByText("report.pdf")).toBeTruthy();
    expect(screen.getByText("report_copy.pdf")).toBeTruthy();
    expect(screen.getByText("C:\\Docs\\report.pdf")).toBeTruthy();
    expect(screen.getAllByText("1 KB").length).toBe(2);
    expect(screen.getByText(/Identical Content Hash/i)).toBeTruthy();
  });

  it("handles missing, disabled-root, and inaccessible files safely by blocking mark preferred", async () => {
    const detailWithUnsafeFiles = makeGroupDetail({
      files: [
        makeComparisonItem({
          file: {
            id: 1,
            scan_root_id: 1,
            absolute_path: "C:\\Docs\\valid.pdf",
            normalized_path: "c:\\docs\\valid.pdf",
            file_name: "valid.pdf",
            extension: "pdf",
            size_bytes: 500,
            modified_at: null,
            created_at_if_available: null,
            discovered_at: "2026-11-20T10:00:00Z",
            last_seen_at: "2026-11-20T10:00:00Z",
            content_hash_nullable: "h1",
            file_id_nullable: null,
            status: "indexed",
          },
          exists_on_disk: true,
          root_enabled: true,
        }),
        makeComparisonItem({
          file: {
            id: 2,
            scan_root_id: 1,
            absolute_path: "C:\\Docs\\missing.pdf",
            normalized_path: "c:\\docs\\missing.pdf",
            file_name: "missing.pdf",
            extension: "pdf",
            size_bytes: 500,
            modified_at: null,
            created_at_if_available: null,
            discovered_at: "2026-11-20T10:00:00Z",
            last_seen_at: "2026-11-20T10:00:00Z",
            content_hash_nullable: "h1",
            file_id_nullable: null,
            status: "missing",
          },
          exists_on_disk: false,
          root_enabled: true,
        }),
        makeComparisonItem({
          file: {
            id: 3,
            scan_root_id: 2,
            absolute_path: "C:\\Disabled\\file.pdf",
            normalized_path: "c:\\disabled\\file.pdf",
            file_name: "file.pdf",
            extension: "pdf",
            size_bytes: 500,
            modified_at: null,
            created_at_if_available: null,
            discovered_at: "2026-11-20T10:00:00Z",
            last_seen_at: "2026-11-20T10:00:00Z",
            content_hash_nullable: "h1",
            file_id_nullable: null,
            status: "indexed",
          },
          exists_on_disk: true,
          root_enabled: false,
        }),
      ],
    });

    getDuplicateGroupDetail.mockResolvedValue(detailWithUnsafeFiles);

    render(<DuplicateGroupDetailView groupId={42} onBack={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("valid.pdf")).toBeTruthy();
    });

    // Valid file has "Mark Preferred" button
    expect(screen.getAllByText("Mark Preferred").length).toBe(1);

    // Unsafe files show warnings and "Unavailable" label
    expect(screen.getAllByText(/Unavailable/i).length).toBe(2);
    expect(screen.getByText(/File does not exist on disk/i)).toBeTruthy();
    expect(screen.getByText(/Scan root is disabled/i)).toBeTruthy();
  });

  it("marks preferred file and displays the safety guarantee notice", async () => {
    const detail = makeGroupDetail();
    getDuplicateGroupDetail.mockResolvedValue(detail);

    const updatedDetail = makeGroupDetail({
      group: { ...detail.group, status: "approved" },
      files: [
        { ...detail.files[0], is_canonical: true },
        { ...detail.files[1], is_canonical: false },
      ],
    });
    markDuplicatePreferred.mockResolvedValue(updatedDetail);

    render(<DuplicateGroupDetailView groupId={42} onBack={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("report.pdf")).toBeTruthy();
    });

    const markBtns = screen.getAllByText("Mark Preferred");
    fireEvent.click(markBtns[0]);

    await waitFor(() => {
      expect(markDuplicatePreferred).toHaveBeenCalledWith(42, 1);
      expect(screen.getByText(/marked as preferred\/canonical/i)).toBeTruthy();
    });

    // Check strict safety guarantee is visible
    expect(
      screen.getByText("LibOne changes only its local metadata. Original files remain untouched.")
    ).toBeTruthy();
  });
});
