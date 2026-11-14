# LibOne Phase 2 Implementation Plan

**Objective**: Extend LibOne from its verified Phase 1 local file-tracking foundation into an organized, private document library by introducing **Collections**, **Tags**, **File Associations**, **Private Notes**, **Combined Metadata Filtering**, and **Enhanced Metadata Export**.

---

## 1. Architectural Guardrails & Preserved Invariants

All Phase 1 safety guarantees remain non-negotiable:
- **Local-Only SQLite**: No cloud services, accounts, telemetry, analytics, or remote transmission.
- **Strict Read-Only Disk Operations**: Original document files on disk are **never** moved, renamed, written to, deleted, or modified by LibOne.
- **No Document Content Extraction**: Full-text parsing, OCR, and AI document indexing are **out of scope** for Phase 2. Indexing operates strictly on filesystem metadata and user-created metadata.
- **Stable Identity Preservation**: NTFS file ID tracking (`BY_HANDLE_FILE_INFORMATION`) preserves file identity across renames/moves within a scan root; associated collections, tags, and notes stay attached to the file record.
- **Non-Destructive Operations**: Removing a collection, tag, or note never touches files on disk. Disabling/removing scan roots leaves collections and tags intact.
- **Safe Launching & Security**: Path authorization and non-shell launching (`ShellExecuteW` / `explorer.exe /select`) continue protecting file access.

---

## 2. Current State Assessment

- **Current SQLite Schema Version**: `2`
  - Version 1: `scan_roots`, `indexed_files`, `library_metadata`, `schema_migrations`.
  - Version 2: Added `file_id_nullable` column and `idx_indexed_files_file_id` index.
- **Current APIs**: 20 Tauri IPC commands handling scan-root lifecycle, real-time scanning with progress events, file querying, file access authorization, file hashing, background watcher status, and format v1.0.0 JSON export.
- **Frontend**: React 19 + TypeScript + Vite with sidebar for scan roots, file table with multi-column sorting/pagination, document detail modal, scanner view, settings modal, and export preview.

---

## 3. Database Migration Plan (Schema Version 3)

The migration executes inside a single SQLite transaction in `src-tauri/src/db.rs` when `current < 3`.

### 3.1 Migration DDL

```sql
-- 1. Collections: user-defined organizational groups
CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT,
    color TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- 2. Tags: lightweight labels
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT,
    created_at TEXT NOT NULL
);

-- 3. File-to-Collection associations (many-to-many)
CREATE TABLE IF NOT EXISTS file_collections (
    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL,
    PRIMARY KEY (file_id, collection_id)
);

-- 4. File-to-Tag associations (many-to-many)
CREATE TABLE IF NOT EXISTS file_tags (
    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL,
    PRIMARY KEY (file_id, tag_id)
);

-- 5. Private Notes: per-file user notes (1-to-1)
CREATE TABLE IF NOT EXISTS file_notes (
    file_id INTEGER PRIMARY KEY REFERENCES indexed_files(id) ON DELETE CASCADE,
    note_text TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Indexes for fast query joins and filtering
CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
CREATE INDEX IF NOT EXISTS idx_file_collections_collection ON file_collections(collection_id);
CREATE INDEX IF NOT EXISTS idx_file_collections_file ON file_collections(file_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_tag ON file_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_file_tags_file ON file_tags(file_id);

-- Version update
UPDATE library_metadata SET schema_version = 3, updated_at = CURRENT_TIMESTAMP WHERE id = 1;
INSERT INTO schema_migrations (version, applied_at) VALUES (3, ?);
```

### 3.2 Migration Safety & Compatibility
- **Foreign Keys**: `PRAGMA foreign_keys = ON;` ensures cascading deletes if a collection or tag is deleted; file deletion or record cleanup cascades cleanly to link tables and notes.
- **Idempotency**: All `CREATE TABLE` and `CREATE INDEX` use `IF NOT EXISTS`.
- **Row Identity**: File moves/renames handled in `db.move_indexed_file` update `indexed_files` by `id`, leaving `file_id` references in `file_collections`, `file_tags`, and `file_notes` unbroken.

---

## 4. Rust Backend Repository & Models

### 4.1 Models (`src-tauri/src/models.rs`)

```rust
// ── Collections ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionWithCount {
    pub collection: Collection,
    pub file_count: usize,
}

// ── Tags ────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagWithCount {
    pub tag: Tag,
    pub file_count: usize,
}

// ── Private Notes ───────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNote {
    pub file_id: i64,
    pub note_text: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── Extended File Models ────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileQueryParams {
    pub query: Option<String>,
    pub root_id: Option<i64>,
    pub extension: Option<String>,
    pub status: Option<String>,
    pub collection_id: Option<i64>,     // NEW: filter by collection
    pub tag_ids: Option<Vec<i64>>,       // NEW: filter by tags (AND match)
    pub has_notes: Option<bool>,         // NEW: filter by presence of notes
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDetail {
    pub file: IndexedFile,
    pub root_display_name: String,
    pub root_absolute_path: String,
    pub exists_on_disk: bool,
    pub collections: Vec<Collection>,    // NEW
    pub tags: Vec<Tag>,                  // NEW
    pub note: Option<FileNote>,          // NEW
}
```

### 4.2 Database Layer (`src-tauri/src/db.rs`)

Implement repository functions:
- **Collections**:
  - `create_collection(name, description, color) -> Result<Collection>`
  - `update_collection(id, name, description, color) -> Result<Collection>`
  - `delete_collection(id) -> Result<()>`
  - `list_collections() -> Result<Vec<CollectionWithCount>>`
  - `get_collection(id) -> Result<Option<Collection>>`
- **Tags**:
  - `create_tag(name, color) -> Result<Tag>`
  - `update_tag(id, name, color) -> Result<Tag>`
  - `delete_tag(id) -> Result<()>`
  - `list_tags() -> Result<Vec<TagWithCount>>`
  - `get_tag(id) -> Result<Option<Tag>>`
- **File Associations**:
  - `add_file_to_collection(file_id, collection_id) -> Result<()>`
  - `remove_file_from_collection(file_id, collection_id) -> Result<()>`
  - `set_file_collections(file_id, collection_ids) -> Result<()>`
  - `get_file_collections(file_id) -> Result<Vec<Collection>>`
  - `add_tag_to_file(file_id, tag_id) -> Result<()>`
  - `remove_tag_from_file(file_id, tag_id) -> Result<()>`
  - `set_file_tags(file_id, tag_ids) -> Result<()>`
  - `get_file_tags(file_id) -> Result<Vec<Tag>>`
- **Notes**:
  - `get_file_note(file_id) -> Result<Option<FileNote>>`
  - `set_file_note(file_id, note_text) -> Result<FileNote>`
  - `delete_file_note(file_id) -> Result<()>`
- **Combined Query (`query_files`)**:
  - Join `file_collections` when `params.collection_id` is set.
  - Join `file_tags` when `params.tag_ids` is set (grouping with `HAVING COUNT(DISTINCT tag_id) = ?` for all-tag matching).
  - Left join `file_notes` when `params.has_notes` is set.
  - Multi-field substring search: when `params.query` is present, match across `file_name`, `absolute_path`, `extension`, tag names (via subquery), and `file_notes.note_text`.

---

## 5. Tauri IPC Command Layer

### 5.1 New Commands (`src-tauri/src/commands.rs`)

| Command | Arguments | Returns | Description |
|---|---|---|---|
| `create_collection` | `name: String, description: Option<String>, color: Option<String>` | `Collection` | Creates collection (fails on duplicate name). |
| `update_collection` | `id: i64, name: String, description: Option<String>, color: Option<String>` | `Collection` | Updates collection name/desc/color. |
| `delete_collection` | `id: i64` | `()` | Deletes collection and associated links. |
| `list_collections` | None | `Vec<CollectionWithCount>` | Lists collections with file counts. |
| `create_tag` | `name: String, color: Option<String>` | `Tag` | Creates tag (fails on duplicate name). |
| `update_tag` | `id: i64, name: String, color: Option<String>` | `Tag` | Updates tag name/color. |
| `delete_tag` | `id: i64` | `()` | Deletes tag and associated links. |
| `list_tags` | None | `Vec<TagWithCount>` | Lists tags with file counts. |
| `set_file_collections` | `file_id: i64, collection_ids: Vec<i64>` | `()` | Replaces file's collections. |
| `set_file_tags` | `file_id: i64, tag_ids: Vec<i64>` | `()` | Replaces file's tags. |
| `get_file_note` | `file_id: i64` | `Option<FileNote>` | Retrieves note for file. |
| `set_file_note` | `file_id: i64, note_text: String` | `FileNote` | Saves or updates note. |
| `delete_file_note` | `file_id: i64` | `()` | Clears note for file. |

### 5.2 Command Registration
Register all 13 new commands in `src-tauri/src/lib.rs` inside `tauri::generate_handler![...]`.

---

## 6. Frontend Types, API Client & UI Design

### 6.1 TypeScript Types (`src/types.ts`)
- Add interfaces: `Collection`, `CollectionWithCount`, `Tag`, `TagWithCount`, `FileNote`.
- Extend `FileQueryParams` with `collection_id?: number | null`, `tag_ids?: number[] | null`, `has_notes?: boolean | null`.
- Extend `FileDetail` with `collections: Collection[]`, `tags: Tag[]`, `note: FileNote | null`.
- Extend `IndexedFile` (or UI view item) with tag badges and note indicators.

### 6.2 API Service (`src/services/api.ts`)
- Implement IPC wrappers for all new commands.
- Provide mock data implementations in dev/browser mode for seamless development testing.

### 6.3 UI Components & Interactions

1. **Sidebar Navigation**:
   - Add a collapsible **Collections** section below Scan Roots.
     - "All Documents" item (resets collection filter).
     - List of collections with color dot and file count badge.
     - "+ New Collection" button triggering creation modal.
   - Add a collapsible **Tags** section.
     - Tag pill cloud with count badges.
     - Clicking a tag filters the file view by that tag.
     - "+ New Tag" inline or modal creator.

2. **File List & Filters**:
   - Active filter indicators showing selected Collection and Tag filters with one-click "clear".
   - File table rows display tag pills (up to 3 + "+N" overflow) and a small note icon if a note exists.
   - Enhanced search bar: queries filenames, paths, tags, and notes with debounced input.

3. **Document Detail Modal (`FileDetailModal.tsx`)**:
   - **Tags Section**: Displays assigned tags as removable pills; searchable dropdown to select existing tags or create new tags on the fly.
   - **Collections Section**: Checkbox or multi-select dropdown for assigning the document to one or more collections.
   - **Private Note Section**: Multi-line markdown-friendly text area with "Save Note" and "Delete Note" actions, showing last modified timestamp.

4. **Collection & Tag Management Modals**:
   - Create/Edit Collection Modal (Name, Description, Color picker from a calm palette).
   - Create/Edit Tag Modal (Name, Color picker).
   - Delete confirmation dialog explaining that only the tag/collection is deleted, not documents.

---

## 7. Metadata Export Specification (Format v1.1.0)

Exports increment from `1.0.0` to `1.1.0` following semantic versioning rules:

```json
{
  "format_version": "1.1.0",
  "app": "LibOne",
  "version": "0.2.0",
  "exported_at": "2026-11-14T12:00:00.000Z",
  "summary": {
    "total_roots": 1,
    "total_files": 12,
    "total_collections": 3,
    "total_tags": 5,
    "total_notes": 4,
    "total_size_bytes": 1048800,
    "export_timestamp": "2026-11-14T12:00:00.000Z"
  },
  "scan_roots": [ ... ],
  "files": [ ... ],
  "collections": [
    {
      "id": 1,
      "name": "Tax 2025",
      "description": "All relevant invoices and receipts",
      "color": "#3b82f6",
      "created_at": "2026-11-14T10:00:00.000Z",
      "updated_at": "2026-11-14T10:00:00.000Z"
    }
  ],
  "tags": [
    {
      "id": 1,
      "name": "receipt",
      "color": "#10b981",
      "created_at": "2026-11-14T10:00:00.000Z"
    }
  ],
  "file_collections": [
    { "file_id": 101, "collection_id": 1, "added_at": "2026-11-14T10:05:00.000Z" }
  ],
  "file_tags": [
    { "file_id": 101, "tag_id": 1, "added_at": "2026-11-14T10:05:00.000Z" }
  ],
  "file_notes": [
    {
      "file_id": 101,
      "note_text": "Deductible business expense from Office Depot",
      "created_at": "2026-11-14T10:10:00.000Z",
      "updated_at": "2026-11-14T10:10:00.000Z"
    }
  ]
}
```

- **Validation**:
  - `summary.total_collections == collections.len()`
  - `summary.total_tags == tags.len()`
  - `summary.total_notes == file_notes.len()`
  - Enforce referential integrity in memory before writing.

---

## 8. Automated & Manual Test Plan

### 8.1 Automated Rust Tests

Add dedicated test modules or tests in `src-tauri/src/db.rs` and `src-tauri/src/commands.rs`:

1. **Migration Verification**:
   - `test_migration_v2_to_v3`: Start database with schema v2, run `migrate()`, assert `schema_version == 3` and all 5 new tables exist with correct foreign keys.
2. **Collection Operations**:
   - `test_collection_crud`: Create, update name/description, delete, assert cascade.
   - `test_collection_name_uniqueness_nocase`: Duplicate names with different case fail.
3. **Tag Operations**:
   - `test_tag_crud`: Create, update, delete, assert cascade.
   - `test_tag_name_uniqueness_nocase`: Duplicate tag names fail.
4. **File Associations**:
   - `test_file_collections_link_and_unlink`: Add to collections, list collections for file, set collections, remove.
   - `test_file_tags_link_and_unlink`: Add tags to file, set tags, list tags for file, remove.
5. **Private Notes**:
   - `test_file_note_lifecycle`: Set note, get note, update note (verifying `updated_at`), delete note.
6. **Rename/Move Identity Preservation**:
   - `test_rename_preserves_collections_tags_notes`: Move an indexed file; verify that associated collections, tags, and notes stay attached to the updated record ID.
7. **Combined Metadata Filtering & Search**:
   - `test_query_files_by_collection`: Returns only files in target collection.
   - `test_query_files_by_tag`: Returns only files with target tag(s).
   - `test_query_files_by_note_presence`: Returns only files with notes.
   - `test_query_files_search_across_metadata`: Search keyword matches note text or tag name and returns the file.
8. **Export Format v1.1.0 Validation & Roundtrip**:
   - `test_export_v1_1_0_roundtrip`: Serializes library with collections, tags, links, and notes; validates structure, counts, and deserialization.

### 8.2 Frontend Automated Verification
- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo test --manifest-path src-tauri\Cargo.toml --lib`
- `npm run build` (`tsc && vite build`)

### 8.3 Manual Test Plan Matrix

| Case ID | Scenario | Steps | Expected Outcome |
|---|---|---|---|
| M2-01 | Create & Manage Collection | Click "+ New Collection", enter "Tax 2025", select blue color, save. Edit description, save. | Collection appears in sidebar with 0 files. Edit updates details immediately. |
| M2-02 | Assign Files to Collection | Select a file, open Detail Modal, assign to "Tax 2025". Filter by "Tax 2025" in sidebar. | File appears in filtered list; collection file count updates to 1. Original file on disk untouched. |
| M2-03 | Create & Apply Tags | Open Detail Modal, create tag "receipt", apply to file. Close modal. | Tag pill appears in file row and detail modal. Tag list in sidebar shows count 1. |
| M2-04 | Add Private Note | Open Detail Modal, write "Deductible business expense" in Private Note, click Save. | Note is persisted. Small note indicator icon appears on the file table row. |
| M2-05 | Combined Metadata Search | Enter "Deductible" in search input while filtered by "Tax 2025". | Target document matches and appears; non-matching documents are filtered out. |
| M2-06 | Move File Preservation | Rename document in Windows File Explorer. Run Scan or let Watcher sync. | File status updates; collections, tags, and private note remain attached to the file. |
| M2-07 | Delete Collection / Tag Safety | Delete "Tax 2025" collection. | Collection disappears from sidebar. Original document on disk and scan root index remain intact. |
| M2-08 | Export v1.1.0 Snapshot | Click Export Metadata, save JSON, inspect file. | JSON has `format_version: "1.1.0"`, includes `collections`, `tags`, `file_collections`, `file_tags`, and `file_notes`. |

---

## 9. Next Steps & Implementation Sequence

1. **Step 1: Database Migration & Models (Rust)**:
   - Update `models.rs` with `Collection`, `Tag`, `FileNote`, extended `FileQueryParams`, and extended `FileDetail`.
   - Update `db.rs` with schema migration v3 and repository CRUD/join methods.
   - Add unit tests for database methods.
2. **Step 2: Tauri Commands & IPC (Rust)**:
   - Add IPC commands in `commands.rs`.
   - Update `export_library_metadata` and validation in `commands.rs` for format v1.1.0.
   - Register commands in `lib.rs`.
   - Add unit tests in `commands.rs`.
3. **Step 3: Frontend Types & API Client (TypeScript)**:
   - Update `src/types.ts` and `src/services/api.ts` with Phase 2 types, commands, and mock data.
4. **Step 4: Frontend UI Components (React)**:
   - Sidebar Collections & Tags sections.
   - Modal management dialogs (Collection, Tag).
   - Document Detail Modal enhancements (Tags picker, Collections picker, Note editor).
   - File table indicators and combined filter controls.
5. **Step 5: Full Verification**:
   - `cargo test --manifest-path src-tauri\Cargo.toml --lib`
   - `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
   - `npm run build`
