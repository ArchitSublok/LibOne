//! The local SQLite repository. SQL is deliberately contained here, away from Tauri commands.

use crate::models::{
    ExportSummary, IndexedFile, LibraryExport, NewIndexedFile, ScanRoot, ScanStats,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result, Transaction};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: i64 = 2;

#[derive(Clone)]
pub struct Database {
    db_path: PathBuf,
}

impl Database {
    /// Creates app data if necessary and migrates the local metadata database transactionally.
    pub fn new<P: AsRef<Path>>(app_data_dir: P) -> Result<Self> {
        std::fs::create_dir_all(app_data_dir.as_ref()).map_err(io_error)?;
        let db = Self {
            db_path: app_data_dir.as_ref().join("libone.db"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn get_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;",
        )?;
        Ok(conn)
    }

    fn migrate(&self) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);")?;
        let current: i64 = tx.query_row(
            "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;

        if current < 1 {
            if legacy_schema(&tx)? {
                tx.execute_batch("ALTER TABLE scan_roots RENAME TO legacy_scan_roots; ALTER TABLE files RENAME TO legacy_files;")?;
            }
            create_schema(&tx)?;
            if table_exists(&tx, "legacy_scan_roots")? {
                tx.execute_batch("INSERT INTO scan_roots (id,absolute_path,normalized_path,display_name,enabled,created_at,last_scan_completed_at,last_scan_status)
                    SELECT id,path,lower(path),label,1,added_at,last_scanned_at,status FROM legacy_scan_roots;
                    INSERT INTO indexed_files (id,scan_root_id,absolute_path,normalized_path,file_name,extension,size_bytes,modified_at,discovered_at,last_seen_at,content_hash_nullable,status)
                    SELECT id,scan_root_id,path,lower(path),filename,extension,size_bytes,CAST(modified_at_ms AS TEXT),first_seen_at,last_seen_at,sha256_prefix,status FROM legacy_files;")?;
            }
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
                params![1, now()],
            )?;
        }

        if current < 2 {
            let mut stmt = tx.prepare("PRAGMA table_info(indexed_files)")?;
            let cols: Vec<String> = stmt.query_map([], |r| r.get(1))?.collect::<Result<_>>()?;
            if !cols.iter().any(|c| c == "file_id_nullable") {
                tx.execute_batch("ALTER TABLE indexed_files ADD COLUMN file_id_nullable TEXT;")?;
            }
            tx.execute_batch("CREATE INDEX IF NOT EXISTS idx_indexed_files_file_id ON indexed_files(file_id_nullable);")?;
            tx.execute(
                "INSERT OR REPLACE INTO library_metadata(id,schema_version,library_created_at,updated_at) VALUES(1,2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);",
                [],
            )?;
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
                params![2, now()],
            )?;
        }

        tx.commit()
    }

    pub fn add_scan_root(&self, absolute_path: &str, display_name: &str) -> Result<ScanRoot> {
        let conn = self.open()?;
        let normalized = normalize(absolute_path);
        conn.execute("INSERT INTO scan_roots(absolute_path,normalized_path,display_name,enabled,created_at,last_scan_status) VALUES(?1,?2,?3,1,?4,'never_scanned') ON CONFLICT(normalized_path) DO UPDATE SET display_name=excluded.display_name,enabled=1", params![absolute_path, normalized, display_name, now()])?;
        self.root_by_normalized(&conn, &normalized)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Non-destructive: disables the root while preserving its local history and indexed records.
    pub fn remove_scan_root(&self, root_id: i64) -> Result<()> {
        self.open()?.execute(
            "UPDATE scan_roots SET enabled=0 WHERE id=?1",
            params![root_id],
        )?;
        Ok(())
    }

    /// Re-enable a previously disabled root, optionally updating its display name.
    pub fn enable_scan_root(&self, root_id: i64, display_name: &str) -> Result<()> {
        self.open()?.execute(
            "UPDATE scan_roots SET enabled=1, display_name=?1 WHERE id=?2",
            params![display_name, root_id],
        )?;
        Ok(())
    }

    /// Look up a root by its normalised absolute path without knowing its id.
    pub fn get_scan_root_by_path(&self, absolute_path: &str) -> Result<Option<ScanRoot>> {
        let normalized = normalize(absolute_path);
        let conn = self.open()?;
        self.root_by_normalized(&conn, &normalized)
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>> {
        self.roots("", [])
    }

    pub fn get_scan_root(&self, root_id: i64) -> Result<Option<ScanRoot>> {
        Ok(self.roots("WHERE r.id=?1", params![root_id])?.pop())
    }

    pub fn get_files_for_root(&self, root_id: i64) -> Result<Vec<IndexedFile>> {
        let conn = self.open()?;
        files_with(&conn, "WHERE scan_root_id=?1", params![root_id])
    }

    pub fn get_indexed_file(&self, id: i64) -> Result<Option<IndexedFile>> {
        let conn = self.open()?;
        Ok(files_with(&conn, "WHERE id=?1", params![id])?.pop())
    }

    pub fn list_files(
        &self,
        query: Option<String>,
        root_id: Option<i64>,
        status: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<IndexedFile>> {
        let conn = self.open()?;
        let mut sql = String::from("WHERE 1=1");
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(root) = root_id {
            sql.push_str(" AND scan_root_id=?");
            values.push(Box::new(root));
        }
        if let Some(s) = status.filter(|s| !s.is_empty() && s != "all") {
            sql.push_str(" AND status=?");
            values.push(Box::new(s));
        }
        if let Some(q) = query.filter(|q| !q.trim().is_empty()) {
            sql.push_str(" AND (file_name LIKE ? OR absolute_path LIKE ? OR extension LIKE ?)");
            let q = format!("%{}%", q.trim());
            values.extend([
                Box::new(q.clone()) as Box<dyn rusqlite::ToSql>,
                Box::new(q.clone()),
                Box::new(q),
            ]);
        }
        sql.push_str(" ORDER BY modified_at DESC,id DESC LIMIT ? OFFSET ?");
        values.push(Box::new(limit.unwrap_or(200).min(1000) as i64));
        values.push(Box::new(offset.unwrap_or(0) as i64));
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        files_with(&conn, &sql, refs.as_slice())
    }

    pub fn insert_indexed_file(&self, file: &NewIndexedFile) -> Result<IndexedFile> {
        let conn = self.open()?;
        let timestamp = now();
        conn.execute("INSERT INTO indexed_files(scan_root_id,absolute_path,normalized_path,file_name,extension,size_bytes,modified_at,created_at_if_available,discovered_at,last_seen_at,content_hash_nullable,file_id_nullable,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?11,?12)", params![file.scan_root_id,file.absolute_path,file.normalized_path,file.file_name,file.extension,file.size_bytes,file.modified_at,file.created_at_if_available,timestamp,file.content_hash_nullable,file.file_id_nullable,file.status])?;
        Ok(files_with(
            &conn,
            "WHERE normalized_path=?1",
            params![file.normalized_path],
        )?
        .pop()
        .unwrap())
    }

    pub fn update_indexed_file(&self, id: i64, file: &NewIndexedFile) -> Result<()> {
        self.open()?.execute("UPDATE indexed_files SET file_name=?1,extension=?2,size_bytes=?3,modified_at=?4,created_at_if_available=?5,content_hash_nullable=?6,file_id_nullable=?7,status=?8,last_seen_at=?9 WHERE id=?10",params![file.file_name,file.extension,file.size_bytes,file.modified_at,file.created_at_if_available,file.content_hash_nullable,file.file_id_nullable,file.status,now(),id])?;
        Ok(())
    }

    pub fn move_indexed_file(&self, id: i64, file: &NewIndexedFile) -> Result<()> {
        self.open()?.execute("UPDATE indexed_files SET absolute_path=?1,normalized_path=?2,file_name=?3,extension=?4,size_bytes=?5,modified_at=?6,created_at_if_available=?7,content_hash_nullable=?8,file_id_nullable=?9,status=?10,last_seen_at=?11 WHERE id=?12",params![file.absolute_path,file.normalized_path,file.file_name,file.extension,file.size_bytes,file.modified_at,file.created_at_if_available,file.content_hash_nullable,file.file_id_nullable,file.status,now(),id])?;
        Ok(())
    }

    pub fn touch_indexed_file(&self, id: i64, status: &str) -> Result<()> {
        self.open()?.execute(
            "UPDATE indexed_files SET status=?1,last_seen_at=?2 WHERE id=?3",
            params![status, now(), id],
        )?;
        Ok(())
    }

    pub fn update_file_hash(&self, id: i64, hash: &str) -> Result<()> {
        self.open()?.execute(
            "UPDATE indexed_files SET content_hash_nullable=?1,last_seen_at=?2 WHERE id=?3",
            params![hash, now(), id],
        )?;
        Ok(())
    }

    pub fn mark_file_missing(&self, id: i64) -> Result<()> {
        self.touch_indexed_file(id, "missing")
    }

    pub fn start_scan(&self, root_id: i64) -> Result<()> {
        self.open()?.execute(
            "UPDATE scan_roots SET last_scan_started_at=?1,last_scan_status='running' WHERE id=?2",
            params![now(), root_id],
        )?;
        Ok(())
    }

    pub fn record_scan_results(&self, stats: &ScanStats) -> Result<()> {
        self.open()?.execute("UPDATE scan_roots SET last_scan_completed_at=?1,last_scan_status='completed' WHERE id=?2",params![now(),stats.root_id])?;
        Ok(())
    }

    pub fn record_scan_cancelled(&self, root_id: i64) -> Result<()> {
        self.open()?.execute("UPDATE scan_roots SET last_scan_completed_at=?1,last_scan_status='cancelled' WHERE id=?2", params![now(), root_id])?;
        Ok(())
    }

    pub fn record_scan_error(&self, root_id: i64, _err: &str) -> Result<()> {
        self.open()?.execute("UPDATE scan_roots SET last_scan_completed_at=?1,last_scan_status='error' WHERE id=?2", params![now(), root_id])?;
        Ok(())
    }

    pub fn export_library(&self) -> Result<LibraryExport> {
        let roots = self.list_scan_roots()?;
        let files = self.list_files(None, None, None, Some(50000), Some(0))?;
        let exported_at = now();
        Ok(LibraryExport {
            app: "LibOne".into(),
            version: "0.1.0".into(),
            exported_at: exported_at.clone(),
            summary: ExportSummary {
                total_roots: roots.len(),
                total_files: files.len(),
                total_size_bytes: files.iter().map(|f| f.size_bytes).sum(),
                export_timestamp: exported_at,
            },
            scan_roots: roots,
            files,
        })
    }

    fn roots<P: rusqlite::Params>(&self, suffix: &str, params: P) -> Result<Vec<ScanRoot>> {
        let conn = self.open()?;
        let sql = format!(
            "SELECT r.id,r.absolute_path,r.display_name,r.enabled,r.created_at,r.last_scan_started_at,r.last_scan_completed_at,r.last_scan_status,COUNT(f.id) FROM scan_roots r LEFT JOIN indexed_files f ON f.scan_root_id=r.id AND f.status != 'missing' {} GROUP BY r.id ORDER BY r.created_at DESC",
            suffix
        );
        let mut s = conn.prepare(&sql)?;
        let rows = s.query_map(params, |r| {
            Ok(ScanRoot {
                id: r.get(0)?,
                absolute_path: r.get(1)?,
                display_name: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                created_at: r.get(4)?,
                last_scan_started_at: r.get(5)?,
                last_scan_completed_at: r.get(6)?,
                last_scan_status: r.get(7)?,
                file_count: r.get(8)?,
            })
        })?;
        rows.collect()
    }

    fn root_by_normalized(&self, conn: &Connection, path: &str) -> Result<Option<ScanRoot>> {
        let mut s = conn.prepare("SELECT r.id,r.absolute_path,r.display_name,r.enabled,r.created_at,r.last_scan_started_at,r.last_scan_completed_at,r.last_scan_status,COUNT(f.id) FROM scan_roots r LEFT JOIN indexed_files f ON f.scan_root_id=r.id AND f.status != 'missing' WHERE r.normalized_path=?1 GROUP BY r.id")?;
        s.query_row(params![path], |r| {
            Ok(ScanRoot {
                id: r.get(0)?,
                absolute_path: r.get(1)?,
                display_name: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                created_at: r.get(4)?,
                last_scan_started_at: r.get(5)?,
                last_scan_completed_at: r.get(6)?,
                last_scan_status: r.get(7)?,
                file_count: r.get(8)?,
            })
        })
        .optional()
    }
}

fn files_with<P: rusqlite::Params>(
    conn: &Connection,
    suffix: &str,
    params: P,
) -> Result<Vec<IndexedFile>> {
    let sql = format!(
        "SELECT id,scan_root_id,absolute_path,normalized_path,file_name,extension,size_bytes,modified_at,created_at_if_available,discovered_at,last_seen_at,content_hash_nullable,file_id_nullable,status FROM indexed_files {}",
        suffix
    );
    let mut s = conn.prepare(&sql)?;
    let rows = s.query_map(params, |r| {
        Ok(IndexedFile {
            id: r.get(0)?,
            scan_root_id: r.get(1)?,
            absolute_path: r.get(2)?,
            normalized_path: r.get(3)?,
            file_name: r.get(4)?,
            extension: r.get(5)?,
            size_bytes: r.get(6)?,
            modified_at: r.get(7)?,
            created_at_if_available: r.get(8)?,
            discovered_at: r.get(9)?,
            last_seen_at: r.get(10)?,
            content_hash_nullable: r.get(11)?,
            file_id_nullable: r.get(12)?,
            status: r.get(13)?,
        })
    })?;
    rows.collect()
}

fn create_schema(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS scan_roots(id INTEGER PRIMARY KEY,absolute_path TEXT NOT NULL,normalized_path TEXT NOT NULL UNIQUE,display_name TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1)),created_at TEXT NOT NULL,last_scan_started_at TEXT,last_scan_completed_at TEXT,last_scan_status TEXT NOT NULL DEFAULT 'never_scanned'); CREATE TABLE IF NOT EXISTS indexed_files(id INTEGER PRIMARY KEY,scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE RESTRICT,absolute_path TEXT NOT NULL,normalized_path TEXT NOT NULL UNIQUE,file_name TEXT NOT NULL,extension TEXT NOT NULL,size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),modified_at TEXT,created_at_if_available TEXT,discovered_at TEXT NOT NULL,last_seen_at TEXT NOT NULL,content_hash_nullable TEXT,file_id_nullable TEXT,status TEXT NOT NULL); CREATE TABLE IF NOT EXISTS library_metadata(id INTEGER PRIMARY KEY CHECK(id=1),schema_version INTEGER NOT NULL,library_created_at TEXT NOT NULL,updated_at TEXT NOT NULL); CREATE INDEX IF NOT EXISTS idx_indexed_files_root_status ON indexed_files(scan_root_id,status); CREATE INDEX IF NOT EXISTS idx_indexed_files_name ON indexed_files(file_name); CREATE INDEX IF NOT EXISTS idx_indexed_files_hash ON indexed_files(content_hash_nullable); CREATE INDEX IF NOT EXISTS idx_indexed_files_file_id ON indexed_files(file_id_nullable); INSERT OR IGNORE INTO library_metadata(id,schema_version,library_created_at,updated_at) VALUES(1,2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);")
}

fn table_exists(tx: &Transaction<'_>, name: &str) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

fn legacy_schema(tx: &Transaction<'_>) -> Result<bool> {
    if !table_exists(tx, "scan_roots")? {
        return Ok(false);
    };
    let mut s = tx.prepare("PRAGMA table_info(scan_roots)")?;
    let c: Vec<String> = s.query_map([], |r| r.get(1))?.collect::<Result<_>>()?;
    Ok(c.iter().any(|v| v == "path") && !c.iter().any(|v| v == "absolute_path"))
}

fn normalize(value: &str) -> String {
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn io_error(error: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn db() -> (Database, PathBuf) {
        let p = std::env::temp_dir().join(format!(
            "libone-db-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (Database::new(&p).unwrap(), p)
    }

    #[test]
    fn migrates_required_schema() {
        let (d, p) = db();
        let c = d.open().unwrap();
        assert_eq!(
            c.query_row("SELECT schema_version FROM library_metadata", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        drop(c);
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn disabling_root_keeps_history() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\\Docs", "Docs").unwrap();
        d.remove_scan_root(r.id).unwrap();
        assert!(!d.get_scan_root(r.id).unwrap().unwrap().enabled);
        assert_eq!(d.add_scan_root(r"C:\\Docs", "New").unwrap().id, r.id);
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn enable_disable_cycle() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\\Projects", "Projects").unwrap();
        assert!(r.enabled);

        d.remove_scan_root(r.id).unwrap();
        assert!(!d.get_scan_root(r.id).unwrap().unwrap().enabled);

        d.enable_scan_root(r.id, "Projects (re-enabled)").unwrap();
        let re = d.get_scan_root(r.id).unwrap().unwrap();
        assert!(re.enabled);
        assert_eq!(re.display_name, "Projects (re-enabled)");
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn get_scan_root_by_path_normalises() {
        let (d, p) = db();
        d.add_scan_root(r"C:\\Docs", "Docs").unwrap();
        let found = d.get_scan_root_by_path(r"C:\\Docs\\").unwrap();
        assert!(found.is_some());
        let not_found = d.get_scan_root_by_path(r"C:\\Other").unwrap();
        assert!(not_found.is_none());
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn duplicate_path_add_returns_same_id() {
        let (d, p) = db();
        let first = d.add_scan_root(r"C:\\Reports", "Reports").unwrap();
        let second = d.add_scan_root(r"C:\\Reports", "Reports v2").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(second.display_name, "Reports v2");
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn missing_files_remain() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\\Docs", "Docs").unwrap();
        let f = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\\Docs\\a.pdf".into(),
                normalized_path: normalize(r"C:\\Docs\\a.pdf"),
                file_name: "a.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 1,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();
        d.mark_file_missing(f.id).unwrap();
        assert_eq!(d.get_files_for_root(r.id).unwrap()[0].status, "missing");
        std::fs::remove_dir_all(p).unwrap()
    }
}
