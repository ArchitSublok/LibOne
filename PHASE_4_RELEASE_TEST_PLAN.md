# LibOne Phase 4 Release Test Plan & Manual Acceptance Checklist

This document defines the comprehensive manual verification plan for the **LibOne Phase 4 Windows Release**. Each test case specifies preconditions, step-by-step actions, expected outcomes, and sign-off checkmarks.

---

## 1. Execution Status

* **Automated Phase 4 Release Test Suite**: **READY (`src-tauri/tests/phase4_release_tests.rs`)**
  - Automated tests cover:
    1. Fresh install database creation
    2. Upgrade from Phase 1 database (v1 -> v4)
    3. Upgrade from Phase 2 database (v2/v3 -> v4)
    4. Upgrade from Phase 3 database (v3 -> v4)
    5. Backup and restore lifecycle
    6. Interrupted backup handling
    7. Corrupted backup rejection and safety rollback
    8. Database integrity failure detection and repair guidance
    9. Path authorization and traversal prevention
    10. Disabled-root access blocking
    11. Watcher recovery and resilience
    12. Scan cancellation without falsely marking files missing
    13. Export metadata-only guarantee
    14. Diagnostics path redaction
  - Compilation check command: `cargo check --tests --manifest-path src-tauri\Cargo.toml` (0 warnings, 0 errors).
  - Frontend type check: `npx tsc --noEmit` (0 errors).
  - Frontend production build: `npm run build` (0 errors).

* **Manual Release Acceptance Testing**: **PENDING HUMAN EXECUTION**
  > [!IMPORTANT]
  > In accordance with strict release protocol, manual test cases below are **strictly marked as unexecuted** until a human tester executes the interactive scenarios on Windows hardware and completes the sign-off table.

---

## 2. Test Environment Setup

- **Operating System**: Windows 10 (64-bit, build 19041+) or Windows 11 (64-bit)
- **Runtime**: Microsoft Edge WebView2 (pre-installed on Windows 11 and modern Windows 10)
- **Filesystem**: NTFS (primary drive `C:\`, optional secondary internal/USB drive `D:\` or `E:\`)
- **Release Artifacts**:
  - Installer: `D:\LibOne\.cargo-target\release\bundle\nsis\LibOne_0.1.0_x64-setup.exe`
  - Portable Executable: `D:\LibOne\.cargo-target\release\libone.exe`
- **Working / Staging Directory**: Create temporary test folders (e.g., `D:\LibOne_Test_Docs`)

---

## 3. Manual Release Test Cases

### TC-01: Fresh Install (NSIS Installer & Per-User Installation)
* **Goal**: Verify that the Windows NSIS installer installs LibOne cleanly in per-user mode without administrative elevation prompts.
* **Preconditions**: Clean Windows environment without an existing installation in `%LOCALAPPDATA%\Programs\LibOne`.
* **Steps**:
  1. Locate `LibOne_0.1.0_x64-setup.exe`.
  2. Double-click the installer to start installation.
  3. Observe installation dialogs and progress.
  4. Note whether any UAC / Administrator elevation prompt appears.
  5. Once installation finishes, inspect:
     - Desktop shortcut
     - Start Menu shortcut
     - Installation folder at `%LOCALAPPDATA%\Programs\LibOne`
* **Expected Outcome**:
  - Installer runs smoothly in current-user mode without requiring administrator rights.
  - Desktop and Start Menu shortcuts are created with the high-resolution LibOne icon.
  - Application binary `libone.exe` and uninstaller `Uninstall.exe` reside in `%LOCALAPPDATA%\Programs\LibOne`.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-02: Upgrade Installation (Preserving User Data & Automatic Migration)
* **Goal**: Verify that running the installer over an existing installation upgrades binaries while preserving all user data and applying database migrations.
* **Preconditions**: A previous installation of LibOne exists with registered scan roots, indexed files, tags, collections, and notes.
* **Steps**:
  1. Launch existing LibOne instance and confirm indexed files, collections, and notes exist.
  2. Close the application.
  3. Run the new `LibOne_0.1.0_x64-setup.exe` without running an uninstaller first.
  4. Complete the installer prompts.
  5. Launch the upgraded LibOne from the Start Menu.
  6. Navigate to **Settings -> Diagnostics**.
  7. Check the displayed **App Version** and **Schema Version**.
* **Expected Outcome**:
  - Installer overwrites existing binaries cleanly without file lock conflicts.
  - Application launches and runs `db.migrate()` automatically inside a transaction.
  - Schema Version reports `4`.
  - All existing scan roots, indexed documents, tags, collections, notes, and suggestions remain 100% intact.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-03: First Launch & Initial State Verification
* **Goal**: Verify first application launch after installation, clean UI rendering, database creation, and absence of external network prompts.
* **Preconditions**: Freshly installed LibOne.
* **Steps**:
  1. Launch LibOne from the desktop shortcut or Start Menu.
  2. Observe window dimensions (default 1180x780, min 860x600), title bar ("LibOne - Local Document Library"), and clean dark theme.
  3. Verify that Windows Defender Firewall does **not** show an "Allow access" prompt (since LibOne binds zero local network sockets and makes zero network calls).
  4. Open File Explorer and inspect `%APPDATA%\com.libone.app\`.
  5. Navigate to **Settings -> Diagnostics** in the UI.
* **Expected Outcome**:
  - Application loads instantaneously with WebView2.
  - SQLite database `libone.db`, `libone.db-wal`, and `libone.db-shm` are created in `%APPDATA%\com.libone.app\`.
  - Diagnostics screen shows App Version `0.1.0`, Schema Version `4`, and Database Integrity `OK`.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-04: Scan Traversal and Real-Time Watcher Behavior
* **Goal**: Verify recursive directory traversal, live progress metrics, and real-time watcher updates when files are created, renamed, or deleted.
* **Preconditions**: Create test folder `D:\LibOne_Test_Docs` with subfolders and documents (`doc1.pdf`, `report.docx`, `notes.txt`).
* **Steps**:
  1. In LibOne, click **Add** in the Folders sidebar and select `D:\LibOne_Test_Docs`.
  2. Go to **Scan Screen** and click **Start Scan**.
  3. Verify live metrics update (directories visited, files found, duration in ms).
  4. Return to **Library** view to confirm indexed files appear in table.
  5. In Windows File Explorer, create a new document `new_budget.xlsx` in `D:\LibOne_Test_Docs`.
  6. Rename `notes.txt` to `renamed_notes.txt`.
  7. Delete `doc1.pdf`.
  8. Observe the header watcher indicator pill and the file table.
* **Expected Outcome**:
  - Initial scan completes quickly and accurately.
  - Watcher detects file creation; `new_budget.xlsx` appears automatically after debouncing (~500 ms).
  - Renamed file updates to `renamed_notes.txt` while maintaining its database identity via NTFS File ID.
  - Deleted file is marked `missing` (preserving record for auditing, never silently purged).
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-05: Collections, Tags, Notes, and Suggestions Persistence
* **Goal**: Verify that organizational metadata (tags, collections, markdown notes, organization suggestions) persists across app restarts.
* **Preconditions**: An active library with indexed files.
* **Steps**:
  1. In the file table, select a document and attach tag `"Urgent"` (color: Red).
  2. Create a collection `"Q4 Audits"` and add the document to it.
  3. Open document details modal and write a note: `"Audited and signed off by Lead on 2026-11-28"`.
  4. Open the **Suggestions** tab, review suggestions, and approve or dismiss a proposal.
  5. Close LibOne completely (exit window).
  6. Re-launch LibOne.
  7. Filter file table by tag `"Urgent"` and open collection `"Q4 Audits"`.
  8. Open the document detail modal to check the note.
* **Expected Outcome**:
  - Tag `"Urgent"` and collection `"Q4 Audits"` persist and properly filter the table.
  - Document note displays exact text and original creation timestamp.
  - Suggestion review statuses remain preserved.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-06: Database Backup, Restore, and Integrity Verification
* **Goal**: Verify local atomic database backup, pre-restore safety snapshots, integrity validation, and atomic restore.
* **Preconditions**: An active library with indexed files, collections, tags, and notes.
* **Steps**:
  1. Navigate to **Settings -> Backup & Restore**.
  2. Click **Create Local Backup**.
  3. In the Windows native save dialog, choose `D:\LibOne_Backup_Snapshot.db`.
  4. Verify backup completion notification.
  5. Deliberately modify the library: remove a tag link and delete a collection.
  6. Click **Restore from Backup** and select `D:\LibOne_Backup_Snapshot.db`.
  7. Review the pre-restore confirmation dialog. Confirm restore.
  8. Inspect `%APPDATA%\com.libone.app\` for safety backup `libone_safety_pre_restore_*.db`.
  9. Reload the library view.
* **Expected Outcome**:
  - Backup is written atomically via sibling temporary file.
  - Safety backup of current database is automatically created before restore begins.
  - Restore validates SQLite header, schema version (`4`), and integrity before replacing database.
  - Library reverts completely to the exact state at backup time (restoring the removed tag and collection).
  - User document files on disk are completely untouched.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-07: Clean Uninstallation & User Document Safety
* **Goal**: Verify that uninstallation removes application binaries while offering optional metadata preservation, and NEVER alters user documents.
* **Preconditions**: LibOne is installed with indexed documents stored in `D:\LibOne_Test_Docs`.
* **Steps**:
  1. Open Windows **Settings -> Apps -> Installed Apps**.
  2. Find **LibOne** and click **Uninstall**.
  3. When the NSIS uninstaller dialog appears:
     *"Do you want to remove LibOne library metadata and settings stored in AppData...?"*
  4. Click **No** (preserve metadata).
  5. Verify uninstallation finishes.
  6. Check `%LOCALAPPDATA%\Programs\LibOne`: verify it has been removed.
  7. Check Start Menu and desktop: verify shortcuts are removed.
  8. Check `%APPDATA%\com.libone.app`: verify `libone.db` is preserved.
  9. Check `D:\LibOne_Test_Docs`: verify all user documents on disk are 100% intact.
* **Expected Outcome**:
  - Application binaries, shortcuts, and registry keys are cleanly deleted.
  - AppData metadata is preserved as requested by the user.
  - Original user documents on disk are NEVER moved, modified, or deleted.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-08: Low Disk Space Behavior and Diagnostics Warnings
* **Goal**: Verify that low disk space is safely detected and reported in Diagnostics without application crash or database corruption.
* **Preconditions**: LibOne running on Windows.
* **Steps**:
  1. Open **Settings -> Diagnostics**.
  2. Inspect the **Storage Volume & Disk Health** panel.
  3. Verify available storage bytes, total storage bytes, and free storage percentage are reported.
  4. If running on a drive with < 2 GB or < 5% available space (or simulating via a small volume/RAM disk):
     - Verify the amber/red low-space warning badge is displayed.
     - Verify actionable warning message is shown.
  5. Attempt a library scan and database backup.
* **Expected Outcome**:
  - Volume metrics are read safely via Win32 `GetDiskFreeSpaceExW`.
  - Low-space condition triggers a clear warning banner.
  - SQLite WAL and transactional rollback protect database from corruption if disk writes fail.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-09: Disconnected External Drive / Offline Folder Handling
* **Goal**: Verify graceful error handling and metadata retention when an indexed USB drive or network share is disconnected.
* **Preconditions**: A removable USB drive or external disk (e.g. `E:\External_Docs`) containing test documents.
* **Steps**:
  1. Register `E:\External_Docs` as a scan root in LibOne and scan it.
  2. Add tags and notes to files indexed from the external drive.
  3. Safely disconnect/unplug the USB drive from the Windows PC.
  4. In LibOne, trigger **Scan All Roots** or scan the disconnected root.
  5. Attempt to open one of the external files using **Open Original**.
  6. Go to **Settings -> Diagnostics -> Check Integrity**.
  7. Reconnect the USB drive to the computer.
  8. Rescan the root.
* **Expected Outcome**:
  - Scanning the missing drive fails non-fatally with clear status: `"Path does not exist"` or `"unavailable"`.
  - Opening an inaccessible file shows clear error dialog without crashing.
  - Database records, tags, and notes for the offline files are **preserved** (never silently deleted).
  - Integrity check flags missing files without altering the database.
  - Once reconnected, rescanning seamlessly re-verifies files and updates their status back to `indexed`.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

### TC-10: Clean Reinstall & Metadata Re-attachment
* **Goal**: Verify that reinstalling LibOne after an uninstallation with preserved metadata immediately re-attaches to existing library data.
* **Preconditions**: LibOne uninstalled via TC-07 with AppData metadata preserved in `%APPDATA%\com.libone.app`.
* **Steps**:
  1. Run `LibOne_0.1.0_x64-setup.exe` again.
  2. Complete the installation wizard.
  3. Launch LibOne.
  4. Inspect the Folders sidebar, Library dashboard, Tags, and Collections.
* **Expected Outcome**:
  - Application starts up instantly and re-opens `%APPDATA%\com.libone.app\libone.db`.
  - All previously registered folders, indexed documents, tags, collections, notes, and suggestions are immediately visible.
  - Background watchers re-attach to all enabled scan roots automatically.
* **Actual Result**: `[ ] Pass` / `[ ] Fail`  
  *Tester Notes*: ____________________________________________________

---

## 4. Human Tester Sign-Off Sheet

| Test Case | Description | Result | Tester Initials | Date Executed |
| :--- | :--- | :--- | :--- | :--- |
| **TC-01** | Fresh Install (NSIS Installer) | `PENDING` | | |
| **TC-02** | Upgrade Installation & Migration | `PENDING` | | |
| **TC-03** | First Launch & Initial State | `PENDING` | | |
| **TC-04** | Scan & Watcher Real-Time Behavior | `PENDING` | | |
| **TC-05** | Metadata & Suggestions Persistence | `PENDING` | | |
| **TC-06** | Backup, Restore, & Integrity Check | `PENDING` | | |
| **TC-07** | Clean Uninstallation & Safety | `PENDING` | | |
| **TC-08** | Low Disk Space Diagnostics | `PENDING` | | |
| **TC-09** | Disconnected Drive Resilience | `PENDING` | | |
| **TC-10** | Clean Reinstall & Re-attachment | `PENDING` | | |

**Overall Acceptance Sign-Off**: `[ ] APPROVED FOR RELEASE` / `[ ] REJECTED (BLOCKING ISSUES)`  
**Sign-off Signature**: ___________________________________________  
**Date**: _______________________________________________________
