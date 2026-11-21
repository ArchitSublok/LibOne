import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { SuggestionsView } from "./SuggestionsView";
import * as api from "../services/api";
import type { LocalAiStatus, OrganizationSuggestion, DuplicateGroup } from "../types";

vi.mock("../services/api", () => ({
  listOrganizationSuggestions: vi.fn(),
  listDuplicateGroups: vi.fn(),
  getLocalAiStatus: vi.fn(),
  generateSuggestions: vi.fn(),
  approveSuggestion: vi.fn(),
  rejectSuggestion: vi.fn(),
  dismissSuggestion: vi.fn(),
  bulkApproveSuggestions: vi.fn(),
  bulkRejectSuggestions: vi.fn(),
  getIndexedFileDetail: vi.fn(),
}));

const listOrganizationSuggestions = vi.mocked(api.listOrganizationSuggestions);
const listDuplicateGroups = vi.mocked(api.listDuplicateGroups);
const getLocalAiStatus = vi.mocked(api.getLocalAiStatus);

const sampleSuggestion: OrganizationSuggestion = {
  id: 1,
  suggestion_type: "suggest_tag",
  title: "Tag 2 files with '2026'",
  rationale: "Discovered year 2026 across 2 documents",
  confidence: 0.9,
  status: "pending",
  created_at: "2026-11-20T10:00:00Z",
  reviewed_at: null,
  affected_file_ids: [10, 11],
  suggested_tag_id: null,
  suggested_tag_name: "2026",
  suggested_collection_id: null,
  suggested_collection_name: null,
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("SuggestionsView - Local AI Provider Interface", () => {
  it("clearly displays 'Local AI is not configured' when the local AI provider is unavailable", async () => {
    const disabledStatus: LocalAiStatus = {
      is_available: false,
      provider_name: "DisabledLocalProvider",
      status_message: "Local AI is not configured",
      model_name: null,
      allows_network: false,
    };

    getLocalAiStatus.mockResolvedValue(disabledStatus);
    listOrganizationSuggestions.mockResolvedValue([sampleSuggestion]);
    listDuplicateGroups.mockResolvedValue([]);

    render(<SuggestionsView onBackToLibrary={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Local AI is not configured")).toBeTruthy();
    });

    // Verifies deterministic metadata suggestions remain active as default
    expect(
      screen.getByText(/Deterministic metadata suggestions remain active as the default engine/i)
    ).toBeTruthy();
    expect(screen.getByText("Tag 2 files with '2026'")).toBeTruthy();
  });

  it("functions normally with deterministic metadata analysis when local AI status fails or is rejected", async () => {
    getLocalAiStatus.mockRejectedValue(new Error("Local AI not available"));
    listOrganizationSuggestions.mockResolvedValue([sampleSuggestion]);
    listDuplicateGroups.mockResolvedValue([]);

    render(<SuggestionsView onBackToLibrary={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByText("Tag 2 files with '2026'")).toBeTruthy();
    });

    expect(screen.getByText("Local AI is not configured")).toBeTruthy();
  });
});
