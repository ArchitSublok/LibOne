import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { VersionGroupDisplay, StaleFileExplanation } from "./RelationshipViews";
import * as api from "../services/api";
import type { FileComparisonItem, OrganizationSuggestion } from "../types";

vi.mock("../services/api", () => ({
  getFileComparison: vi.fn(),
  getIndexedFileDetail: vi.fn(),
}));

const getFileComparison = vi.mocked(api.getFileComparison);

const makeComparisonItem = (overrides: Partial<FileComparisonItem> = {}): FileComparisonItem => ({
  file: {
    id: 1,
    scan_root_id: 1,
    absolute_path: "C:\\Projects\\Report.docx",
    normalized_path: "c:\\projects\\report.docx",
    file_name: "Report.docx",
    extension: "docx",
    size_bytes: 50000,
    modified_at: "2026-11-10T10:00:00Z",
    created_at_if_available: null,
    discovered_at: "2026-11-10T10:00:00Z",
    last_seen_at: "2026-11-10T10:00:00Z",
    content_hash_nullable: "hash1",
    file_id_nullable: null,
    status: "indexed",
  },
  root_display_name: "Projects",
  root_enabled: true,
  exists_on_disk: true,
  is_canonical: false,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("VersionGroupDisplay", () => {
  it("renders related version timeline with newest version highlighted", async () => {
    const suggestion: OrganizationSuggestion = {
      id: 101,
      suggestion_type: "likely_version",
      title: "Version sequence detected: 'Report'",
      rationale: "Detected 2 related versions in the same folder",
      confidence: 0.85,
      status: "pending",
      created_at: "2026-11-20T10:00:00Z",
      reviewed_at: null,
      affected_file_ids: [1, 2],
      suggested_tag_id: null,
      suggested_tag_name: "version-series",
      suggested_collection_id: null,
      suggested_collection_name: null,
    };

    const items: FileComparisonItem[] = [
      makeComparisonItem({
        file: {
          id: 1,
          scan_root_id: 1,
          absolute_path: "C:\\Projects\\Report_v1.docx",
          normalized_path: "c:\\projects\\report_v1.docx",
          file_name: "Report_v1.docx",
          extension: "docx",
          size_bytes: 40000,
          modified_at: "2026-11-01T10:00:00Z",
          created_at_if_available: null,
          discovered_at: "2026-11-01T10:00:00Z",
          last_seen_at: "2026-11-01T10:00:00Z",
          content_hash_nullable: "hash_v1",
          file_id_nullable: null,
          status: "indexed",
        },
      }),
      makeComparisonItem({
        file: {
          id: 2,
          scan_root_id: 1,
          absolute_path: "C:\\Projects\\Report_v2.docx",
          normalized_path: "c:\\projects\\report_v2.docx",
          file_name: "Report_v2.docx",
          extension: "docx",
          size_bytes: 55000,
          modified_at: "2026-11-15T12:00:00Z",
          created_at_if_available: null,
          discovered_at: "2026-11-15T12:00:00Z",
          last_seen_at: "2026-11-15T12:00:00Z",
          content_hash_nullable: "hash_v2",
          file_id_nullable: null,
          status: "indexed",
        },
      }),
    ];

    getFileComparison.mockResolvedValue(items);

    render(<VersionGroupDisplay suggestion={suggestion} />);

    await waitFor(() => {
      expect(screen.getByText("Report_v2.docx")).toBeTruthy();
    });

    expect(screen.getByText("Report_v1.docx")).toBeTruthy();
    expect(screen.getByText("Latest")).toBeTruthy();
    expect(screen.getByText(/2 related versions detected/i)).toBeTruthy();
  });
});

describe("StaleFileExplanation", () => {
  it("renders explanation, last seen info, and modification age", async () => {
    const suggestion: OrganizationSuggestion = {
      id: 202,
      suggestion_type: "stale_file",
      title: "Archive candidate: 1 inactive document",
      rationale: "1 document has had no modifications for over 180 days.",
      confidence: 0.7,
      status: "pending",
      created_at: "2026-11-20T10:00:00Z",
      reviewed_at: null,
      affected_file_ids: [1],
      suggested_tag_id: null,
      suggested_tag_name: "archive",
      suggested_collection_id: null,
      suggested_collection_name: null,
    };

    const items: FileComparisonItem[] = [
      makeComparisonItem({
        file: {
          id: 1,
          scan_root_id: 1,
          absolute_path: "C:\\Projects\\OldArchive.pdf",
          normalized_path: "c:\\projects\\oldarchive.pdf",
          file_name: "OldArchive.pdf",
          extension: "pdf",
          size_bytes: 80000,
          modified_at: "2025-01-01T10:00:00Z",
          created_at_if_available: null,
          discovered_at: "2025-01-01T10:00:00Z",
          last_seen_at: "2026-05-01T10:00:00Z",
          content_hash_nullable: null,
          file_id_nullable: null,
          status: "indexed",
        },
      }),
    ];

    getFileComparison.mockResolvedValue(items);

    render(<StaleFileExplanation suggestion={suggestion} />);

    await waitFor(() => {
      expect(screen.getByText("OldArchive.pdf")).toBeTruthy();
    });

    expect(screen.getByText(/Last seen:/i)).toBeTruthy();
    expect(screen.getByText(/Last modified:/i)).toBeTruthy();
    expect(screen.getByText(/No files are moved, deleted, or altered/i)).toBeTruthy();
  });
});
