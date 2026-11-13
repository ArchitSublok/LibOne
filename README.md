# LibOne

**Your private, local-first document library for Windows.**

LibOne helps people make sense of documents already stored on their Windows PC. It brings files scattered across Downloads, Desktop, Documents, and other folders into one searchable, organized library—without uploading them, moving them, or requiring an account.

> **Core Guarantee**: LibOne indexes your files in place. It operates in strict read-only mode on your documents and never alters, moves, uploads, or deletes your original files.

---

## Phase 1 Feature Overview

LibOne Phase 1 delivers a rock-solid local foundation for document discovery, tracking, and management:

1. **Local Scan Roots & Recursive Discovery**
   - User-authorized folder registration with persistent SQLite storage.
   - Recursive discovery of regular files with non-fatal error handling (locked files, inaccessible directories, paths disappearing during scans).
   - Strict boundary safety: symlinks and directory junctions are skipped by default; build and dependency folders (`.git`, `node_modules`, `target`, `dist`, LibOne AppData) and temporary lock files (`~$*`, `*.tmp`) are safely excluded.
   - Non-destructive folder removal: removing or disabling a folder removes only local index records—original files remain 100% untouched on disk.

2. **Dedicated Real-Time Scan Screen**
   - Live metrics: directories visited, files discovered, files indexed, and duration in milliseconds.
   - Non-fatal warnings panel categorized by code (`permission_denied`, `symlink_skipped`, `unreadable_path`, `file_disappeared`).
   - Cooperative, instantaneous cancellation: cancelling a scan preserves partial progress; unvisited subtrees remain untouched and are never marked missing.
   - Optional streaming SHA-256 content hashing (64 KB fixed buffer; off by default to maintain maximum traversal speed).

3. **Fast Change Detection & Stable File Identity**
   - Normalization: lowercase backslash-normalized paths.
   - NTFS Identity: Extracts Win32 volume serial and 64-bit file index (`BY_HANDLE_FILE_INFORMATION`) to track renamed and moved files within a root, preserving database IDs and discovery history.
   - Heuristic Fallback: On FAT32, exFAT, or network shares where file IDs are not available, falls back to unambiguous name + size + mtime matching.
   - 5-State Lifecycle Model: `indexed`, `changed`, `missing`, `inaccessible`, `error`. Missing records are preserved in the database for auditing and never silently deleted.

4. **Library Query Engine & Interactive Dashboard**
   - Paginated file table with configurable page sizes (10, 25, 50, 100).
   - Multi-column sorting (Name, Format, Status, Size, Modified Date).
   - Instant filtering by scan root, document format extension (`.pdf`, `.docx`, `.xlsx`, `.txt`, etc.), and lifecycle status.
   - Rich Document Detail modal displaying full filesystem metadata, parent folder info, disk existence verification, and on-demand SHA-256 calculation.
   - Actionable empty states with instant filter clearance and scan triggers.

5. **Path Authorization & Non-Shell Launching**
   - Strict security pipeline validates every open/reveal request against indexed SQLite records, active scan root boundaries, and on-disk file existence.
   - Non-shell execution: launches files via direct Win32 `ShellExecuteW` from `shell32.dll` (no `cmd.exe` or PowerShell command interpolation).
   - Windows File Explorer integration: reveals original files via direct process invocation (`explorer.exe /select,"<path>"`).

6. **Background File-System Watcher & Coalescing**
   - Real-time directory monitoring powered by Windows `ReadDirectoryChangesW` (`notify` crate).
   - Dual-threshold debouncing loop: 500 ms quiet window with a 2,500 ms maximum coalesce deadline to prevent UI starvation during burst file operations.
   - Targeted incremental refreshes for <= 50 file batches; automatic rescan fallback for directory operations or buffer overflows.
   - Non-intrusive live status indicator pill in the UI header with diagnostics popover.

7. **Portable Metadata Export (Format v1.0.0)**
   - Complete library snapshot exported to structured, self-contained JSON conforming to [`EXPORT_FORMAT.md`](EXPORT_FORMAT.md).
   - In-memory pre-write validation asserts schema integrity and summary counts before any disk operations.
   - Atomic safe-write: writes to a hidden sibling temporary file (`.libone-export-<nanos>.tmp`) before executing an atomic rename, preventing partial or corrupt files.
   - Native Windows save-file dialog and JSON preview modal with one-click clipboard copying.

8. **Local Storage Transparency & Settings**
   - Dedicated Settings screen displaying exact SQLite database path (`%APPDATA%\com.libone.app\libone.db`), AppData root directory, and app version.
   - Explicit architectural declarations: read-only access, zero cloud synchronization or external uploads, and metadata-only indexing.

---

## Local Setup & Development

### Prerequisites

- **Windows 10 or 11 (64-bit)**
- **Node.js**: v18.0.0 or later (v20+ recommended)
- **Rust Toolchain**: `stable-x86_64-pc-windows-msvc` (install via [rustup.rs](https://rustup.rs))
- **Visual Studio C++ Build Tools**: Required for native MSVC linking (`link.exe` and Windows SDK libraries)
- **WebView2 Runtime**: Pre-installed on Windows 10/11

### Step-by-Step Instructions

1. **Clone the repository**:
   ```powershell
   git clone https://github.com/ArchitSublok/LibOne.git
   cd LibOne
   ```

2. **Install frontend dependencies**:
   ```powershell
   npm install
   ```

3. **Run TypeScript check & build frontend**:
   ```powershell
   npm run build
   ```

4. **Run Rust unit and integration tests**:
   ```powershell
   cd src-tauri
   cargo test
   cd ..
   ```

5. **Verify Rust code formatting**:
   ```powershell
   cd src-tauri
   cargo fmt --check
   cd ..
   ```

6. **Launch the desktop app in development mode**:
   ```powershell
   npm run tauri dev
   ```

7. **Compile production release binary**:
   ```powershell
   cd src-tauri
   cargo build --release
   cd ..
   ```
   The optimized executable will be located at:
   `src-tauri\target\release\libone.exe`

---

## Documentation Index

- [ARCHITECTURE.md](ARCHITECTURE.md) — Comprehensive technical architecture, database schemas, scanner pipeline, watcher lifecycle, and security model.
- [EXPORT_FORMAT.md](EXPORT_FORMAT.md) — JSON schema definition, example payload, and semantic compatibility rules for metadata exports.
- [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) — Platform-specific edge cases, NTFS file ID limitations, network drive behavior, and editor save patterns.
- [PHASE_1_MANUAL_TEST_PLAN.md](PHASE_1_MANUAL_TEST_PLAN.md) — 10 reproducible manual test cases, expected outcomes, and sign-off recording sheet for Phase 1 verification.
