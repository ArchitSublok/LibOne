use std::path::{Path, PathBuf};
use chrono::Utc;
use rusqlite::{params, Connection, Result};

use crate::models::{ExportSummary, IndexedFile, LibraryExport, ScanRoot, ScanStats};

pub struct Database {
    db_path: PathBuf,
}

impl Database {
    pub fn new<P: AsRef<Path>>(app_data_dir: P) -> Result<Self> {
        let dir = app_data_dir.as_ref();
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
        }

        let db_path = dir.join("libone.db");
        let db = Database { db_path };
        db.init_schema()?;
        Ok(db)
    }

    pub fn get_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(conn)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.open()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scan_roots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT UNIQUE NOT NULL,
                label TEXT NOT NULL,
                added_at TEXT NOT NULL,
                last_scanned_at TEXT,
                status TEXT NOT NULL DEFAULT 'ready'
            );

            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
                path TEXT UNIQUE NOT NULL,
                filename TEXT NOT NULL,
                extension TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_at_ms INTEGER NOT NULL,
                sha256_prefix TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                first_seen_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_files_root ON files(scan_root_id);
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);
            CREATE INDEX IF NOT EXISTS idx_files_filename ON files(filename);
            CREATE INDEX IF NOT EXISTS idx_files_size_hash ON files(size_bytes, sha256_prefix);

            CREATE TABLE IF NOT EXISTS scan_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE CASCADE,
                scanned_at TEXT NOT NULL,
                total_scanned INTEGER NOT NULL,
                added_count INTEGER NOT NULL,
                modified_count INTEGER NOT NULL,
                missing_count INTEGER NOT NULL,
                moved_count INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn add_scan_root(&self, path: &str, label: &str) -> Result<ScanRoot> {
        let conn = self.open()?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO scan_roots (path, label, added_at, status)
             VALUES (?1, ?2, ?3, 'ready')
             ON CONFLICT(path) DO UPDATE SET label = ?2",
            params![path, label, now],
        )?;

        let id: i64 = conn.query_row(
            "SELECT id FROM scan_roots WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;

        Ok(ScanRoot {
            id,
            path: path.to_string(),
            label: label.to_string(),
            added_at: now,
            last_scanned_at: None,
            file_count: 0,
            status: "ready".to_string(),
        })
    }

    pub fn remove_scan_root(&self, root_id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM scan_roots WHERE id = ?1", params![root_id])?;
        Ok(())
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.path, r.label, r.added_at, r.last_scanned_at, r.status,
                    COUNT(f.id) as file_count
             FROM scan_roots r
             LEFT JOIN files f ON r.id = f.scan_root_id AND f.status != 'missing'
             GROUP BY r.id
             ORDER BY r.added_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ScanRoot {
                id: row.get(0)?,
                path: row.get(1)?,
                label: row.get(2)?,
                added_at: row.get(3)?,
                last_scanned_at: row.get(4)?,
                status: row.get(5)?,
                file_count: row.get(6)?,
            })
        })?;

        let mut roots = Vec::new();
        for r in rows {
            roots.push(r?);
        }
        Ok(roots)
    }

    pub fn get_scan_root(&self, root_id: i64) -> Result<Option<ScanRoot>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT r.id, r.path, r.label, r.added_at, r.last_scanned_at, r.status,
                    COUNT(f.id) as file_count
             FROM scan_roots r
             LEFT JOIN files f ON r.id = f.scan_root_id AND f.status != 'missing'
             WHERE r.id = ?1
             GROUP BY r.id",
        )?;

        let mut rows = stmt.query_map(params![root_id], |row| {
            Ok(ScanRoot {
                id: row.get(0)?,
                path: row.get(1)?,
                label: row.get(2)?,
                added_at: row.get(3)?,
                last_scanned_at: row.get(4)?,
                status: row.get(5)?,
                file_count: row.get(6)?,
            })
        })?;

        if let Some(root) = rows.next() {
            Ok(Some(root?))
        } else {
            Ok(None)
        }
    }

    pub fn get_files_for_root(&self, root_id: i64) -> Result<Vec<IndexedFile>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, scan_root_id, path, filename, extension, size_bytes,
                    modified_at_ms, sha256_prefix, status, first_seen_at, last_seen_at
             FROM files
             WHERE scan_root_id = ?1",
        )?;

        let rows = stmt.query_map(params![root_id], |row| {
            Ok(IndexedFile {
                id: row.get(0)?,
                scan_root_id: row.get(1)?,
                path: row.get(2)?,
                filename: row.get(3)?,
                extension: row.get(4)?,
                size_bytes: row.get(5)?,
                modified_at_ms: row.get(6)?,
                sha256_prefix: row.get(7)?,
                status: row.get(8)?,
                first_seen_at: row.get(9)?,
                last_seen_at: row.get(10)?,
            })
        })?;

        let mut files = Vec::new();
        for f in rows {
            files.push(f?);
        }
        Ok(files)
    }

    pub fn list_files(
        &self,
        query: Option<String>,
        root_id: Option<i64>,
        status_filter: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<IndexedFile>> {
        let conn = self.open()?;
        let mut sql = String::from(
            "SELECT id, scan_root_id, path, filename, extension, size_bytes,
                    modified_at_ms, sha256_prefix, status, first_seen_at, last_seen_at
             FROM files WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(r_id) = root_id {
            sql.push_str(" AND scan_root_id = ?");
            params_vec.push(Box::new(r_id));
        }

        if let Some(s) = status_filter {
            if !s.is_empty() && s != "all" {
                sql.push_str(" AND status = ?");
                params_vec.push(Box::new(s));
            }
        }

        if let Some(q) = query {
            let trimmed = q.trim();
            if !trimmed.is_empty() {
                sql.push_str(" AND (filename LIKE ? OR path LIKE ? OR extension LIKE ?)");
                let pattern = format!("%{}%", trimmed);
                params_vec.push(Box::new(pattern.clone()));
                params_vec.push(Box::new(pattern.clone()));
                params_vec.push(Box::new(pattern));
            }
        }

        sql.push_str(" ORDER BY modified_at_ms DESC");

        let lim = limit.unwrap_or(200);
        let off = offset.unwrap_or(0);
        sql.push_str(" LIMIT ? OFFSET ?");
        params_vec.push(Box::new(lim as i64));
        params_vec.push(Box::new(off as i64));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(&param_refs[..], |row| {
            Ok(IndexedFile {
                id: row.get(0)?,
                scan_root_id: row.get(1)?,
                path: row.get(2)?,
                filename: row.get(3)?,
                extension: row.get(4)?,
                size_bytes: row.get(5)?,
                modified_at_ms: row.get(6)?,
                sha256_prefix: row.get(7)?,
                status: row.get(8)?,
                first_seen_at: row.get(9)?,
                last_seen_at: row.get(10)?,
            })
        })?;

        let mut files = Vec::new();
        for f in rows {
            files.push(f?);
        }
        Ok(files)
    }

    pub fn record_scan_results(&self, stats: &ScanStats) -> Result<()> {
        let conn = self.open()?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE scan_roots SET last_scanned_at = ?1, status = 'ready' WHERE id = ?2",
            params![now, stats.root_id],
        )?;

        conn.execute(
            "INSERT INTO scan_history (
                scan_root_id, scanned_at, total_scanned, added_count,
                modified_count, missing_count, moved_count, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                stats.root_id,
                now,
                stats.total_scanned as i64,
                stats.added as i64,
                stats.modified as i64,
                stats.missing as i64,
                stats.moved as i64,
                stats.duration_ms as i64
            ],
        )?;

        Ok(())
    }

    pub fn export_library(&self) -> Result<LibraryExport> {
        let roots = self.list_scan_roots()?;
        let files = self.list_files(None, None, None, Some(50000), Some(0))?;

        let total_size: i64 = files.iter().map(|f| f.size_bytes).sum();
        let now = Utc::now().to_rfc3339();

        let summary = ExportSummary {
            total_roots: roots.len(),
            total_files: files.len(),
            total_size_bytes: total_size,
            export_timestamp: now.clone(),
        };

        Ok(LibraryExport {
            app: "LibOne".to_string(),
            version: "0.1.0".to_string(),
            exported_at: now,
            summary,
            scan_roots: roots,
            files,
        })
    }
}
