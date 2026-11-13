# LibOne Phase 1 Manual Test Plan & Handoff Verification

This document provides reproducible step-by-step test cases for manual validation of LibOne Phase 1 on Windows. Each test case specifies preconditions, test steps, expected outcomes, and a section to record actual execution results.

---

## 1. Test Environment Setup
- **Operating System**: Windows 10 / 11 (64-bit)
- **Filesystem**: NTFS (primary system or secondary internal drive)
- **Node.js**: v18+ or v20+
- **Rust Toolchain**: 1.77+ with MSVC toolchain
- **Test Working Directory**: Create a temporary test folder (e.g. `C:\LibOne_Test` or `D:\LibOne_Test`)

---

## 2. Test Cases

### TC-01: Application Bootstrap & Database Initialization
* **Goal**: Verify that LibOne creates its local SQLite metadata database on first launch without errors.
* **Preconditions**: No previous instance running.
* **Steps**:
  1. Launch LibOne via `npm run tauri dev` or running the production binary.
  2. Observe application launch and the main dashboard.
  3. Click **Settings** in the top navigation bar.
  4. Note the displayed database path (e.g., `%APPDATA%\com.libone.app\libone.db`).
* **Expected Outcome**:
  - Application starts with clean dark theme.
  - SQLite database file is created at the designated AppData path.
  - "Local Data Location" is clearly shown in Settings and the sidebar footer.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-02: Folder Registration & Path Validation
* **Goal**: Verify folder addition and rejection of unauthorized/invalid paths.
* **Preconditions**: Test directory created with subfolders.
* **Steps**:
  1. Click **Add** in the left "Folders" sidebar.
  2. Select `C:\LibOne_Test` (or your test folder).
  3. Verify it appears in the sidebar list with its display name and folder path.
  4. Attempt to add the exact same folder again.
* **Expected Outcome**:
  - Selected folder is accepted and persisted in SQLite.
  - Attempting to add an already registered folder shows a non-intrusive duplicate warning banner.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-03: Safe Non-Destructive Folder Removal
* **Goal**: Verify that removing or disabling a scan root removes only LibOne index records, leaving original files untouched.
* **Preconditions**: A registered folder with files indexed.
* **Steps**:
  1. Click the trash icon next to the test folder in the sidebar.
  2. Inspect the confirmation dialog text.
  3. Confirm removal.
  4. Open Windows File Explorer and inspect the original folder.
* **Expected Outcome**:
  - Dialog explicitly states: *"LibOne will only remove its local index records. Your actual files and folders will not be touched."*
  - The folder is removed from the LibOne UI.
  - All original files, folders, and documents remain 100% intact on disk.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-04: Real-Time Scanning & Progress Tracking
* **Goal**: Verify recursive discovery, progress emission, and metrics.
* **Preconditions**: Folder containing nested subdirectories and documents (PDF, DOCX, TXT, XLSX).
* **Steps**:
  1. Click **Scan Screen** in the header.
  2. Select the target scan root from the dropdown.
  3. Leave "Compute SHA-256 Hashes" unchecked for fast scan.
  4. Click **Start Scan**.
  5. Observe directories visited, files discovered, and live metrics.
* **Expected Outcome**:
  - State changes from "Ready to Scan" -> "Scanning…" -> "Completed".
  - Metrics accurately display total files added and duration in milliseconds.
  - Progress bar and visited directory paths update smoothly without freezing the UI.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-05: Safe Scan Cancellation
* **Goal**: Verify that cancelling an in-progress scan preserves partial progress internally.
* **Preconditions**: A folder with a large number of files or nested trees.
* **Steps**:
  1. Start a scan on the test folder.
  2. While the scan is actively running, click **Cancel Scan**.
* **Expected Outcome**:
  - Status updates to "Cancelled (Partial Progress Saved)".
  - All documents discovered prior to cancellation remain queryable in the library.
  - Unvisited subdirectories are left untouched and are NOT marked missing.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-06: Fast Change & Status Model Detection
* **Goal**: Verify detection of indexed, changed, missing, and inaccessible files.
* **Preconditions**: A previously scanned folder with indexed files.
* **Steps**:
  1. Modify the content of one indexed text file.
  2. Delete a second indexed file from disk.
  3. Run an incremental scan ("Scan All").
  4. Inspect the file table in the dashboard.
* **Expected Outcome**:
  - Modified file displays a `Changed` status badge with updated modified timestamp.
  - Deleted file displays a `Missing` status badge.
  - The missing file record is preserved in SQLite; it is NOT deleted.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-07: Search, Filtering & Pagination
* **Goal**: Verify multi-criteria querying and UI responsiveness.
* **Preconditions**: At least 30 indexed files across different formats.
* **Steps**:
  1. Type a partial filename into the search box.
  2. Click the `PDF` format filter dropdown.
  3. Click the `Indexed` status pill.
  4. Click the "Size" table header to toggle sorting (asc / desc).
  5. Switch pagination between 10, 25, 50, and 100 items per page.
* **Expected Outcome**:
  - Results update instantly based on combined criteria.
  - Column sort arrows indicate direction.
  - Pagination controls allow seamless navigation across pages.
  - Empty state displays actionable "Clear Filters & Search" button if query yields 0 results.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-08: Path Authorization & Non-Shell Launching
* **Goal**: Verify safe opening of original files using default apps without shell execution.
* **Preconditions**: Valid indexed document with a registered default application.
* **Steps**:
  1. Click **Open** on a valid document row.
  2. Verify that Windows opens the document in its default viewer.
  3. Attempt to open a file marked `Missing`.
  4. Disable the containing folder in the sidebar, then attempt to open a file from it.
* **Expected Outcome**:
  - Valid files open directly via Win32 `ShellExecuteW` (no `cmd.exe` or PowerShell process).
  - Missing file displays an actionable warning: *"File unavailable: '...' was not found on disk."*
  - Disabled folder displays an authorization error: *"Access denied: Containing folder is currently disabled."*
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-09: Live File-System Watcher & Debouncing
* **Goal**: Verify background watching, event coalescing, and non-intrusive status pill.
* **Preconditions**: At least one enabled scan root.
* **Steps**:
  1. Observe the live watcher pill in the header (green pulsing dot: *"Watching N folders"*).
  2. In Windows File Explorer, create a new document in the watched folder.
  3. Watch the pill transition to *"Syncing changes…"*.
  4. Observe the document appear in the LibOne library table without manual refreshing.
* **Expected Outcome**:
  - Rapid file operations are coalesced within the 500ms debounce window.
  - Background updates trigger a silent library reload.
  - Clicking the watcher pill opens the diagnostics popover showing per-root health.
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

### TC-10: Portable Metadata Export (v1.0.0)
* **Goal**: Verify atomic export to JSON, format versioning, and validation.
* **Preconditions**: Library contains indexed folders and files.
* **Steps**:
  1. Click **Export** in the top navigation bar.
  2. Inspect the metadata preview modal (verify format version `v1.0.0`, total folders, files, size).
  3. Click **Copy to Clipboard** and paste into a text editor to inspect JSON structure.
  4. Click **Save to File (.json)**, choose a location, and save.
  5. Open the exported file in an external editor or validator.
* **Expected Outcome**:
  - JSON strictly conforms to `EXPORT_FORMAT.md`.
  - Temporary `.tmp` file is cleanly replaced by the final `.json` file atomically.
  - Zero document payload contents are included (metadata only).
* **Actual Result**: [ ] Pass / [ ] Fail — Notes: __________________________

---

## 3. Test Execution Summary Sheet

| Test Case | Description | Pass / Fail | Executed By | Date | Remarks |
|---|---|---|---|---|---|
| TC-01 | Bootstrap & SQLite DB Init | | | | |
| TC-02 | Folder Add & Validation | | | | |
| TC-03 | Safe Non-Destructive Removal | | | | |
| TC-04 | Real-Time Scanning & Metrics | | | | |
| TC-05 | Safe Scan Cancellation | | | | |
| TC-06 | Fast Change & Status Model | | | | |
| TC-07 | Search, Filtering & Pagination | | | | |
| TC-08 | Path Authorization & Safe Launch | | | | |
| TC-09 | Live Watcher & Debouncing | | | | |
| TC-10 | Portable Metadata Export | | | | |

**Overall Result**: [ ] PASS (Handoff Ready) / [ ] BLOCKED  
**Sign-off**: ___________________________ Date: _________________
