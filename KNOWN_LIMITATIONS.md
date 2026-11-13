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
