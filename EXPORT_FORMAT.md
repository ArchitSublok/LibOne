# LibOne Portable Metadata Export Format Specification

**Format Version**: `1.2.0`
**MIME Type**: `application/json`
**File Extension**: `.json`

---

## 1. Overview & Guarantees

LibOne provides a portable, human-readable JSON export format for library metadata. This specification defines the schema, field types, semantic rules, and forward/backward compatibility guarantees.

### Non-Destructive Privacy Guarantees

- **Metadata Only**: LibOne never stores, embeds, or exports document contents. Exports contain only filesystem metadata (paths, filenames, extensions, byte sizes, timestamps, hashes where computed, indexing status), user-authored organization metadata (collections, tags, links, private notes), and Phase 3 local organization proposals (organization suggestions, suggestion-file links, duplicate groups, duplicate-group files, suggestion status, and review timestamps).
- **User-Authored Notes Only**: `file_notes[].note_text` is written only by the user. No automatically extracted document content is ever placed in a note.
- **Zero Cloud Leakage**: Exports are generated locally on demand and saved directly to the user's chosen local filesystem path.
- **Atomic Persistence**: Serialized JSON is written to a temporary sibling file (`.libone-export-<nanos>.tmp`) and atomically renamed to the final path, preventing partial or corrupt export files.
- **Validation Before Writing**: The complete export model is validated in memory before any bytes are written to disk.

---

## 2. Complete Example

```json
{
  "format_version": "1.2.0",
  "app": "LibOne",
  "version": "0.1.0",
  "exported_at": "2026-11-17T10:00:00.000Z",
  "summary": {
    "total_roots": 1,
    "total_files": 2,
    "total_size_bytes": 1048800,
    "export_timestamp": "2026-11-17T10:00:00.000Z"
  },
  "scan_roots": [
    {
      "id": 1,
      "absolute_path": "D:\\Documents\\Research",
      "display_name": "Research",
      "enabled": true,
      "created_at": "2026-11-10T08:30:00.000Z",
      "last_scan_started_at": "2026-11-17T09:45:00.000Z",
      "last_scan_completed_at": "2026-11-17T09:45:02.000Z",
      "last_scan_status": "completed",
      "file_count": 2
    }
  ],
  "files": [
    {
      "id": 101,
      "scan_root_id": 1,
      "absolute_path": "D:\\Documents\\Research\\quarterly_report.pdf",
      "normalized_path": "d:\\documents\\research\\quarterly_report.pdf",
      "file_name": "quarterly_report.pdf",
      "extension": "pdf",
      "size_bytes": 1048576,
      "modified_at": "2026-11-12T14:20:00.000Z",
      "created_at_if_available": "2026-11-10T09:00:00.000Z",
      "discovered_at": "2026-11-10T09:05:00.000Z",
      "last_seen_at": "2026-11-17T09:45:01.000Z",
      "content_hash_nullable": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "file_id_nullable": "0028a1f4:000000000014ef02",
      "status": "indexed"
    },
    {
      "id": 102,
      "scan_root_id": 1,
      "absolute_path": "D:\\Documents\\Research\\notes.txt",
      "normalized_path": "d:\\documents\\research\\notes.txt",
      "file_name": "notes.txt",
      "extension": "txt",
      "size_bytes": 224,
      "modified_at": "2026-11-13T09:30:00.000Z",
      "created_at_if_available": "2026-11-13T09:25:00.000Z",
      "discovered_at": "2026-11-13T09:45:01.000Z",
      "last_seen_at": "2026-11-17T09:45:01.000Z",
      "content_hash_nullable": null,
      "file_id_nullable": "0028a1f4:000000000014ef03",
      "status": "indexed"
    }
  ],
  "collections": [
    {
      "id": 10,
      "name": "Finance",
      "description_nullable": "Tax and reporting documents",
      "color_nullable": "#10b981",
      "created_at": "2026-11-10T08:35:00.000Z",
      "updated_at": "2026-11-10T08:35:00.000Z"
    }
  ],
  "tags": [
    {
      "id": 20,
      "name": "reviewed",
      "color_nullable": "#f59e0b",
      "created_at": "2026-11-10T08:36:00.000Z"
    },
    {
      "id": 21,
      "name": "important",
      "color_nullable": "#ef4444",
      "created_at": "2026-11-10T08:37:00.000Z"
    }
  ],
  "file_collections": [
    {
      "file_id": 101,
      "collection_id": 10,
      "created_at": "2026-11-10T08:40:00.000Z"
    }
  ],
  "file_tags": [
    {
      "file_id": 101,
      "tag_id": 20,
      "created_at": "2026-11-10T08:41:00.000Z"
    },
    {
      "file_id": 101,
      "tag_id": 21,
      "created_at": "2026-11-10T08:42:00.000Z"
    }
  ],
  "file_notes": [
    {
      "id": 30,
      "file_id": 101,
      "note_text": "Approved by accounting on 2026-11-15.",
      "created_at": "2026-11-15T11:00:00.000Z",
      "updated_at": "2026-11-16T09:30:00.000Z"
    }
  ],
  "organization_suggestions": [
    {
      "id": 1,
      "suggestion_type": "suggest_tag",
      "title": "Tag matching document as 'finance'",
      "rationale": "File resides in 'Research' folder with finance references.",
      "confidence": 0.85,
      "status": "approved",
      "created_at": "2026-11-17T09:50:00.000Z",
      "reviewed_at": "2026-11-17T09:55:00.000Z",
      "affected_file_ids": [101],
      "suggested_tag_id": 20,
      "suggested_tag_name": "reviewed",
      "suggested_collection_id": null,
      "suggested_collection_name": null
    }
  ],
  "suggestion_files": [
    {
      "suggestion_id": 1,
      "file_id": 101
    }
  ],
  "duplicate_groups": [
    {
      "id": 1,
      "match_type": "name_and_size",
      "confidence": 0.75,
      "created_at": "2026-11-17T09:50:00.000Z",
      "status": "pending",
      "file_ids": [101]
    }
  ],
  "duplicate_group_files": [
    {
      "group_id": 1,
      "file_id": 101
    }
  ]
}
```

---

## 3. Schema Field Specifications

### 3.1 Top-Level Object

| Field | Type | Required | Description |
|---|---|---|---|
| `format_version` | String | Yes | Semantic version of this export schema (`"1.2.0"` for this revision). |
| `app` | String | Yes | Identifier of the emitting application (always `"LibOne"`). |
| `version` | String | Yes | Cargo package release version of the LibOne build that produced this export. |
| `exported_at` | String (ISO 8601 UTC) | Yes | Timestamp when the export was initiated. |
| `summary` | Object | Yes | Aggregate statistics describing the exported library snapshot. |
| `scan_roots` | Array of ScanRoot | Yes | User-authorized scan roots recorded in the library. |
| `files` | Array of IndexedFile | Yes | All indexed file records (Phase 1 fields, unchanged). |
| `collections` | Array of Collection | Yes (1.1.0+) | User-created collections. |
| `tags` | Array of Tag | Yes (1.1.0+) | User-created tags. |
| `file_collections` | Array of FileCollectionLink | Yes (1.1.0+) | Many-to-many links between files and collections. |
| `file_tags` | Array of FileTagLink | Yes (1.1.0+) | Many-to-many links between files and tags. |
| `file_notes` | Array of FileNote | Yes (1.1.0+) | User-authored private notes. |
| `organization_suggestions` | Array of OrganizationSuggestion | Yes (1.2.0+) | Local pattern and organization suggestions with review status and timestamps. |
| `suggestion_files` | Array of SuggestionFileLink | Yes (1.2.0+) | Associations between organization suggestions and affected indexed files. |
| `duplicate_groups` | Array of DuplicateGroup | Yes (1.2.0+) | Detected duplicate groups with match type, confidence, and status. |
| `duplicate_group_files` | Array of DuplicateGroupFileLink | Yes (1.2.0+) | Associations between duplicate groups and member indexed files. |

### 3.2 `summary` Object

| Field | Type | Description |
|---|---|---|
| `total_roots` | Integer | Total count of scan roots in `scan_roots`. |
| `total_files` | Integer | Total count of files in `files`. |
| `total_size_bytes` | Integer | Sum of all `size_bytes` across the `files` array. |
| `export_timestamp` | String (ISO 8601 UTC) | Timestamp matching top-level `exported_at`. |

### 3.3 `scan_roots[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the scan root. |
| `absolute_path` | String | Native filesystem directory path. |
| `display_name` | String | User-facing folder name. |
| `enabled` | Boolean | Whether folder was actively monitored/scanned at export time. |
| `created_at` | String (ISO 8601) | When the folder was added to LibOne. |
| `last_scan_started_at` | String or `null` | Start timestamp of the most recent scan. |
| `last_scan_completed_at` | String or `null` | Completion timestamp of the most recent scan. |
| `last_scan_status` | String | Final status (`completed`, `cancelled`, `error`, `never_scanned`). |
| `file_count` | Integer | Number of non-missing files indexed in this root. |

### 3.4 `files[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the file record. |
| `scan_root_id` | Integer | Foreign key linking to `scan_roots.id`. |
| `absolute_path` | String | Exact path to the document on the host filesystem. |
| `normalized_path` | String | Lowercase backslash-normalized path used for lookups. |
| `file_name` | String | File basename including extension. |
| `extension` | String | Extension without leading dot (lowercase). |
| `size_bytes` | Integer | File size in bytes reported by the filesystem. |
| `modified_at` | String or `null` | Last modified timestamp from filesystem metadata. |
| `created_at_if_available` | String or `null` | Creation timestamp if supported by the host volume. |
| `discovered_at` | String (ISO 8601) | Timestamp when LibOne first discovered the file. |
| `last_seen_at` | String (ISO 8601) | Timestamp when LibOne last confirmed the file on disk. |
| `content_hash_nullable` | String or `null` | Hex-encoded SHA-256 digest if computed; `null` otherwise. |
| `file_id_nullable` | String or `null` | Win32 volume serial and file index if on NTFS; `null` otherwise. |
| `status` | String | `indexed`, `changed`, `missing`, `inaccessible`, or `error`. |

### 3.5 `collections[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the collection. |
| `name` | String | User-provided collection name. |
| `description_nullable` | String or `null` | Optional description. |
| `color_nullable` | String or `null` | Optional display color. |
| `created_at` | String (ISO 8601) | Creation timestamp. |
| `updated_at` | String (ISO 8601) | Last update timestamp. |

### 3.6 `tags[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the tag. |
| `name` | String | User-provided tag name. |
| `color_nullable` | String or `null` | Optional display color. |
| `created_at` | String (ISO 8601) | Creation timestamp. |

### 3.7 `file_collections[]` Object

| Field | Type | Description |
|---|---|---|
| `file_id` | Integer | Foreign key to `files.id`. |
| `collection_id` | Integer | Foreign key to `collections.id`. |
| `created_at` | String (ISO 8601) | When the link was created. |

### 3.8 `file_tags[]` Object

| Field | Type | Description |
|---|---|---|
| `file_id` | Integer | Foreign key to `files.id`. |
| `tag_id` | Integer | Foreign key to `tags.id`. |
| `created_at` | String (ISO 8601) | When the link was created. |

### 3.9 `file_notes[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the note. |
| `file_id` | Integer | Foreign key to `files.id`. |
| `note_text` | String | User-authored note text; never extracted document content. |
| `created_at` | String (ISO 8601) | Creation timestamp. |
| `updated_at` | String (ISO 8601) | Last edit timestamp. |

### 3.10 `organization_suggestions[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the suggestion. |
| `suggestion_type` | String | Category (`duplicate_files`, `likely_version`, `suggest_collection`, `suggest_tag`, `stale_file`). |
| `title` | String | User-facing descriptive title. |
| `rationale` | String | Deterministic heuristic explanation for the suggestion. |
| `confidence` | Float | Confidence score between `0.0` and `1.0`. |
| `status` | String | Workflow status (`pending`, `approved`, `rejected`, `dismissed`). |
| `created_at` | String (ISO 8601) | When the suggestion was generated. |
| `reviewed_at` | String or `null` | Timestamp when user explicitly approved, rejected, or dismissed the suggestion. |
| `affected_file_ids` | Array of Integer | IDs of indexed files relevant to this suggestion. |
| `suggested_tag_id` | Integer or `null` | Target tag ID if suggestion relates to an existing tag. |
| `suggested_tag_name` | String or `null` | Target tag name if creating a new tag. |
| `suggested_collection_id` | Integer or `null` | Target collection ID if suggestion relates to an existing collection. |
| `suggested_collection_name` | String or `null` | Target collection name if creating a new collection. |

### 3.11 `suggestion_files[]` Object

| Field | Type | Description |
|---|---|---|
| `suggestion_id` | Integer | Foreign key to `organization_suggestions.id`. |
| `file_id` | Integer | Foreign key to `files.id`. |

### 3.12 `duplicate_groups[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the duplicate cluster. |
| `match_type` | String | Detection heuristic (`exact_hash`, `name_and_size`, `file_identity`). |
| `confidence` | Float | Confidence score between `0.0` and `1.0`. |
| `created_at` | String (ISO 8601) | When the duplicate group was formed. |
| `status` | String | Review status (`pending`, `approved`, `rejected`, `dismissed`). |
| `file_ids` | Array of Integer | IDs of member indexed files. |

### 3.13 `duplicate_group_files[]` Object

| Field | Type | Description |
|---|---|---|
| `group_id` | Integer | Foreign key to `duplicate_groups.id`. |
| `file_id` | Integer | Foreign key to `files.id`. |

---

## 4. Semantic Compatibility Rules

LibOne follows Semantic Versioning (`MAJOR.MINOR.PATCH`) for the metadata format.

1. **Same Major Version (`1.x.y`)**:
   - Consumers MUST ignore unknown fields.
   - `1.1.0` added `collections`, `tags`, `file_collections`, `file_tags`, and `file_notes`.
   - `1.2.0` adds `organization_suggestions`, `suggestion_files`, `duplicate_groups`, and `duplicate_group_files`. Readers of older exports (`1.0.0` or `1.1.0`) must treat missing arrays as empty.
   - No Phase 1 or Phase 2 field is removed or renamed, and existing field types remain stable.

2. **Breaking Changes (`2.0.0`)**:
   - Field removal, renaming, or type changes require a major version bump.
   - Consumers should inspect `format_version` before ingesting. If the major version is unsupported, halt or warn the user.

3. **Validation Requirements**:
   A valid export MUST satisfy all of the following:
   - `format_version`, `app`, `version`, and `exported_at` are non-empty strings.
   - `summary.total_roots == scan_roots.length`.
   - `summary.total_files == files.length`.
   - `summary.total_size_bytes == sum(files[i].size_bytes)`.
   - `summary.export_timestamp == exported_at`.
   - All IDs referenced by files, links, notes, suggestions, and duplicate groups exist in their corresponding arrays; link pairs and note IDs are unique.
   - Collection/tag names, note text, suggestion titles/types, and match types are non-empty.
   - Suggestion and duplicate group confidence values are within `0.0` to `1.0`.
   - Suggestion and duplicate group statuses are valid workflow states (`pending`, `approved`, `rejected`, `dismissed`).

---

## 5. Portability & Interoperability

- **Standard Tools**: Valid UTF-8 JSON with indentation can be processed by `jq`, Python (`json.load`), PowerShell (`ConvertFrom-Json`), Node.js, and data science notebooks.
- **Offline Archiving**: Users can archive exports alongside cold-storage backups or audit library metadata without the live SQLite database.
- **No Proprietary Encodings**: Dates are ISO 8601 UTC strings, byte sizes are 64-bit integers, and boolean flags are literal `true`/`false`.

---

## 6. Integration Review Notes

- The exporter reads only SQLite rows. It never opens an original document to read bytes, and it does not call any text extraction, OCR, or AI component.
- `file_notes[].note_text` is copied verbatim from user-authored notes. Because notes can contain sensitive user text, treat exported JSON files as private data and store them accordingly.
- Link timestamps use the `created_at` key (not `added_at`). The `summary` object intentionally keeps only the Phase 1 counters; consumers should derive collection/tag/note counts from the corresponding arrays.
- Validation is performed in memory before any write. A failed validation leaves any pre-existing target file unchanged because no temporary file is created.
