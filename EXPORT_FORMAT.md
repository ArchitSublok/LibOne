# LibOne Portable Metadata Export Format Specification

**Format Version**: `1.0.0`  
**MIME Type**: `application/json`  
**File Extension**: `.json`

---

## 1. Overview & Guarantees

LibOne provides a portable, human-readable JSON export format for library metadata. This specification defines the schema, field types, semantic rules, and forward/backward compatibility guarantees.

### Non-Destructive Privacy Guarantees
- **Metadata Only**: LibOne never stores, embeds, or exports document contents. Exports contain only filesystem metadata (file paths, filenames, extensions, byte sizes, modification timestamps, hashes where computed, and indexing status).
- **Zero Cloud Leakage**: Exports are generated locally on-demand and saved directly to the user's chosen local filesystem path via a native save-file dialog.
- **Atomic Persistence**: Serialized JSON is written to a temporary sibling file (`.libone-export-<nanos>.tmp`) and atomically renamed to the final path, preventing partial or corrupt export files in case of system power loss or interruption.
- **Validation Before Writing**: Export models are structurally validated in-memory before any bytes are written to disk.

---

## 2. Complete Example

```json
{
  "format_version": "1.0.0",
  "app": "LibOne",
  "version": "0.1.0",
  "exported_at": "2026-11-13T10:00:00.000Z",
  "summary": {
    "total_roots": 1,
    "total_files": 2,
    "total_size_bytes": 1048800,
    "export_timestamp": "2026-11-13T10:00:00.000Z"
  },
  "scan_roots": [
    {
      "id": 1,
      "absolute_path": "D:\\Documents\\Research",
      "display_name": "Research",
      "enabled": true,
      "created_at": "2026-11-10T08:30:00.000Z",
      "last_scan_started_at": "2026-11-13T09:45:00.000Z",
      "last_scan_completed_at": "2026-11-13T09:45:02.000Z",
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
      "last_seen_at": "2026-11-13T09:45:01.000Z",
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
      "last_seen_at": "2026-11-13T09:45:01.000Z",
      "content_hash_nullable": null,
      "file_id_nullable": "0028a1f4:000000000014ef03",
      "status": "indexed"
    }
  ]
}
```

---

## 3. Schema Field Specifications

### 3.1 Top-Level Object

| Field | Type | Required | Description |
|---|---|---|---|
| `format_version` | String | Yes | Semantic version of this export schema (e.g. `"1.0.0"`). |
| `app` | String | Yes | Identifier of the emitting application (always `"LibOne"`). |
| `version` | String | Yes | Cargo package release version of the LibOne build that produced this export. |
| `exported_at` | String (ISO 8601 UTC) | Yes | Timestamp when the export was initiated. |
| `summary` | Object | Yes | Aggregate statistics describing the exported library snapshot. |
| `scan_roots` | Array of ScanRoot | Yes | List of user-authorized scan roots recorded in the library. |
| `files` | Array of IndexedFile | Yes | Complete list of all indexed regular files recorded in the library. |

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
| `last_scan_status` | String | Final status of the last scan (`completed`, `cancelled`, `error`, `never_scanned`). |
| `file_count` | Integer | Number of non-missing files indexed in this root. |

### 3.4 `files[]` Object

| Field | Type | Description |
|---|---|---|
| `id` | Integer | Local database ID of the file record. |
| `scan_root_id` | Integer | Foreign key linking to parent `scan_roots.id`. |
| `absolute_path` | String | Exact path to the document file on the host filesystem. |
| `normalized_path` | String | Lowercase backslash-normalized path used for fast lookups. |
| `file_name` | String | File basename including extension. |
| `extension` | String | File extension without leading dot (lowercase). |
| `size_bytes` | Integer | File size in bytes as reported by the filesystem. |
| `modified_at` | String or `null` | Last modified timestamp from filesystem metadata. |
| `created_at_if_available` | String or `null` | Creation timestamp if supported by the host volume. |
| `discovered_at` | String (ISO 8601) | Timestamp when LibOne first discovered this file. |
| `last_seen_at` | String (ISO 8601) | Timestamp when LibOne last confirmed this file on disk. |
| `content_hash_nullable` | String or `null` | Hex-encoded SHA-256 digest if computed; `null` otherwise. |
| `file_id_nullable` | String or `null` | Win32 volume serial and 64-bit file index (`"{vol:08x}:{idx:016x}"`) if on NTFS; `null` on FAT32/exFAT. |
| `status` | String | Document status: `indexed`, `changed`, `missing`, `inaccessible`, `error`. |

---

## 4. Semantic Compatibility Rules

LibOne follows Semantic Versioning (`MAJOR.MINOR.PATCH`) for the metadata format:

1. **Backwards Compatibility (Same Major Version `1.x.y`)**:
   - Consumers reading `1.x.y` MUST ignore unknown fields.
   - Future `1.x.y` revisions may add optional keys to objects or new arrays, but will NEVER remove or rename existing fields.
   - Types of existing fields (`string`, `integer`, `boolean`, `null`) will remain stable.

2. **Breaking Changes (Major Version Bump `2.0.0`)**:
   - Any structural re-architecture, field removal, or semantic type changes will increment the major version number.
   - Tools parsing LibOne exports should inspect `format_version` before ingesting. If the major version is greater than what the consumer supports, the consumer should halt or warn the user.

3. **Validation Requirements**:
   - A valid export MUST satisfy:
     - `summary.total_roots == scan_roots.length`
     - `summary.total_files == files.length`
     - `format_version`, `app`, `version`, `exported_at` are non-empty strings.
     - `summary.total_size_bytes` equals the sum of `files[i].size_bytes`.

---

## 5. Portability & Interoperability

- **Standard Tools**: Because the export is valid UTF-8 formatted JSON with indentation, it can be processed directly by `jq`, Python (`json.load`), PowerShell (`ConvertFrom-Json`), Node.js, and data science notebooks.
- **Migration & Offline Archiving**: Users can archive export JSON files alongside cold-storage backups or use them to audit library contents without needing access to the live SQLite database.
- **No Proprietary Encodings**: Dates use standard ISO 8601 UTC strings (`YYYY-MM-DDTHH:MM:SS.sssZ`), byte sizes are unformatted 64-bit integers, and boolean flags are literal `true`/`false`.
