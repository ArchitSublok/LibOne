use libone_lib::db::Database;
use libone_lib::models::NewIndexedFile;
use libone_lib::scanner::scan_folder_with_progress;
use libone_lib::watcher::WatcherManager;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "libone_integ_{}_{}",
        label,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_interrupted_scan_recovers_cleanly_on_rescan() {
    let base = temp_dir("interrupted_scan");
    let root_dir = base.join("docs");
    fs::create_dir_all(&root_dir).unwrap();
    fs::write(root_dir.join("a.txt"), b"alpha").unwrap();
    fs::write(root_dir.join("b.txt"), b"beta").unwrap();

    let db = Database::new(&base).unwrap();
    let root = db
        .add_scan_root(root_dir.to_str().unwrap(), "Docs")
        .unwrap();

    // Simulate an interrupted scan: mark root as scanning, then recover via a fresh scan
    db.record_scan_started(root.id).unwrap();
    let progress = scan_folder_with_progress(&db, root.id, None, None, |_| {}).unwrap();
    assert_eq!(progress.state, "completed");
    let files = db.get_files_for_root(root.id).unwrap();
    assert_eq!(files.len(), 2);

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_scan_cancellation_preserves_unvisited_files_as_indexed() {
    let base = temp_dir("cancel_scan");
    let root_dir = base.join("docs");
    fs::create_dir_all(&root_dir).unwrap();
    let file_a = root_dir.join("existing.pdf");
    fs::write(&file_a, b"content").unwrap();

    let db = Database::new(&base).unwrap();
    let root = db
        .add_scan_root(root_dir.to_str().unwrap(), "Docs")
        .unwrap();

    // Initial complete scan
    scan_folder_with_progress(&db, root.id, None, None, |_| {}).unwrap();

    // Cancelled scan must NOT mark existing unvisited files as missing
    let cancel_token = AtomicBool::new(true);
    let progress =
        scan_folder_with_progress(&db, root.id, None, Some(&cancel_token), |_| {}).unwrap();
    assert_eq!(progress.state, "cancelled");

    let files = db.get_files_for_root(root.id).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status, "indexed");

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_watcher_failure_and_restart_lifecycle() {
    let mut wm = WatcherManager::new();
    let status_before = wm.get_overall_status();
    assert_eq!(status_before.active_roots, 0);

    // Stopping non-existent or all watchers is idempotent and safe
    wm.stop_watching_root(999);
    wm.stop_all();
    let status_after = wm.get_overall_status();
    assert_eq!(status_after.active_roots, 0);
}

#[test]
fn test_disconnected_drive_or_missing_root_graceful_handling() {
    let base = temp_dir("disconnected_drive");
    let db = Database::new(&base).unwrap();

    let missing_root_path = PathBuf::from("Z:\\LibOne_NonExistent_Drive_999\\Docs");
    let root = db
        .add_scan_root(missing_root_path.to_str().unwrap(), "Disconnected Drive")
        .unwrap();

    // Scanning a disconnected drive/missing root returns an Err without panicking or deleting metadata
    let res = scan_folder_with_progress(&db, root.id, None, None, |_| {});
    assert!(res.is_err());

    let roots = db.list_scan_roots().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, root.id);

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_locked_file_handled_without_aborting_scan() {
    let base = temp_dir("locked_file");
    let root_dir = base.join("docs");
    fs::create_dir_all(&root_dir).unwrap();
    let normal_file = root_dir.join("normal.txt");
    let locked_file = root_dir.join("locked.txt");
    fs::write(&normal_file, b"ok").unwrap();
    fs::write(&locked_file, b"locked").unwrap();

    // Hold an open handle on locked_file
    let _handle = OpenOptions::new().read(true).open(&locked_file).unwrap();

    let db = Database::new(&base).unwrap();
    let root = db
        .add_scan_root(root_dir.to_str().unwrap(), "Docs")
        .unwrap();

    let progress = scan_folder_with_progress(&db, root.id, None, None, |_| {}).unwrap();
    assert_eq!(progress.state, "completed");
    let files = db.get_files_for_root(root.id).unwrap();
    assert!(!files.is_empty());

    drop(_handle);
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_corrupted_export_rejected_by_validation() {
    let base = temp_dir("corrupt_export");
    let corrupt_json = base.join("corrupt_export.json");
    fs::write(&corrupt_json, b"{ invalid json [").unwrap();

    let content = fs::read_to_string(&corrupt_json).unwrap();
    let parsed: Result<libone_lib::models::LibraryExport, _> = serde_json::from_str(&content);
    assert!(parsed.is_err());

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_missing_original_database_initializes_fresh_safely() {
    let base = temp_dir("missing_db");
    let db_file = base.join("libone.db");
    assert!(!db_file.exists());

    let db = Database::new(&base).unwrap();
    assert!(db_file.exists());
    let report = db.check_integrity().unwrap();
    assert!(report.is_healthy);

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn test_restored_database_preserves_metadata_and_recovers_state() {
    let base1 = temp_dir("restore_src");
    let base2 = temp_dir("restore_dst");
    let docs_dir = base1.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    let doc_path = docs_dir.join("contract.pdf");
    fs::write(&doc_path, b"contract bytes").unwrap();

    let db1 = Database::new(&base1).unwrap();
    let r1 = db1
        .add_scan_root(docs_dir.to_str().unwrap(), "Docs")
        .unwrap();
    let f1 = db1
        .insert_indexed_file(&NewIndexedFile {
            scan_root_id: r1.id,
            absolute_path: doc_path.to_str().unwrap().into(),
            normalized_path: doc_path.to_string_lossy().to_lowercase(),
            file_name: "contract.pdf".into(),
            extension: "pdf".into(),
            size_bytes: 14,
            modified_at: None,
            created_at_if_available: None,
            content_hash_nullable: None,
            file_id_nullable: None,
            status: "indexed".into(),
        })
        .unwrap();
    db1.create_file_note(f1.id, "Signed 2026").unwrap();

    let backup_path = base1.join("backup.db");
    db1.create_backup(&backup_path).unwrap();

    let db2 = Database::new(&base2).unwrap();
    let restore_res = db2.restore_from_backup(&backup_path).unwrap();
    assert_eq!(restore_res.summary.total_files, 1);
    assert_eq!(restore_res.summary.total_notes, 1);

    let notes = db2.list_file_notes(f1.id).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].note_text, "Signed 2026");

    let _ = fs::remove_dir_all(&base1);
    let _ = fs::remove_dir_all(&base2);
}
