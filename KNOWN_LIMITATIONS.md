# LibOne: Phase 1 Known Limitations & Edge Cases

This document transparently records the architectural constraints, platform-specific trade-offs, and unresolved edge cases of LibOne Phase 1.

---

## 1. Filesystem & OS Limitations (Windows)

### 1.1 Non-NTFS Volumes (FAT32, exFAT, ReFS)
- **Stable File Identity**: On NTFS volumes, LibOne uses native Win32 `BY_HANDLE_FILE_INFORMATION` (volume serial + 64-bit file index) to track file moves and renames across directories.
- **Fallback Behavior**: On FAT32, exFAT, or virtual filesystems where file IDs are not exposed or persistent, LibOne falls back to unambiguous heuristic matching (identical filename, size, and modification timestamp).
- **Limitation**: If a file is simultaneously renamed AND modified on a non-NTFS volume, LibOne treats it as a new file, marking the previous record as `missing`.

### 1.2 Network Shares (SMB / CIFS / NFS)
- **Directory Notifications**: Real-time file-system watching relies on Windows `ReadDirectoryChangesW`. Remote network drives often fail to emit notifications or deliver delayed event batches depending on server caching settings.
- **Recommendation**: For remote network shares, use manual scans ("Scan All" or "Scan Screen") to reconcile changes.

### 1.3 Drive Dismounting & Removable Storage
- **Detached Drives**: If an indexed external USB hard drive or flash volume is unplugged while the app is active, the active file watcher encounters an OS I/O error (`ERROR_DEV_NOT_EXIST`).
- **Resilience**: LibOne isolates the error, updates the per-root status to `Error`, and highlights a non-intrusive warning pill in the UI. The app never crashes.
- **Accessing Files**: Attempting to open files from an unplugged drive safely fails path validation and notifies the user without stalling the UI.

### 1.4 Very Long Paths (> 260 characters)
- LibOne normalizes paths and supports Windows Extended-Length Paths (`\\?\`) where appropriate. However, certain third-party default viewer applications launched via `ShellExecuteW` may fail to open paths exceeding the legacy `MAX_PATH` (260 characters) limit unless system-wide long path support is enabled in Windows Registry (`LongPathsEnabled`).

---

## 2. Editor & File Replacement Patterns

### 2.1 Atomic "Delete + Rename" Editors
- **Behavior**: Several popular applications (e.g., VS Code, Vim, JetBrains IDEs, Microsoft Word) implement "safe save" by writing data to a temporary file (`~$doc.docx`, `file.tmp`), deleting the original file, and renaming the temporary file into place.
- **File ID Reassignment**: On NTFS, this operation may assign a brand new 64-bit file index to the target file.
- **Handling**: LibOne's 500ms event debouncing window absorbs rapid successive events and reconciles via normalized path matching. However, during rapid successive disk writes, a momentary `changed` or `missing` state may occur before settling.

### 2.2 Locked Files (Exclusive Access)
- Files opened by applications with exclusive file locks (`FILE_SHARE_READ` denied) cannot be read during traversal.
- **Handling**: LibOne records a non-fatal warning (`permission_denied` / `unreadable_path`), marks the record as `inaccessible`, and continues traversal without interrupting the scan.

---

## 3. Scope Boundaries for Phase 1

### 3.1 Metadata-Only (No Document Previews or Full-Text Search)
- Phase 1 indexes filesystem metadata only (filename, path, extension, byte size, timestamps, optional SHA-256 hash, and lifecycle status).
- Document contents are **never** loaded into memory, extracted, or searched. Full-text search, OCR, and in-app document viewing are scheduled for subsequent phases.

### 3.2 No Cloud Backup or Cross-Device Sync
- Phase 1 operates purely offline and local-first. There is no cloud storage, telemetry, multi-device synchronization, or automatic external backup.
- All database state resides locally in SQLite at `%APPDATA%\com.libone.app\libone.db`.

### 3.3 Scoped Rename & Move Tracking
- Move and rename detection is strictly scoped within each user-configured scan root.
- Moving a file across two distinct scan roots (e.g., from `D:\Projects` to `E:\Archive`) is tracked as a deletion (`missing`) in the source root and a new creation (`indexed`) in the destination root.

### 3.4 Symlinks and Directory Junctions
- Symbolic links and directory junctions are explicitly excluded from traversal and watching to prevent infinite directory recursion, duplicate indexing, and boundary escapes.

---

## 4. Phase 2 Integration Limitations

### 4.1 Search Scope

- Free-text search still matches **file name, absolute path, and extension only**. It does not search note text, tag names, or collection names.
- Tag filtering is AND-only: selecting tags `A` and `B` returns files that have **both**. There is no OR mode.
- Only one collection can be selected at a time.
- Bounds are intentional: free-text queries are truncated to 200 characters, tag IDs are deduplicated and capped at 64, and page size is clamped to 1..200. Requests beyond these limits are silently reduced rather than rejected.
- The debounce is 300 ms. A filter change made while the search box is still debouncing applies immediately; the final file list combines both once the search settles.

### 4.2 Notes and Organization UI

- Notes are plain user-authored text. The UI does not render Markdown, attachments, or rich text, and it does not auto-populate notes from document contents.
- The file table does not display inline tag pills or a note indicator; organization state is managed from the sidebar and the file detail dialog.
- File counts in the sidebar are computed by querying the library per collection/tag. They refresh after mutations but are not maintained as materialized counters.
- Browser dev-preview mode uses in-memory mock data. Mock notes/links reset when the page reloads; only the Tauri/SQLite path is persistent.

### 4.3 Disabled Roots and Missing Files

- Disabling a scan root keeps its indexed files, collections, tags, links, and notes. It does not detach or delete metadata.
- While a root is disabled, all organization **mutations** are blocked (add/remove links, create/update/delete notes). Reads remain available so historical metadata can be inspected.
- If an original file is missing but its root is enabled, existing notes remain editable and new notes can still be added; this is metadata-only and never touches the file path.

### 4.4 Export

- Format version `1.2.0` includes all Phase 1 and Phase 2 metadata plus Phase 3 organization suggestions, suggestion-file links, duplicate groups, duplicate-group files, suggestion workflow statuses, and review timestamps.
- Exports include user-authored note text. Treat exported JSON files as private data.
- `summary` keeps only the Phase 1 counters (`total_roots`, `total_files`, `total_size_bytes`); counts for collections, tags, notes, suggestions, and duplicate groups must be derived from their respective arrays.
- Validation checks structure and referential integrity across all entities, but does not require `format_version` to equal the current version. Older `1.0.0` and `1.1.0` exports remain readable by design (missing Phase 2/3 arrays default to empty lists).
- Atomic write/rename uses a temporary file in the target directory, so the final target must be on the same filesystem.

### 4.5 Error Messages

- Errors are shown in context (toolbar banners, sidebar retry, dialogs) and are human-readable, but some include raw technical identifiers such as `Note #42 not found.` or `Indexed file #7 not found in library.`. There is no localization layer or stable error-code catalog.

### 4.6 Scope Boundaries (Unchanged)

- LibOne does not perform cloud sync, telemetry, account sign-in, AI classification, OCR, or full-text extraction in Phase 2.
- No shell-based file launching was introduced. Windows file opening uses direct `ShellExecuteW`; Explorer reveal uses a direct `explorer.exe` process with arguments.
- All production SQL remains in the repository layer (`db.rs`). The frontend and command layer never construct SQL.

---

## 5. Phase 3 Suggestion Engine & Local AI Provider Boundaries

### 5.1 Local AI Interface Only (Disabled by Default)
- **Trait Definition Only**: LibOne defines an abstract Rust provider trait (`LocalIntelligenceProvider` in `src-tauri/src/intelligence.rs`) as an architectural placeholder for future on-device intelligence.
- **No AI Enabled or Required**: No machine learning model, weights, neural network runtime, or background inference engine is bundled or active. The default implementation is `DisabledLocalProvider`.
- **Status Transparency**: Whenever local AI is unavailable, the application UI clearly and explicitly communicates:
  > **“Local AI is not configured”**

### 5.2 Deterministic Metadata Rules as Default Engine
- **Pure Metadata Analysis**: All Phase 3 suggestions (duplicate files, likely versions, collection clusters, tag proposals, and stale files) are generated using deterministic rule-based algorithms analyzing local filesystem timestamps, folder names, extensions, file sizes, and NTFS identities.
- **No Document Content Extraction**: Document payloads, full text, and binary bytes are never parsed, read, or transmitted.
- **Explicit User Action Required**: Suggestions are never automatically applied. Every metadata modification requires an explicit user click.
- **Non-Destructive Operations**: Marking a preferred file or approving duplicate/version suggestions assigns SQLite tags and notes only. No files on disk are ever moved, renamed, overwritten, or deleted.

### 5.3 Strict Privacy & Networking Constraints
- **Zero Network Calls**: Outbound network connections, cloud API calls, and telemetry are strictly forbidden across all intelligence abstractions (`allows_network: false`).
- **No API Keys or Accounts**: LibOne requires no API tokens, accounts, subscription setups, or external registrations.
- **No Model Downloads**: The application does not perform automatic or background model downloads.
- **Code Isolation**: All AI provider structures are quarantined behind the `intelligence.rs` module boundary, ensuring zero impact on core library indexing, searching, or scanning performance.
