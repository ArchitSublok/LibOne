import { describe, expect, it } from "vitest";
import {
  addFileToTag,
  exportLibraryMetadata,
  queryIndexedFiles,
  removeFileFromTag,
} from "./api";

describe("Phase 2 search and export API", () => {
  it("combines collection, AND tag, text filters, and pagination", async () => {
    await addFileToTag(1, 2);
    await addFileToTag(2, 1);
    await addFileToTag(2, 2);

    try {
      const bothTags = await queryIndexedFiles({ tag_ids: [1, 2] });
      expect(bothTags.total).toBe(2);
      expect(bothTags.files.map((file) => file.id).sort()).toEqual([1, 2]);

      const combined = await queryIndexedFiles({
        query: "Tax",
        collection_id: 1,
        tag_ids: [1, 2],
        status: "indexed",
        extension: "pdf",
        page: 1,
        page_size: 1,
      });
      expect(combined.total).toBe(1);
      expect(combined.files[0].id).toBe(1);
      expect(combined.total_pages).toBe(1);

      const secondPage = await queryIndexedFiles({
        tag_ids: [1, 2],
        page: 2,
        page_size: 1,
      });
      expect(secondPage.total).toBe(2);
      expect(secondPage.files).toHaveLength(1);
      expect(secondPage.page).toBe(2);
    } finally {
      await removeFileFromTag(1, 2);
      await removeFileFromTag(2, 1);
      await removeFileFromTag(2, 2);
    }
  });

  it("exports all Phase 1 and Phase 2 metadata fields", async () => {
    const exportData = await exportLibraryMetadata();

    expect(exportData.format_version).toBe("1.1.0");
    expect(exportData.summary.total_roots).toBe(exportData.scan_roots.length);
    expect(exportData.summary.total_files).toBe(exportData.files.length);
    expect(exportData.summary.total_size_bytes).toBe(
      exportData.files.reduce((sum, file) => sum + file.size_bytes, 0)
    );
    expect(exportData.summary.export_timestamp).toBe(exportData.exported_at);
    expect(exportData.collections.length).toBeGreaterThan(0);
    expect(exportData.tags.length).toBeGreaterThan(0);
    expect(exportData.file_collections.length).toBeGreaterThan(0);
    expect(exportData.file_tags.length).toBeGreaterThan(0);
    expect(exportData.file_notes.length).toBeGreaterThan(0);

    const serialized = JSON.stringify(exportData);
    for (const key of [
      "format_version",
      "scan_roots",
      "files",
      "collections",
      "tags",
      "file_collections",
      "file_tags",
      "file_notes",
    ]) {
      expect(serialized).toContain(`"${key}"`);
    }
  });
});
