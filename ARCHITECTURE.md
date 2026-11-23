# LibOne Architecture & Phase 1 Foundation

LibOne is a local-first desktop document library for Windows. It provides a non-destructive library layer over documents where they already live, indexing them in place without moving, renaming, altering, or uploading them.

---

## 1. System Overview & Technology Stack

- **Desktop Shell**: [Tauri 2](https://v2.tauri.app/) (Rust core with Windows WebView2 integration)
- **Frontend**: React 19 + TypeScript + Vite
- **Backend / Engine**: Rust native application layer
- **Local Persistence**: Embedded SQLite (`rusqlite` WAL mode) stored in Windows AppData
- **Primary OS Target**: Windows 10 / 11 (64-bit)

---

## 2. Folder Structure

```text
LibOne/
├── .gitignore                    # Ignores Node, Rust target, Tauri gen, and local SQLite data
├── README.md                     # Product overview, core workflow, and development guide
├── ARCHITECTURE.md               # Architecture, data flow, boundaries, and data locations
├── index.html                    # Vite entry HTML
├── package.json                  # Frontend dependencies and dev scripts
├── tsconfig.json                 # TypeScript compiler configuration
├── tsconfig.node.json            # Vite build TypeScript config
├── vite.config.ts                # Vite config tailored for Tauri IPC and HMR
├── public/                       # Static public assets (icons, logos)
├── src/                          # Frontend React/TypeScript application
│   ├── assets/                   # UI assets
│   ├── services/
│   │   └── api.ts                # Typed Tauri IPC wrappers with browser dev fallbacks
│   ├── types.ts                  # Shared data interfaces (ScanRoot, IndexedFile, etc.)
│   ├── App.css                   # Calm, local-first Windows desktop design system
│   ├── App.tsx                   # Main desktop application interface (Phase 1)
│   ├── main.tsx                  # React DOM root entry
│   └── vite-env.d.ts             # Vite client types
└── src-tauri/                    # Rust native backend
    ├── Cargo.toml                # Rust dependencies (tauri, rusqlite, walkdir, sha2, chrono)
    ├── build.rs                  # Tauri build hook
    ├── tauri.conf.json           # Window configuration, capabilities, and plugin bindings
    ├── capabilities/
    │   └── default.json          # Security capabilities (core, opener, dialog)
    ├── icons/                    # Windows application icons (ico, png)
    └── src/
        ├── main.rs               # Win32 binary entry point
        ├── lib.rs                # Tauri runtime initialization, state, and command bindings
        ├── models.rs             # Core data structures (ScanRoot, IndexedFile, ScanStats, Export)
        ├── db.rs                 # SQLite schema initialization, migrations, and CRUD operations
        ├── scanner.rs            # Recursive in-place file scanner and change detection
        └── commands.rs           # Tauri IPC commands invoked by the React UI
```

---

## 3. Local Data Location

LibOne never stores your documents in an opaque vault or uploads them to any server. File metadata and scan configurations are stored strictly on the local machine:

- **Windows Standard Path**:  
  `%APPDATA%\com.libone.app\libone.db`  
  (Resolves dynamically via Tauri's `app.path().app_data_dir()`)
- **Storage Engine**: SQLite in Write-Ahead Logging (`WAL`) mode with `NORMAL` synchronous writes for crash resilience and zero file locking.
- **Migrations**: The Rust `Database` repository applies versioned migrations inside a single SQLite transaction. `schema_migrations` records the applied version; `library_metadata` stores the library's schema version and creation time. A legacy pre-migration schema is renamed and copied forward rather than discarded.
- **Tables**:
  - `scan_roots`: user-selected absolute and normalized paths, display name, enabled state, creation time, and scan lifecycle timestamps/status.
  - `indexed_files`: metadata only—absolute/normalized path, file name/type, size, available filesystem dates, discovery/last-seen dates, optional hash, and lifecycle status. Original file contents are never copied into SQLite.
  - `library_metadata`: one local-library record containing schema and creation information.
  - Phase 2 (`schema_version = 3`, additive migration): `collections` (`id`, `name`, `description_nullable`, `color_nullable`, `created_at`, `updated_at`), `tags` (`id`, `name`, `color_nullable`, `created_at`), `file_collections` (`file_id`, `collection_id`, `created_at`), `file_tags` (`file_id`, `tag_id`, `created_at`), `file_notes` (`id`, `file_id`, `note_text`, `created_at`, `updated_at`). Notes are one-to-many per file; all link tables use `ON DELETE CASCADE` only when the metadata record itself is deleted.

---

## 4. End-to-End Data Flow

```mermaid
flowchart LR
    subgraph UI ["Frontend (React + TypeScript)"]
        A[User Selects Folders] --> B[Browse / Search / Filter]
        C[Trigger Scan / Re-scan] --> D[View Change Deltas]
        E[Click 'Open'] --> F[Export Metadata to JSON]
    end

    subgraph IPC ["Tauri 2 IPC Bridge"]
        G[Tauri Command Invocation]
    end

    subgraph Rust ["Rust Native Engine"]
        H[commands.rs]
        I[scanner.rs]
        J[Default Windows Shell Execute]
    end

    subgraph Storage ["Local Storage & Disk"]
        K[(SQLite: libone.db)]
        L[Original Documents on PC Disk]
    end

    A --> G
    C --> G
    E --> G
    F --> G
    G --> H
    H --> I
    H --> J
    I -->|Read-only metadata scan| L
    I -->|Batch update deltas| K
    H -->|Query metadata| K
    J -->|ShellExecuteW| L
    H -->|Deliver results| G
    G --> UI
```

1. **Folder Selection**: User chooses folders via native Windows folder picker (`tauri-plugin-dialog`) or path input.
2. **In-Place Scanning**: `scanner.rs` traverses folders recursively via `walkdir` without following symlinks. It reads standard filesystem metadata only. SQL is kept exclusively in `db.rs`, the repository layer.
3. **Change Detection**:
   - **New file**: Recorded as `active`.
   - **Timestamp/size change**: Updated and marked `modified`.
   - **Identical signature at new path with missing old path**: Detected as `moved`.
   - **Missing from disk**: Flagged as `missing` (never deleted from index without user consent).
4. **Opening Originals & Revealing Files**:
   - **Path Authorization**: Before opening or revealing any file, LibOne verifies that the file record exists in SQLite, belongs to an enabled scan root, and is strictly within authorized scan root boundaries.
   - **Non-Shell Execution**: LibOne never executes files via `cmd.exe` or shell command string interpolation. On Windows, it invokes native Win32 `ShellExecuteW` directly to launch the default registered handler, and spawns `explorer.exe /select,"<path>"` via direct argument passing without shell interpolation.
   - **Unavailable / Missing Handling**: If an indexed document no longer exists on disk, LibOne updates its status to `missing` and returns a descriptive explanation instead of failing silently.
5. **Metadata Export**: Complete snapshot serialized to a standardized, self-contained JSON schema.

---

## 5. Phase 1 Boundaries & Guarantees

Phase 1 focuses exclusively on the **Local Library Foundation**:

- **Read-Only File Safety**: LibOne never moves, renames, writes, alters, or deletes original files on disk.
- **Zero Cloud / Zero Network**: No external API calls, cloud storage, telemetry, analytics, user accounts, or background phone-homes.
- **No AI / Cloud Processing in Phase 1**: All indexing and change detection operate deterministically on the local CPU.
- **Safe Export**: Users can export their complete library database to portable JSON at any time.

---

## 6. Scanner Design, File Identity & Honest Limitations

### 6.1 Fast Change Detection
LibOne uses a tiered, metadata-first change detection strategy:
1. **Path Normalization**: Canonical lowercase paths with uniform Windows backslashes (`\`) ensure case-insensitive Windows filesystem lookups match reliably.
2. **Fast Metadata Check**: When traversing, LibOne inspects file size in bytes and filesystem modification timestamp (`mtime`).
   - If path, size, and timestamp match: marked `indexed` (unchanged), and `last_seen_at` is updated via a lightweight SQL touch.
   - If size or timestamp differs: marked `changed`, metadata updated, and content hash invalidated if not recomputed.

### 6.2 Windows Stable File Identity (NTFS & ReFS)
To track files across renames and moves within a scan root without reading file contents:
- On Windows systems supporting `BY_HANDLE_FILE_INFORMATION`, LibOne extracts the 32-bit volume serial number and 64-bit file index via Rust's `std::os::windows::fs::MetadataExt`.
- Stored as `"{vol:08x}:{idx:016x}"` in `indexed_files.file_id_nullable` (indexed in SQLite Schema version 2).
- When a file disappears from path A and appears at path B with the same file ID, LibOne updates the existing row in place (preserving historical discovery time, tags, and notes).

### 6.3 Move & Rename Detection Priority
1. **File ID Match**: When the file ID matches an existing record in the scan root that has not yet been matched at its old path, the file is recognized as moved/renamed.
2. **Unambiguous Fallback**: On filesystems where file IDs are not available (e.g., FAT32, exFAT, network shares), LibOne looks for exactly **one** unmatched record with the identical filename, size, and modified time. If exactly one candidate exists, it is linked as a move.
3. **Safe Fallback**: If multiple candidates exist or no match is found, the file is indexed as a new record and the old file is marked `missing` after the scan completes.

### 6.4 Status Model
- `indexed`: Present on disk and unchanged since last scan.
- `changed`: Present on disk, but size or modification time differs from the stored record.
- `missing`: Was previously indexed, but no longer found on disk at the recorded path. Historical records remain intact and are never deleted without explicit user confirmation.
- `inaccessible`: Path exists on disk but could not be read during traversal (permission denied, locked by another application).
- `error`: Failed during traversal due to I/O error or filesystem corruption.

### 6.5 Optional Streaming SHA-256 Hashing
- **Never loads whole documents into memory**: SHA-256 is calculated using a fixed 64 KB streaming buffer (`std::io::Read` into `sha2::Sha256`).
- **Opt-in Only**: Regular scans skip content hashing for maximum speed (scanning thousands of files in milliseconds). Hashing can be enabled per-scan via `ScanOptions { compute_hashes: true }` or on-demand for individual files via `compute_file_hash`.

### 6.6 Honest Limitations
1. **Non-NTFS Filesystems**: FAT32, exFAT, and some SMB/CIFS network shares do not expose persistent 64-bit file indices. On these volumes, LibOne relies on unambiguous name+size+mtime heuristics; complex simultaneous rename+edits will be treated as new files with the old record marked missing.
2. **Cross-Root Moves**: Rename and move tracking is scoped to the containing scan root. Moving a file from `D:\Documents` (Root 1) to `E:\Archive` (Root 2) will mark the file missing in Root 1 and new in Root 2.
3. **External Editors that Replace via Delete+Create**: Some text editors save files by writing to a temporary file, deleting the original, and renaming the temporary file into place. On NTFS, this may generate a new file ID. The scanner handles this via normalized path matching if the destination path remains the same.
4. **Cooperative Cancellation**: Scans check atomic cancellation tokens between directory entries. Cancelling a scan halts further traversal and preserves all records indexed up to that point; files in unvisited subtrees are left untouched and are **not** marked missing.

---

## 7. Library Access API & Security Design

### 7.1 Paginated File Listing & Query Engine
The SQLite repository layer provides `query_files` with parameterized SQL:
- **Pagination**: Returns fixed-size chunks using SQL `LIMIT ? OFFSET ?`, accompanied by total count calculations (`COUNT(*)`) and total page computation.
- **Sorting**: Validates sort keys against a strict whitelist (`name`, `path`, `extension`, `size`, `modified`, `status`) to prevent SQL injection, appending direction (`ASC` / `DESC`).
- **Filtering**: Combines scan root restrictions (`scan_root_id`), format restrictions (`extension`), lifecycle status filters (`status`), and multi-field substring queries (`file_name`, `absolute_path`, `extension`). Phase 2 adds `collection_id`, `tag_ids` (AND match via grouped `file_tags` subquery), and `has_notes`.
- **Bounds**: Page size is clamped to 1..200. The text query is truncated to 200 characters and tag IDs are deduplicated then capped at 64 before SQL generation. All user values are bound parameters; only whitelisted column names and generated `?` placeholders enter the SQL string.
- **Format Discovery**: `get_distinct_extensions` queries distinct non-empty file extensions to populate UI filter dropdowns dynamically.

### 7.2 Path Authorization Pipeline
Before opening or revealing any original document on disk, LibOne runs `validate_file_for_access`:
1. **Index Existence**: Ensures the target record exists in SQLite. Rejects arbitrary unindexed file paths.
2. **Scan Root Authorization**: Ensures the containing scan root exists and is marked `enabled`. Disabling a folder instantly blocks opening documents from it.
3. **Boundary Verification**: Verifies the normalized file path starts with the normalized scan root path, preventing directory traversal or symlink escapes.
4. **On-Disk Verification**: Checks that the file exists on disk. If the file has disappeared, LibOne updates its SQLite status to `missing` and returns a descriptive error explanation.
5. **Regular File Verification**: Rejects directories, junctions, or device targets.

### 7.3 Non-Shell Execution Guarantee
To guarantee safety and prevent command injection or execution of malicious batch scripts:
- **Default Application Launching**: On Windows, LibOne calls native Win32 `ShellExecuteW(NULL, L"open", wide_path, NULL, NULL, SW_SHOWNORMAL)` from `shell32.dll`. This directly resolves the registered default application without invoking `cmd.exe`, PowerShell, or any shell wrapper.
- **Show in File Explorer**: Uses direct process creation via `explorer.exe` with `/select,<path>` passed as a single argument. It validates that the record is indexed and its root is enabled, then uses the stored path (it does not recompute the root boundary). If the file is missing, LibOne attempts to reveal the containing folder while notifying the user. No `cmd.exe`/PowerShell shell is used.

### 7.4 Document Privacy
- LibOne displays and queries filesystem metadata only (paths, sizes, dates, hashes, status).
- Document contents are never loaded into memory for previewing, and are never transmitted across IPC or external networks.

---

## 8. File-System Watcher & Live Sync Architecture

### 8.1 Watcher Lifecycle
- **Active only while running**: File-system watching runs strictly within the LibOne process lifetime using the `notify` (v6.1) crate backed by Windows `ReadDirectoryChangesW`.
- **Dynamic root management**: On application startup, LibOne queries all `enabled` scan roots and registers watchers.
  - Enabling or adding a scan root starts its watcher thread immediately.
  - Disabling or removing a scan root aborts its watcher thread and releases Windows file-system handles.
  - When LibOne shuts down, dropping `WatcherManager` cleanly terminates all watcher threads.
- **Independence from manual scanning**: Watcher failures are isolated; manual scans remain fully functional regardless of watcher health.

### 8.2 Event Coalescing & Debouncing
- High-frequency file operations (e.g., git checkouts, compiling, downloading, multi-file extract) can generate hundreds of events per second.
- LibOne uses an event coalescing loop with two thresholds:
  1. **500 ms Quiet Window**: After receiving a file-system event, LibOne waits 500 ms for subsequent events to settle.
  2. **2,500 ms Maximum Window**: If events continue arriving indefinitely, a refresh triggers after 2.5 seconds to prevent starvation.
- While coalescing, the UI displays a subtle `Syncing changes…` indicator. Once coalescing finishes, a refresh executes in the background and emits a `library-updated` event so the frontend refreshes automatically.

### 8.3 Targeted Refresh vs. Rescan Fallback
When coalescing completes, LibOne determines the safest refresh strategy:
1. **Targeted Refresh**:
   - Used when the batch contains **<= 50 regular file changes** and no directory modifications.
   - **Created/Modified**: Inspects file metadata on disk (size, modified time, NTFS file ID) and updates the database row incrementally.
   - **Deleted**: Safely updates the SQLite file status to `missing` rather than deleting the historical record.
   - **Renamed**: Correlates the old and new file paths via NTFS file identity (`BY_HANDLE_FILE_INFORMATION`). If matched, the existing record is updated in place.
2. **Safe Rescan Fallback**:
   - Triggered when:
     - The event batch contains more than 50 files.
     - A directory was created, renamed, or deleted.
     - The OS buffer experienced an overflow or ambiguous event sequence.
   - Calls the core incremental scanner (`scanner::scan_folder_with_progress`), cleanly reconciling the root in SQLite.

### 8.4 Boundary Safety & Exclusion Rules
- **Symlinks & Junctions**: Excluded from event processing. Events originating from reparse points outside the target root are rejected.
- **Ignored Directories**: Ignores changes within `.git`, `node_modules`, `target`, `dist`, `build`, and LibOne's own AppData database directory.
- **Temporary Files**: Ignores common Office locking and temporary swap files (`~$*`, `*.tmp`, `*.swp`).

### 8.5 Honest Watcher Limitations
1. **Network Shares (SMB/CIFS)**: Depending on server configuration and Windows client cache settings, remote network shares may fail to emit `ReadDirectoryChangesW` events. Users should use manual scans for network locations.
2. **Drive Unmounting**: If an external USB drive or secondary hard drive is detached while LibOne is watching it, the OS watcher emits an I/O error. LibOne catches this, records the error in the per-root status, and displays a non-intrusive warning badge in the UI without crashing.
3. **Atomic Replace Patterns**: When editors create a temporary file, delete the target, and rename the temporary file over the target, intermediate delete events can be coalesced with create events. The debounce window minimizes transient flickers, but rapid successive saves may momentarily appear as a new record if NTFS assigns a fresh file ID.
4. **Coalesce Latency**: File changes are deliberately delayed by 500 ms to preserve CPU and battery life on laptop devices.

---

## 9. Portable Metadata Export Architecture

### 9.1 Self-Contained Versioned Snapshot
- **Format Version `1.2.0`**: Exports carry a dedicated `format_version` attribute conforming to semantic versioning guidelines documented in `EXPORT_FORMAT.md`.
- **Complete Library Metadata**: Includes top-level application metadata, aggregate summaries (`total_roots`, `total_files`, `total_size_bytes`), complete scan root definitions, and all indexed document file entries with their statuses, timestamps, sizes, NTFS identities, and computed hashes. Phase 2 adds `collections`, `tags`, `file_collections`, `file_tags`, and `file_notes`. Phase 3 adds `organization_suggestions`, `suggestion_files`, `duplicate_groups`, and `duplicate_group_files`.
- **Strictly No Document Content**: Under no circumstances does the export engine read or embed file payload bytes. `file_notes[].note_text` is user-authored only; suggestions and duplicate groups are derived solely from filesystem and database metadata; no extracted text, OCR output, or AI-generated content is ever stored or exported. The original documents remain completely untouched on disk.

### 9.2 Validation Pipeline
Before serializing or persisting any export to disk, the backend runs `validate_export`:
1. **Header Checks**: Asserts that `format_version`, `app`, `version`, and `exported_at` are present and non-empty.
2. **Integrity Validation**: Verifies `summary.total_roots == scan_roots.len()`, `summary.total_files == files.len()`, `summary.total_size_bytes == sum(files.size_bytes)`, and `summary.export_timestamp == exported_at`.
3. **Referential Validation**:
   - Collections and tags have unique positive IDs and non-empty names.
   - Every collection/tag link references an existing file and collection/tag; link pairs and note IDs are unique; notes have non-empty text and both timestamps.
   - Organization suggestions have unique positive IDs, valid non-empty types and titles, confidence in `0.0..=1.0`, valid status (`pending`, `approved`, `rejected`, `dismissed`), non-empty `created_at`, and non-empty `reviewed_at` if set.
   - Suggestion-file links reference existing suggestions and files with unique pairs.
   - Duplicate groups have unique positive IDs, valid match types, confidence in `0.0..=1.0`, valid status, and non-empty timestamps.
   - Duplicate-group-file links reference existing duplicate groups and files with unique pairs.
4. If any assertion fails, the operation aborts before performing filesystem writes, returning a descriptive error to the UI.

### 9.3 Atomic Safe-Write Guarantee
Writing metadata snapshots can fail if the user disconnects a drive, runs out of disk space, or encounters a power disruption. To prevent partial or corrupt JSON files:
- The backend writes the serialized JSON to a hidden sibling file (`.libone-export-<nanos>.tmp`) located in the same target folder.
- Once the write completes successfully, LibOne performs an atomic filesystem rename (`std::fs::rename`) into the final target filename.
- If writing or renaming fails, the temporary file is removed immediately and the original file (if one existed) remains intact.

### 9.4 Local Storage Transparency & Settings
- The application exposes a dedicated **Settings & Library Storage** screen detailing:
  - Exact path to the local SQLite database (`<AppData>\libone.db`).
  - Path to the application data root directory.
  - Active operating system and version.
  - Architecture guarantees: original files remain read-only; zero cloud synchronization or external uploads; metadata-only index.
- No cloud backups, background synchronization, or automatic external transmissions are implemented. Users retain complete local custody over all data.



---

## 10. Phase 2 As-Built Integration Review

This section describes the behavior that is actually implemented, which takes precedence over the earlier planning text in `PHASE_2_PLAN.md`.

### 10.1 Migration and Existing Data

- `schema_version` is `3`. The v3 migration is additive and runs inside the repository's transaction.
- The migration adds the five Phase 2 tables and indexes with `IF NOT EXISTS`; no Phase 1 table or column is dropped or renamed.
- A Phase 1 (v2) database with existing scan roots and indexed files is migrated in place. Root records, file IDs, paths, hashes, NTFS file IDs, statuses, and timestamps are preserved.
- Existing files can immediately be linked to collections/tags and given notes without reindexing.

### 10.2 Scanning and Watcher Updates

- The scanner and watcher call only `touch_indexed_file`, `update_indexed_file`, `move_indexed_file`, `mark_file_missing`, and `insert_indexed_file` on file records.
- They never issue `DELETE FROM indexed_files` and never touch `collections`, `tags`, `file_collections`, `file_tags`, or `file_notes`.
- Moves/renames update the existing row by `id`; because all Phase 2 links and notes reference that `id`, organization metadata follows the file record.
- A file that disappears from disk is marked `missing`; its links and notes remain readable.
- A disabled scan root keeps its indexed files and all organization metadata. It does not delete or detach anything.

### 10.3 Original File Lifecycle

- Deleting, renaming, or moving an original file on disk changes only LibOne metadata (`status`, path, timestamps). No original file is modified by any Phase 2 operation.
- Deleting a collection or tag removes only that metadata row and its links (`ON DELETE CASCADE` applies to links, not to files). The indexed file and its other links/notes remain.
- Deleting a note removes only that note row.
- All collection/tag/note commands are local SQLite operations. There is no remote call of any kind.

### 10.4 Disabled Scan Roots

For consistency, every **mutation** of organization metadata is blocked when the containing scan root is disabled:

- `add_file_to_collection`, `remove_file_from_collection`
- `add_file_to_tag`, `remove_file_from_tag`
- `create_file_note`, `update_file_note`, `delete_file_note`

These commands return a descriptive error such as `Action denied: Scan root 'X' containing file #N is disabled.` Reading existing metadata (`list_file_notes`, `list_file_collections`, `list_file_tags`, `list_collections`, `list_tags`) is still allowed, so historical data remains inspectable.

### 10.5 Search and Filtering

- Phase 1 filters (filename/path substring, scan root, status, extension) and pagination/sorting are preserved.
- A single collection filter and a multi-tag AND filter are applied in SQLite with bound parameters. Tag IDs are deduplicated and capped at 64; the free-text term is capped at 200 characters; page size is clamped to 1..200.
- The text box is debounced in the UI (300 ms). Explicit filter changes are not debounced. Pagination resets to page 1 whenever any filter or sort option changes.

### 10.6 Export

- Export format is `1.2.0`: all Phase 1 and Phase 2 fields plus `organization_suggestions`, `suggestion_files`, `duplicate_groups`, and `duplicate_group_files`.
- The export model is validated before serialization, then written to a sibling `.tmp` file and atomically renamed.
- Exported notes are user-authored text only. Suggestions and duplicate groups are derived solely from filesystem and database metadata. No document bytes, extracted text, OCR output, or external AI output is read or written.

### 10.7 Security and Scope Boundaries

- No cloud calls, telemetry, analytics, accounts, OAuth, AI services, OCR, or full-text extraction were introduced. The frontend uses only Tauri IPC and native dialogs; there is no `fetch`, `XMLHttpRequest`, or WebSocket usage in application code.
- `@tauri-apps/plugin-opener` is initialized but is not used to open URLs or external links.
- No shell-based launching was reintroduced. Windows opening uses direct `ShellExecuteW`; Explorer reveal uses `explorer.exe` with direct arguments. Neither uses `cmd.exe`/PowerShell interpolation.
- All production SQL is confined to `src-tauri/src/db.rs` (the repository layer). `commands.rs`, `scanner.rs`, `watcher.rs`, and `validate.rs` contain no SQL statements. Raw SQL in `db.rs` tests is fixture setup only.
- Tracked files contain no secrets, `.env` files, local SQLite databases, `node_modules`, `dist`, Rust `target`, Tauri `gen`, `.npm-cache`, or SDK downloads. `package-lock.json` and `Cargo.lock` are intentionally tracked for reproducible builds.

### 10.8 UI Error Handling

Errors from the backend are surfaced in the relevant surface rather than silently swallowed:

- Load failures appear in the sidebar/file table with retry or clear actions.
- CRUD dialogs show inline error text and keep user input so a failed save can be retried.
- Delete and bulk-assignment dialogs explain that only LibOne metadata changes.
- Messages are human-readable but can include technical identifiers (for example `Note #42 not found.` or `Indexed file #7 not found in library.`). See `KNOWN_LIMITATIONS.md`.

---

## 11. Phase 3 Architecture: Metadata Suggestion Engine & Optional Local Provider Boundary

Phase 3 introduces intelligent organization suggestions, duplicate group review, version sequence detection, and stale-file identification, while maintaining LibOne's strict zero-cloud, privacy-first invariants.

### 11.1 Deterministic Metadata Suggestions as Default Engine
- **Source of Truth**: All suggestions are generated strictly from indexed local filesystem metadata (filenames, parent folder clusters, timestamps, byte sizes, NTFS file identity, SHA-256 hashes, and existing collections/tags).
- **Zero Document Content Reading**: Document text, body bytes, and binary contents are never loaded or analyzed by the suggestion engine.
- **Explainability**: Every suggestion carries an explicit human-readable rationale, confidence score (0.0 to 1.0), and pending status.
- **Non-Destructive Metadata Operations**: Approving duplicate groups, versions, or suggestions updates only LibOne's SQLite metadata (tags, collections, notes). Original files on disk are never moved, renamed, replaced, or deleted.

### 11.2 Optional Local Provider Interface (`LocalIntelligenceProvider`)
To accommodate future on-device intelligence without compromising privacy, Phase 3 defines an abstract Rust trait isolated in `src-tauri/src/intelligence.rs`:

```rust
pub trait LocalIntelligenceProvider: Send + Sync {
    fn is_available(&self) -> bool;
    fn provider_name(&self) -> &str;
    fn status_message(&self) -> &str;
    fn status(&self) -> LocalAiStatus;
    fn enhance_rationale(&self, context: &str) -> Option<String>;
}
```

### 11.3 Strict Architectural Boundaries & Safeguards
1. **No Network Requests**: The intelligence interface enforces an offline invariant (`allows_network: false`). Outbound HTTP, WebSocket, or socket connections are strictly prohibited.
2. **No API Keys or Account Setup**: No cloud credentials, API tokens, user authentication, or vendor account configurations are accepted or stored.
3. **No Document Uploads**: File contents and user documents are never transmitted across process boundaries or off-device.
4. **No Default Model Downloads**: The application does not silently or automatically download heavy LLM weights, embedding checkpoints, or runtime packages in the background.
5. **Deterministic Default**: The `DisabledLocalProvider` is the active default implementation. If local AI is not explicitly configured, the UI clearly displays:
   > **“Local AI is not configured”**  
   and the application functions seamlessly using purely deterministic metadata analysis.
6. **Code Isolation**: All intelligence provider interfaces and status structures are strictly segregated in `src-tauri/src/intelligence.rs`, preventing coupling with SQLite persistence or scanner routines.

---

## 12. Local Backup, Restore, and Integrity Verification Architecture

Phase 4 introduces robust local database backup, validation, safety snapshots, atomic restore, and diagnostic integrity verification.

### 12.1 Local Metadata Backup
- **SQLite `VACUUM INTO`**: Creates a clean, defragmented SQLite snapshot of `libone.db` without holding write locks or requiring WAL reconciliation.
- **Atomic Temporary Sibling Staging**: Backups are written to `.libone_backup_tmp_<timestamp>.tmp` in the destination directory, verified, and renamed atomically into place.
- **Metadata-Only Guarantee**: Only LibOne's SQLite database schema and records (roots, indexed files, tags, collections, notes, suggestions) are included. User document files on disk are never packaged or copied into the backup file.
- **Zero Cloud Uploads**: The backup file is saved purely on local or user-selected storage through the native Windows save dialog.

### 12.2 Safe Restore with Automatic Pre-Restore Safety Snapshot
- **Pre-Flight Validation**: Before replacing any active database file, the selected backup is inspected:
  - Verifies SQLite header, integrity check (`PRAGMA quick_check`), and required table existence (`library_metadata`, `scan_roots`, `files`, `collections`, `tags`, etc.).
  - Validates schema version compatibility (must match `schema_version = 4`).
- **Safety Backup**: LibOne automatically duplicates the current active `libone.db` to `libone_safety_pre_restore_<timestamp>.db` before performing any database replacement.
- **Crash-Safe Staging**: The new database is staged as `libone.db.restore_staging` before atomic swap. If the process is interrupted, `recover_interrupted_restore` detects staging files on subsequent startup and safely recovers.
- **Rollback on Error**: If any step of the replacement fails, the safety backup is automatically restored, ensuring zero data loss.
- **Non-Destructive Invariant**: Restoring metadata never creates, deletes, moves, or alters any original document files on disk.

### 12.3 Database Integrity Diagnostics & Repair Guidance
- **PRAGMA quick_check**: Evaluates low-level B-tree and page allocation integrity.
- **Foreign Key Violations**: Identifies orphaned records across junction tables (`file_tags`, `file_collections`, `file_notes`, `suggestion_files`, `duplicate_group_files`).
- **Missing File Detection**: Flags registered files whose physical disk paths are missing or unreadable, without silently purging their database records.
- **Actionable Repair Guidance**: Every detected issue provides clear non-destructive guidance (e.g. remounting an external drive, running a manual scan, or restoring from a backup) without silent record deletion.
