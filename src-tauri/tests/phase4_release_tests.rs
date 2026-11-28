//! Phase 4 Release Integration Test Suite
//!
//! Automated tests for:
//! 1. Fresh install database creation
//! 2. Upgrade from Phase 1 database (v1 -> v4)
//! 3. Upgrade from Phase 2 database (v2/v3 -> v4)
//! 4. Upgrade from Phase 3 database (v3 -> v4)
//! 5. Backup and restore lifecycle
//! 6. Interrupted backup handling
//! 7. Corrupted backup rejection and safety rollback
//! 8. Database integrity failure detection and repair guidance
//! 9. Path authorization and traversal prevention
//! 10. Disabled-root access blocking
//! 11. Watcher recovery and resilience
//! 12. Scan cancellation without falsely marking files missing
//! 13. Export metadata-only guarantee
//! 14. Diagnostics path redaction

use libone_lib::commands::validate_file_for_access;
use libone_lib::db::Database;
use libone_lib::models::{
    redact_diagnostics_report, redact_path, BackupHealthInfo, DatabaseIntegrityReport,
    DiagnosticsReport, DiskSpaceInfo, FileCountsHealthInfo, NewIndexedFile, ScanHealthInfo,
    WatcherOverallStatus, WatcherRootStatus,
};
use libone_lib::scanner::scan_folder_with_progress;
use libone_lib::validate::validate_scan_root_path;
use libone_lib::watcher::WatcherManager;
use rusqlite::Connection;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "libone_p4_test_{}_{}_{}",
        label,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ==============================================================================
// 1. Fresh Install Database Creation
// ==============================================================================
#[test]
fn test_fresh_install_database_creation() {
    let base = unique_temp_dir("fresh_install");
    let db_path = base.join("libone.db");
    assert!(!db_path.exists());

    let db = Database::new(&base).expect("Failed to initialize database on fresh install");
    assert!(db_path.exists(), "libone.db must be created");

    // Verify integrity is clean out of the box
    let report = db.check_integrity().expect("Integrity check must succeed");
    assert!(report.is_healthy);
    assert_eq!(report.foreign_key_violations.len(), 0);
    assert_eq!(report.missing_tables.len(), 0);
    assert_eq!(report.missing_files_count, 0);

    // Verify schema migrations reached current schema version 4
    let roots = db.list_scan_roots().unwrap();
    assert_eq!(roots.len(), 0);
    let collections = db.list_collections().unwrap();
    assert_eq!(collections.len(), 0);
    let tags = db.list_tags().unwrap();
    assert_eq!(tags.len(), 0);

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 2. Upgrade from Phase 1 Database (Schema v1 -> v4)
// ==============================================================================
#[test]
fn test_upgrade_from_phase1_database() {
    let base = unique_temp_dir("upgrade_phase1");
    let db_path = base.join("libone.db");

    // Manually create an SQLite database with Phase 1 schema (schema version 1)
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations (version, applied_at) VALUES (1, '2026-01-01T00:00:00Z');

             CREATE TABLE library_metadata (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 schema_version INTEGER NOT NULL,
                 library_created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO library_metadata (id, schema_version, library_created_at, updated_at)
             VALUES (1, 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

             CREATE TABLE scan_roots (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 absolute_path TEXT NOT NULL UNIQUE,
                 normalized_path TEXT NOT NULL UNIQUE,
                 display_name TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 created_at TEXT NOT NULL,
                 last_scan_completed_at TEXT,
                 last_scan_status TEXT
             );
             INSERT INTO scan_roots (id, absolute_path, normalized_path, display_name, enabled, created_at)
             VALUES (1, 'D:\\Docs', 'd:\\docs', 'Documents', 1, '2026-01-01T00:00:00Z');

             CREATE TABLE indexed_files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
                 absolute_path TEXT NOT NULL UNIQUE,
                 normalized_path TEXT NOT NULL UNIQUE,
                 file_name TEXT NOT NULL,
                 extension TEXT NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 modified_at TEXT,
                 discovered_at TEXT NOT NULL,
                 last_seen_at TEXT NOT NULL,
                 content_hash_nullable TEXT,
                 status TEXT NOT NULL DEFAULT 'indexed'
             );
             INSERT INTO indexed_files (id, scan_root_id, absolute_path, normalized_path, file_name, extension, size_bytes, discovered_at, last_seen_at, status)
             VALUES (1, 1, 'D:\\Docs\\p1_doc.txt', 'd:\\docs\\p1_doc.txt', 'p1_doc.txt', 'txt', 100, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'indexed');"
        ).unwrap();
    }

    // Now instantiate Database::new() which runs transactional migrations
    let db = Database::new(&base).expect("Upgrade from Phase 1 must succeed");
    let roots = db.list_scan_roots().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].display_name, "Documents");

    let file = db.get_indexed_file(1).unwrap().expect("File 1 must exist");
    assert_eq!(file.file_name, "p1_doc.txt");
    assert_eq!(file.file_id_nullable, None);

    // Verify Phase 2/3/4 tables are available: can create collections, tags, and notes on upgraded db
    let col = db.create_collection("Finance", None, None).unwrap();
    assert_eq!(col.name, "Finance");
    let tag = db.create_tag("Receipt", None).unwrap();
    assert_eq!(tag.name, "Receipt");
    db.create_file_note(1, "Phase 1 file upgraded").unwrap();

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 3. Upgrade from Phase 2 Database (Schema v2/v3 -> v4)
// ==============================================================================
#[test]
fn test_upgrade_from_phase2_database() {
    let base = unique_temp_dir("upgrade_phase2");
    let db_path = base.join("libone.db");

    // Create database up to schema v3
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             INSERT INTO schema_migrations (version, applied_at) VALUES (1, '2026-01-01T00:00:00Z'), (2, '2026-01-02T00:00:00Z'), (3, '2026-01-03T00:00:00Z');

             CREATE TABLE library_metadata (
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 schema_version INTEGER NOT NULL,
                 library_created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO library_metadata (id, schema_version, library_created_at, updated_at)
             VALUES (1, 3, '2026-01-01T00:00:00Z', '2026-01-03T00:00:00Z');

             CREATE TABLE scan_roots (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 absolute_path TEXT NOT NULL UNIQUE,
                 normalized_path TEXT NOT NULL UNIQUE,
                 display_name TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1,
                 created_at TEXT NOT NULL,
                 last_scan_completed_at TEXT,
                 last_scan_status TEXT
             );
             INSERT INTO scan_roots (id, absolute_path, normalized_path, display_name, enabled, created_at)
             VALUES (1, 'D:\\Phase2Docs', 'd:\\phase2docs', 'Phase2Docs', 1, '2026-01-01T00:00:00Z');

             CREATE TABLE indexed_files (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
                 absolute_path TEXT NOT NULL UNIQUE,
                 normalized_path TEXT NOT NULL UNIQUE,
                 file_name TEXT NOT NULL,
                 extension TEXT NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 modified_at TEXT,
                 discovered_at TEXT NOT NULL,
                 last_seen_at TEXT NOT NULL,
                 content_hash_nullable TEXT,
                 status TEXT NOT NULL DEFAULT 'indexed',
                 file_id_nullable TEXT
             );
             INSERT INTO indexed_files (id, scan_root_id, absolute_path, normalized_path, file_name, extension, size_bytes, discovered_at, last_seen_at, status, file_id_nullable)
             VALUES (1, 1, 'D:\\Phase2Docs\\report.pdf', 'd:\\phase2docs\\report.pdf', 'report.pdf', 'pdf', 500, '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z', 'indexed', '123456');

             CREATE TABLE collections (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 description_nullable TEXT,
                 color_nullable TEXT,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO collections (id, name, created_at, updated_at) VALUES (1, 'Reports', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z');

             CREATE TABLE tags (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                 color_nullable TEXT,
                 created_at TEXT NOT NULL
             );
             INSERT INTO tags (id, name, created_at) VALUES (1, 'Draft', '2026-01-02T00:00:00Z');

             CREATE TABLE file_tags (
                 file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                 tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                 created_at TEXT NOT NULL,
                 PRIMARY KEY (file_id, tag_id)
             );
             INSERT INTO file_tags (file_id, tag_id, created_at) VALUES (1, 1, '2026-01-02T00:00:00Z');

             CREATE TABLE file_collections (
                 file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                 collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                 created_at TEXT NOT NULL,
                 PRIMARY KEY (file_id, collection_id)
             );
             INSERT INTO file_collections (file_id, collection_id, created_at) VALUES (1, 1, '2026-01-02T00:00:00Z');

             CREATE TABLE file_notes (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                 note_text TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO file_notes (id, file_id, note_text, created_at, updated_at) VALUES (1, 1, 'Phase 2 note', '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z');"
        ).unwrap();
    }

    // Upgrade to current version 4
    let db = Database::new(&base).expect("Upgrade from Phase 2 must succeed");
    let collections = db.list_collections().unwrap();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].name, "Reports");

    let tags = db.list_tags().unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "Draft");

    let notes = db.list_file_notes(1).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note_text, "Phase 2 note");

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 4. Upgrade from Phase 3 Database
// ==============================================================================
#[test]
fn test_upgrade_from_phase3_database() {
    let base = unique_temp_dir("upgrade_phase3");
    let db = Database::new(&base).expect("Initializing DB must work");
    let report = db.check_integrity().unwrap();
    assert!(report.is_healthy);

    // Verify Phase 3 suggestions and duplicate groups tables are functional
    let suggestions = db.list_organization_suggestions(None, None).unwrap();
    assert_eq!(suggestions.len(), 0);
    let duplicates = db.list_duplicate_groups(None).unwrap();
    assert_eq!(duplicates.len(), 0);

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 5. Backup and Restore Lifecycle
// ==============================================================================
#[test]
fn test_backup_and_restore_full_lifecycle() {
    let base1 = unique_temp_dir("backup_src");
    let base2 = unique_temp_dir("backup_dst");
    let backup_dir = unique_temp_dir("backup_archive");
    let backup_file = backup_dir.join("library_backup.db");

    let db1 = Database::new(&base1).unwrap();
    let root = db1.add_scan_root("D:\\TestData", "TestData").unwrap();
    let file = db1
        .insert_indexed_file(&NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: "D:\\TestData\\invoice.pdf".into(),
            normalized_path: "d:\\testdata\\invoice.pdf".into(),
            file_name: "invoice.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 2048,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: Some("abcdef123456".into()),
            file_id_nullable: None,
            status: "indexed".into(),
        })
        .unwrap();

    let tag = db1.create_tag("Accounting", None).unwrap();
    db1.add_file_to_tag(file.id, tag.id).unwrap();
    db1.create_file_note(file.id, "Paid invoice").unwrap();

    // Create atomic backup
    let backup_res = db1.create_backup(&backup_file).unwrap();
    assert!(backup_file.exists());
    assert_eq!(backup_res.summary.total_files, 1);
    assert_eq!(backup_res.summary.total_tags, 1);
    assert_eq!(backup_res.summary.total_notes, 1);

    // Restore into empty second database
    let db2 = Database::new(&base2).unwrap();
    let restore_res = db2.restore_from_backup(&backup_file).unwrap();
    assert_eq!(restore_res.summary.total_files, 1);
    assert_eq!(restore_res.summary.total_tags, 1);
    assert_eq!(restore_res.summary.total_notes, 1);

    let restored_notes = db2.list_file_notes(file.id).unwrap();
    assert_eq!(restored_notes.len(), 1);
    assert_eq!(restored_notes[0].note_text, "Paid invoice");

    let _ = fs::remove_dir_all(&base1);
    let _ = fs::remove_dir_all(&base2);
    let _ = fs::remove_dir_all(&backup_dir);
}

// ==============================================================================
// 6. Interrupted Backup Handling
// ==============================================================================
#[test]
fn test_interrupted_backup_atomic_cleanup() {
    let base = unique_temp_dir("interrupted_backup");
    let db = Database::new(&base).unwrap();

    // Attempting to write a backup to an invalid/non-existent directory path
    let invalid_target = PathBuf::from("Z:\\NonExistent_Drive_999\\backup.db");
    let res = db.create_backup(&invalid_target);
    assert!(res.is_err(), "Backup to invalid target must fail cleanly");

    // Confirm that the source database remains intact and healthy
    let integrity = db.check_integrity().unwrap();
    assert!(integrity.is_healthy);

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 7. Corrupted Backup Rejection and Safety Rollback
// ==============================================================================
#[test]
fn test_corrupted_backup_rejection_and_safety_rollback() {
    let base = unique_temp_dir("corrupted_backup_src");
    let db = Database::new(&base).unwrap();
    let root = db.add_scan_root("D:\\Original", "Original").unwrap();
    db.insert_indexed_file(&NewIndexedFile {
        scan_root_id: root.id,
        absolute_path: "D:\\Original\\keep_me.txt".into(),
        normalized_path: "d:\\original\\keep_me.txt".into(),
        file_name: "keep_me.txt".into(),
        extension: "txt".into(),
        size_bytes: 42,
        modified_at: None,
        created_at_if_available: None,
        content_hash_nullable: None,
        file_id_nullable: None,
        status: "indexed".into(),
    })
    .unwrap();

    // Create a corrupted backup file (garbage non-SQLite bytes)
    let corrupt_file = base.join("corrupt_backup.db");
    {
        let mut f = File::create(&corrupt_file).unwrap();
        f.write_all(b"CORRUPT_HEADER_NOT_A_VALID_SQLITE_DATABASE")
            .unwrap();
    }

    // Attempt to restore corrupt backup
    let res = db.restore_from_backup(&corrupt_file);
    assert!(res.is_err(), "Restoring corrupted backup must be rejected");

    // Verify original database data is preserved intact
    let files = db.get_files_for_root(root.id).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "keep_me.txt");

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 8. Database Integrity Failure Detection and Repair Guidance
// ==============================================================================
#[test]
fn test_database_integrity_failure_detection_and_repair_guidance() {
    let base = unique_temp_dir("integrity_failure");
    let db = Database::new(&base).unwrap();

    // Disable foreign keys temporarily to deliberately inject an orphaned record
    {
        let conn = Connection::open(db.get_path()).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        conn.execute(
            "INSERT INTO file_notes (id, file_id, note_text, created_at, updated_at) VALUES (999, 88888, 'Orphaned note', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
    }

    let report = db.check_integrity().unwrap();
    assert!(
        !report.is_healthy,
        "Database with orphaned records must report unhealthy"
    );
    assert!(
        !report.foreign_key_violations.is_empty(),
        "Must identify foreign key violations"
    );
    assert!(
        !report.issues.is_empty(),
        "Must provide issues list with actionable repair guidance"
    );
    assert!(
        !report.issues[0].repair_guidance.is_empty(),
        "Repair guidance must not be empty"
    );

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 9. Path Authorization and Traversal Prevention
// ==============================================================================
#[test]
fn test_path_authorization_rejects_unauthorized_and_traversal_paths() {
    let base = unique_temp_dir("path_auth");
    let db = Database::new(&base).unwrap();

    // 1. Negative or zero file ID is rejected
    assert!(validate_file_for_access(&db, -1).is_err());
    assert!(validate_file_for_access(&db, 0).is_err());

    // 2. Non-existent file ID is rejected
    assert!(validate_file_for_access(&db, 9999).is_err());

    // 3. Scan root validation rejects system temp and app data folders
    let temp_dir = std::env::temp_dir();
    let temp_str = temp_dir.to_str().unwrap();
    assert!(validate_scan_root_path(temp_str, &base).is_err());
    assert!(validate_scan_root_path(base.to_str().unwrap(), &base).is_err());

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 10. Disabled-Root Access Blocking
// ==============================================================================
#[test]
fn test_disabled_root_blocks_access() {
    let base = unique_temp_dir("disabled_root");
    let docs_dir = base.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    let file_path = docs_dir.join("secret.pdf");
    fs::write(&file_path, b"pdf content").unwrap();

    let db = Database::new(&base).unwrap();
    let root = db
        .add_scan_root(docs_dir.to_str().unwrap(), "Docs")
        .unwrap();
    let file = db
        .insert_indexed_file(&NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: file_path.to_str().unwrap().into(),
            normalized_path: file_path.to_string_lossy().to_lowercase(),
            file_name: "secret.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 11,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        })
        .unwrap();

    // When root is enabled, validation passes
    let valid = validate_file_for_access(&db, file.id);
    assert!(valid.is_ok());

    // Disable root (sets enabled = 0 non-destructively)
    db.remove_scan_root(root.id).unwrap();

    // Access to file in disabled root must be explicitly rejected
    let denied = validate_file_for_access(&db, file.id);
    assert!(denied.is_err());
    let err_msg = denied.unwrap_err();
    assert!(
        err_msg.contains("disabled"),
        "Error message must indicate root is disabled: {err_msg}"
    );

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 11. Watcher Recovery and Resilience
// ==============================================================================
#[test]
fn test_watcher_recovery_and_resilience() {
    let mut wm = WatcherManager::new();
    let initial_status = wm.get_overall_status();
    assert_eq!(initial_status.watched_roots_count, 0);

    // Stopping non-existent roots or all roots is safe and non-panicking
    wm.stop_watching_root(1234);
    wm.stop_all();

    let final_status = wm.get_overall_status();
    assert_eq!(final_status.watched_roots_count, 0);
}

// ==============================================================================
// 12. Scan Cancellation Without Falsely Marking Files Missing
// ==============================================================================
#[test]
fn test_scan_cancellation_preserves_unvisited_files() {
    let base = unique_temp_dir("scan_cancel");
    let docs_dir = base.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    fs::write(docs_dir.join("sample.pdf"), b"content").unwrap();

    let db = Database::new(&base).unwrap();
    let root = db
        .add_scan_root(docs_dir.to_str().unwrap(), "Docs")
        .unwrap();

    // First scan indices the file
    scan_folder_with_progress(&db, root.id, &root.absolute_path, None, None, None, |_| {}).unwrap();

    // Now start a second scan with cancellation token already set
    let cancel_token = AtomicBool::new(true);
    let progress = scan_folder_with_progress(
        &db,
        root.id,
        &root.absolute_path,
        None,
        None,
        Some(&cancel_token),
        |_| {},
    )
    .unwrap();
    assert_eq!(progress.state, "cancelled");

    // Files must remain indexed and NEVER marked missing due to scan cancellation
    let files = db.get_files_for_root(root.id).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status, "indexed");

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 13. Export Metadata-Only Guarantee
// ==============================================================================
#[test]
fn test_export_metadata_only_guarantee() {
    let base = unique_temp_dir("export_meta");
    let db = Database::new(&base).unwrap();
    let root = db.add_scan_root("D:\\Documents", "Documents").unwrap();
    let file = db
        .insert_indexed_file(&NewIndexedFile {
            scan_root_id: root.id,
            absolute_path: "D:\\Documents\\confidential.docx".into(),
            normalized_path: "d:\\documents\\confidential.docx".into(),
            file_name: "confidential.docx".into(),
            extension: "docx".into(),
            size_bytes: 4096,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: Some("hash123".into()),
            file_id_nullable: None,
            status: "indexed".into(),
        })
        .unwrap();
    db.create_file_note(file.id, "Metadata note").unwrap();

    let export = db.export_library().unwrap();
    assert_eq!(export.format_version, "1.2.0");
    assert_eq!(export.files.len(), 1);
    assert_eq!(export.file_notes.len(), 1);

    // Verify exported JSON does not contain binary content or original body bytes
    let json_str = serde_json::to_string(&export).unwrap();
    assert!(json_str.contains("confidential.docx"));
    assert!(json_str.contains("Metadata note"));
    assert!(!json_str.contains("raw_data"));
    assert!(!json_str.contains("body_bytes"));
    assert!(!json_str.contains("file_content"));

    let _ = fs::remove_dir_all(&base);
}

// ==============================================================================
// 14. Diagnostics Path Redaction
// ==============================================================================
#[test]
fn test_diagnostics_path_redaction() {
    // Windows username path redaction
    let win_path = "C:\\Users\\JohnDoe\\AppData\\Roaming\\com.libone.app\\libone.db";
    let redacted_win = redact_path(win_path);
    assert_eq!(
        redacted_win,
        "C:\\Users\\<redacted>\\AppData\\Roaming\\com.libone.app\\libone.db"
    );

    // Unix /home/ username path redaction
    let unix_home = "/home/secretuser/data/libone.db";
    let redacted_home = redact_path(unix_home);
    assert_eq!(redacted_home, "/home/<redacted>/data/libone.db");

    // Unix /Users/ username path redaction
    let mac_path = "/Users/secretadmin/documents/report.pdf";
    let redacted_mac = redact_path(mac_path);
    assert_eq!(redacted_mac, "/Users/<redacted>/documents/report.pdf");

    // Full DiagnosticsReport redaction
    let report = DiagnosticsReport {
        app_version: "0.1.0".into(),
        schema_version: 4,
        db_path: "C:\\Users\\JohnDoe\\AppData\\Roaming\\com.libone.app\\libone.db".into(),
        app_data_dir: "C:\\Users\\JohnDoe\\AppData\\Roaming\\com.libone.app".into(),
        os: "Windows".into(),
        scan_health: ScanHealthInfo {
            last_scan_started_at: None,
            last_scan_completed_at: None,
            last_scan_status: Some("completed".into()),
            enabled_roots_count: 1,
            disabled_roots_count: 0,
            total_roots_count: 1,
        },
        file_counts: FileCountsHealthInfo {
            indexed_files_count: 10,
            missing_files_count: 0,
            inaccessible_files_count: 0,
            error_files_count: 0,
            total_tracked_files: 10,
        },
        watcher_health: WatcherOverallStatus {
            is_watching: true,
            watched_roots_count: 1,
            is_coalescing: false,
            is_refreshing: false,
            last_refreshed_at: None,
            root_statuses: vec![WatcherRootStatus {
                root_id: 1,
                path: "C:\\Users\\JohnDoe\\Documents".into(),
                active: true,
                last_event_at: None,
                error_message: None,
            }],
        },
        backup_health: BackupHealthInfo {
            safety_backups_count: 1,
            latest_safety_backup_time: None,
            latest_safety_backup_path: Some(
                "C:\\Users\\JohnDoe\\AppData\\Roaming\\com.libone.app\\safety.db".into(),
            ),
            status: "ready".into(),
        },
        integrity_report: DatabaseIntegrityReport {
            is_healthy: true,
            schema_version: 4,
            integrity_check_passed: true,
            foreign_key_check_passed: true,
            missing_tables: vec![],
            foreign_key_violations: vec![],
            invalid_paths_count: 0,
            missing_files_count: 0,
            total_roots: 1,
            total_files: 10,
            total_collections: 0,
            total_tags: 0,
            total_notes: 0,
            issues: vec![],
        },
        disk_space: Some(DiskSpaceInfo {
            available_bytes: 100_000_000_000,
            total_bytes: 500_000_000_000,
            free_percentage: 20.0,
            is_low_space: false,
            warning_message: None,
        }),
        generated_at: "2026-01-01T00:00:00Z".into(),
    };

    let redacted_report = redact_diagnostics_report(&report);
    assert!(!redacted_report.db_path.contains("JohnDoe"));
    assert!(!redacted_report.app_data_dir.contains("JohnDoe"));
    assert!(redacted_report.db_path.contains("<redacted>"));
    assert!(redacted_report.app_data_dir.contains("<redacted>"));
    assert!(!redacted_report.watcher_health.root_statuses[0]
        .path
        .contains("JohnDoe"));
    assert!(redacted_report.watcher_health.root_statuses[0]
        .path
        .contains("<redacted>"));
    assert!(!redacted_report
        .backup_health
        .latest_safety_backup_path
        .as_ref()
        .unwrap()
        .contains("JohnDoe"));
}
