//! The local SQLite repository. SQL is deliberately contained here, away from Tauri commands.

use crate::models::{
    BackupResult, BackupSummary, BackupValidationResult, BulkSuggestionResult, Collection,
    DatabaseIntegrityReport, DuplicateGroup, DuplicateGroupDetail, DuplicateGroupFileLink,
    ExportSummary, FileCollectionLink, FileComparisonItem, FileCountsHealthInfo, FileDetail,
    FileNote, FileQueryParams, FileTagLink, ForeignKeyViolation, IndexedFile, IntegrityIssue,
    LibraryExport, NewIndexedFile, OrganizationSuggestion, PaginatedFiles, RestoreResult,
    ScanHealthInfo, ScanRoot, ScanStats, SuggestionFileLink, Tag, EXPORT_FORMAT_VERSION,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result, Transaction};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: i64 = 4;

/// Upper bound for tag IDs accepted by a single file query. Keeps the generated
/// SQL below SQLite's variable limit even for an untrusted request.
pub const MAX_TAG_FILTERS: usize = 64;

/// Upper bound for the free-text search term.
pub const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone)]
pub struct Database {
    db_path: PathBuf,
}

impl Database {
    /// Creates app data if necessary and migrates the local metadata database transactionally.
    pub fn new<P: AsRef<Path>>(app_data_dir: P) -> Result<Self> {
        std::fs::create_dir_all(app_data_dir.as_ref()).map_err(io_error)?;
        Self::recover_interrupted_restore(app_data_dir.as_ref())?;
        let db = Self {
            db_path: app_data_dir.as_ref().join("libone.db"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn get_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    pub(crate) fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY;",
        )?;
        Ok(conn)
    }

    /// Retries a database operation if SQLite returns SQLITE_BUSY or SQLITE_LOCKED.
    pub fn with_busy_retry<F, T>(mut op: F) -> Result<T>
    where
        F: FnMut() -> Result<T>,
    {
        let mut attempts = 0;
        loop {
            match op() {
                Ok(val) => return Ok(val),
                Err(e) => {
                    let is_busy = matches!(
                        e,
                        rusqlite::Error::SqliteFailure(
                            rusqlite::ffi::Error {
                                code: rusqlite::ErrorCode::DatabaseBusy
                                    | rusqlite::ErrorCode::DatabaseLocked,
                                ..
                            },
                            _
                        )
                    );
                    if is_busy && attempts < 5 {
                        attempts += 1;
                        std::thread::sleep(std::time::Duration::from_millis(40 * attempts));
                        continue;
                    }
                    return Err(e);
                }
            }
        }
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

        if current < 3 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS collections (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                    description_nullable TEXT,
                    color_nullable TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                    color_nullable TEXT,
                    created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS file_collections (
                    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (file_id, collection_id)
                );
                CREATE TABLE IF NOT EXISTS file_tags (
                    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (file_id, tag_id)
                );
                CREATE TABLE IF NOT EXISTS file_notes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                    note_text TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_collections_name ON collections(name);
                CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
                CREATE INDEX IF NOT EXISTS idx_file_collections_file_id ON file_collections(file_id);
                CREATE INDEX IF NOT EXISTS idx_file_collections_collection_id ON file_collections(collection_id);
                CREATE INDEX IF NOT EXISTS idx_file_tags_file_id ON file_tags(file_id);
                CREATE INDEX IF NOT EXISTS idx_file_tags_tag_id ON file_tags(tag_id);
                CREATE INDEX IF NOT EXISTS idx_file_notes_file_id ON file_notes(file_id);",
            )?;
            tx.execute(
                "INSERT INTO library_metadata(id,schema_version,library_created_at,updated_at)
                 VALUES(1, 3, ?1, ?1)
                 ON CONFLICT(id) DO UPDATE SET schema_version=3, updated_at=?1;",
                params![now()],
            )?;
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
                params![3, now()],
            )?;
        }

        if current < 4 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS organization_suggestions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    suggestion_type TEXT NOT NULL,
                    title TEXT NOT NULL,
                    rationale TEXT NOT NULL,
                    confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
                    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'approved', 'rejected', 'dismissed')),
                    created_at TEXT NOT NULL,
                    reviewed_at TEXT
                );
                CREATE TABLE IF NOT EXISTS suggestion_files (
                    suggestion_id INTEGER NOT NULL REFERENCES organization_suggestions(id) ON DELETE CASCADE,
                    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                    PRIMARY KEY (suggestion_id, file_id)
                );
                CREATE TABLE IF NOT EXISTS suggestion_tags (
                    suggestion_id INTEGER NOT NULL REFERENCES organization_suggestions(id) ON DELETE CASCADE,
                    tag_id_nullable INTEGER REFERENCES tags(id) ON DELETE CASCADE,
                    suggested_name_nullable TEXT,
                    CHECK (tag_id_nullable IS NOT NULL OR suggested_name_nullable IS NOT NULL)
                );
                CREATE TABLE IF NOT EXISTS suggestion_collections (
                    suggestion_id INTEGER NOT NULL REFERENCES organization_suggestions(id) ON DELETE CASCADE,
                    collection_id_nullable INTEGER REFERENCES collections(id) ON DELETE CASCADE,
                    suggested_name_nullable TEXT,
                    CHECK (collection_id_nullable IS NOT NULL OR suggested_name_nullable IS NOT NULL)
                );
                CREATE TABLE IF NOT EXISTS duplicate_groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    match_type TEXT NOT NULL,
                    confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
                    created_at TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'approved', 'rejected', 'dismissed'))
                );
                CREATE TABLE IF NOT EXISTS duplicate_group_files (
                    group_id INTEGER NOT NULL REFERENCES duplicate_groups(id) ON DELETE CASCADE,
                    file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                    PRIMARY KEY (group_id, file_id)
                );
                CREATE INDEX IF NOT EXISTS idx_org_suggestions_status ON organization_suggestions(status);
                CREATE INDEX IF NOT EXISTS idx_org_suggestions_type ON organization_suggestions(suggestion_type);
                CREATE INDEX IF NOT EXISTS idx_suggestion_files_file ON suggestion_files(file_id);
                CREATE INDEX IF NOT EXISTS idx_suggestion_tags_suggestion ON suggestion_tags(suggestion_id);
                CREATE INDEX IF NOT EXISTS idx_suggestion_tags_tag ON suggestion_tags(tag_id_nullable);
                CREATE INDEX IF NOT EXISTS idx_suggestion_collections_suggestion ON suggestion_collections(suggestion_id);
                CREATE INDEX IF NOT EXISTS idx_suggestion_collections_col ON suggestion_collections(collection_id_nullable);
                CREATE INDEX IF NOT EXISTS idx_duplicate_groups_status ON duplicate_groups(status);
                CREATE INDEX IF NOT EXISTS idx_duplicate_group_files_file ON duplicate_group_files(file_id);",
            )?;
            tx.execute(
                "INSERT INTO library_metadata(id,schema_version,library_created_at,updated_at)
                 VALUES(1, 4, ?1, ?1)
                 ON CONFLICT(id) DO UPDATE SET schema_version=4, updated_at=?1;",
                params![now()],
            )?;
            tx.execute(
                "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,?2)",
                params![4, now()],
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

    pub fn get_file_by_normalized_path(
        &self,
        normalized_path: &str,
    ) -> Result<Option<IndexedFile>> {
        let conn = self.open()?;
        Ok(files_with(&conn, "WHERE normalized_path=?1", params![normalized_path])?.pop())
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

    pub fn query_files(&self, params: &FileQueryParams) -> Result<PaginatedFiles> {
        let conn = self.open()?;
        let mut where_clauses = vec!["1=1".to_string()];
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(root) = params.root_id {
            where_clauses.push("scan_root_id=?".into());
            values.push(Box::new(root));
        }

        if let Some(ref ext) = params
            .extension
            .as_ref()
            .filter(|e| !e.trim().is_empty() && *e != "all")
        {
            where_clauses.push("extension=?".into());
            values.push(Box::new(ext.trim().to_lowercase()));
        }

        if let Some(ref st) = params
            .status
            .as_ref()
            .filter(|s| !s.trim().is_empty() && *s != "all")
        {
            where_clauses.push("status=?".into());
            values.push(Box::new(st.trim().to_lowercase()));
        }

        if let Some(ref q) = params.query.as_ref().filter(|q| !q.trim().is_empty()) {
            let bounded_query: String = q.trim().chars().take(MAX_QUERY_CHARS).collect();
            if !bounded_query.is_empty() {
                where_clauses
                    .push("(file_name LIKE ? OR absolute_path LIKE ? OR extension LIKE ?)".into());
                let pattern = format!("%{bounded_query}%");
                values.push(Box::new(pattern.clone()));
                values.push(Box::new(pattern.clone()));
                values.push(Box::new(pattern));
            }
        }

        if let Some(col_id) = params.collection_id {
            where_clauses.push(
                "id IN (SELECT file_id FROM file_collections WHERE collection_id = ?)".into(),
            );
            values.push(Box::new(col_id));
        }

        if let Some(ref tag_ids) = params.tag_ids {
            let mut unique_tags = tag_ids.clone();
            unique_tags.sort_unstable();
            unique_tags.dedup();
            unique_tags.truncate(MAX_TAG_FILTERS);
            if !unique_tags.is_empty() {
                let placeholders = vec!["?"; unique_tags.len()].join(",");
                where_clauses.push(format!(
                    "id IN (SELECT file_id FROM file_tags WHERE tag_id IN ({placeholders}) GROUP BY file_id HAVING COUNT(DISTINCT tag_id) = ?)"
                ));
                for t in &unique_tags {
                    values.push(Box::new(*t));
                }
                values.push(Box::new(unique_tags.len() as i64));
            }
        }

        if let Some(has_notes) = params.has_notes {
            if has_notes {
                where_clauses.push(
                    "EXISTS (SELECT 1 FROM file_notes WHERE file_notes.file_id = indexed_files.id)"
                        .into(),
                );
            } else {
                where_clauses.push(
                    "NOT EXISTS (SELECT 1 FROM file_notes WHERE file_notes.file_id = indexed_files.id)"
                        .into(),
                );
            }
        }

        let where_sql = where_clauses.join(" AND ");

        // 1. Total matching records
        let count_sql = format!("SELECT COUNT(*) FROM indexed_files WHERE {where_sql}");
        let count_refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let total: usize = conn.query_row(&count_sql, count_refs.as_slice(), |r| r.get(0))?;

        // 2. Sorting
        let order_col = match params.sort_by.as_deref().unwrap_or("name") {
            "name" => "file_name",
            "path" => "normalized_path",
            "extension" => "extension",
            "size" => "size_bytes",
            "modified" => "modified_at",
            "status" => "status",
            _ => "file_name",
        };

        let order_dir = match params
            .sort_order
            .as_deref()
            .unwrap_or("asc")
            .to_lowercase()
            .as_str()
        {
            "desc" => "DESC",
            _ => "ASC",
        };

        let page = params.page.unwrap_or(1).max(1);
        let page_size = params.page_size.unwrap_or(25).clamp(1, 200);
        let offset = (page - 1) * page_size;
        let total_pages = if total == 0 {
            1
        } else {
            (total + page_size - 1) / page_size
        };

        let query_sql =
            format!("WHERE {where_sql} ORDER BY {order_col} {order_dir}, id ASC LIMIT ? OFFSET ?");
        values.push(Box::new(page_size as i64));
        values.push(Box::new(offset as i64));

        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let items = files_with(&conn, &query_sql, refs.as_slice())?;

        Ok(PaginatedFiles {
            files: items,
            total,
            page,
            page_size,
            total_pages,
        })
    }

    pub fn get_file_detail(&self, file_id: i64) -> Result<Option<FileDetail>> {
        let file = match self.get_indexed_file(file_id)? {
            Some(f) => f,
            None => return Ok(None),
        };

        let root = self.get_scan_root(file.scan_root_id)?;
        let (root_name, root_path) = match root {
            Some(r) => (r.display_name, r.absolute_path),
            None => ("Unknown Root".to_string(), String::new()),
        };

        let exists = Path::new(&file.absolute_path).exists();

        Ok(Some(FileDetail {
            file,
            root_display_name: root_name,
            root_absolute_path: root_path,
            exists_on_disk: exists,
        }))
    }

    pub fn get_distinct_extensions(&self) -> Result<Vec<String>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT extension FROM indexed_files WHERE extension IS NOT NULL AND extension != '' ORDER BY extension ASC",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        let mut exts = Vec::new();
        for r in rows {
            exts.push(r?);
        }
        Ok(exts)
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
        self.open()?.execute(
            "UPDATE scan_roots SET last_scan_completed_at=?1,last_scan_status='error' WHERE id=?2",
            params![now(), root_id],
        )?;
        Ok(())
    }

    /// Snapshot of all library metadata. This never reads document contents.
    pub fn export_library(&self) -> Result<LibraryExport> {
        let roots = self.list_scan_roots()?;
        let files = self.list_files(None, None, None, None, Some(0))?;
        let collections = self.list_collections()?;
        let tags = self.list_tags()?;
        let file_collections = self.list_file_collection_links(None, None)?;
        let file_tags = self.list_file_tag_links(None, None)?;
        let file_notes = self.list_all_file_notes()?;
        let organization_suggestions = self.list_organization_suggestions(None, None)?;
        let suggestion_files = self.list_all_suggestion_files()?;
        let duplicate_groups = self.list_duplicate_groups(None)?;
        let duplicate_group_files = self.list_all_duplicate_group_files()?;
        let exported_at = now();

        Ok(LibraryExport {
            format_version: EXPORT_FORMAT_VERSION.into(),
            app: "LibOne".into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: exported_at.clone(),
            summary: ExportSummary {
                total_roots: roots.len(),
                total_files: files.len(),
                total_size_bytes: files.iter().map(|f| f.size_bytes).sum(),
                export_timestamp: exported_at,
            },
            scan_roots: roots,
            files,
            collections,
            tags,
            file_collections,
            file_tags,
            file_notes,
            organization_suggestions,
            suggestion_files,
            duplicate_groups,
            duplicate_group_files,
        })
    }

    // ── Phase 4: Local Backup, Restore & Integrity Checks ────────────────────

    /// Recover safely from an interrupted restore if staging or crash files exist.
    pub fn recover_interrupted_restore(app_data_dir: &Path) -> Result<()> {
        let db_path = app_data_dir.join("libone.db");
        // 1. Clean up any stale staging files
        if let Ok(entries) = std::fs::read_dir(app_data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with(".libone_restore_staging_") && name.ends_with(".tmp") {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }

        // 2. If libone.db is missing or 0-bytes, recover from latest safety backup
        let needs_recovery = !db_path.exists()
            || std::fs::metadata(&db_path)
                .map(|m| m.len() == 0)
                .unwrap_or(true);

        if needs_recovery {
            let mut latest_backup: Option<(PathBuf, std::time::SystemTime)> = None;
            if let Ok(entries) = std::fs::read_dir(app_data_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("libone_safety_pre_restore_") && name.ends_with(".db") {
                            if let Ok(meta) = path.metadata() {
                                if meta.len() > 0 {
                                    let time = meta
                                        .modified()
                                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                                    if latest_backup
                                        .as_ref()
                                        .map(|(_, t)| time > *t)
                                        .unwrap_or(true)
                                    {
                                        latest_backup = Some((path, time));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some((backup_file, _)) = latest_backup {
                let _ = std::fs::copy(&backup_file, &db_path);
            }
        }
        Ok(())
    }

    /// Creates an atomic local backup of the SQLite database and metadata.
    /// Writes through a sibling temporary file and verifies integrity before replacing target.
    /// Never includes document contents or uploads data.
    pub fn create_backup<P: AsRef<Path>>(&self, target_path: P) -> Result<BackupResult> {
        let target = target_path.as_ref();
        let parent = target.parent().ok_or_else(|| {
            rusqlite::Error::InvalidPath(PathBuf::from(
                "Target backup path has no parent directory",
            ))
        })?;
        std::fs::create_dir_all(parent).map_err(io_error)?;

        // Unique sibling temp file name
        let tmp_name = format!(
            ".libone_backup_tmp_{}_{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let tmp_path = parent.join(&tmp_name);
        if tmp_path.exists() {
            let _ = std::fs::remove_file(&tmp_path);
        }

        // Flush and checkpoint WAL
        let conn = self.open()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

        let tmp_str = tmp_path.to_str().ok_or_else(|| {
            rusqlite::Error::InvalidPath(PathBuf::from("Backup path contains non-UTF8 characters"))
        })?;

        // VACUUM INTO writes directly to the temporary path
        let vacuum_res = conn.execute("VACUUM INTO ?1", params![tmp_str]);
        if let Err(e) = vacuum_res {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }

        // Validate the temp file integrity
        let val = match Self::validate_backup_file(&tmp_path) {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(e);
            }
        };

        // Atomically rename temporary file to target path
        if target.exists() {
            let _ = std::fs::remove_file(target);
        }
        if let Err(_) = std::fs::rename(&tmp_path, target) {
            // Fallback for cross-filesystem move
            if let Err(copy_err) = std::fs::copy(&tmp_path, target) {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(io_error(copy_err));
            }
            let _ = std::fs::remove_file(&tmp_path);
        }

        Ok(BackupResult {
            backup_path: target.to_string_lossy().to_string(),
            file_size_bytes: val.file_size_bytes,
            sha256_hash: val.sha256_hash,
            created_at: now(),
            summary: val.summary,
        })
    }

    /// Validates a backup file's format, schema version, integrity, and metadata constraints
    /// without modifying anything.
    pub fn validate_backup_file<P: AsRef<Path>>(backup_path: P) -> Result<BackupValidationResult> {
        let path = backup_path.as_ref();
        if !path.exists() {
            return Err(rusqlite::Error::InvalidPath(PathBuf::from(format!(
                "Backup file does not exist: {}",
                path.display()
            ))));
        }
        let metadata = std::fs::metadata(path).map_err(io_error)?;
        if metadata.len() == 0 {
            return Err(io_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Backup file is empty (0 bytes)",
            )));
        }

        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )?;

        // 1. SQLite integrity check
        let integrity: String = conn.query_row("PRAGMA integrity_check;", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(io_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Corrupted backup file: database failed SQLite integrity check: {integrity}"
                ),
            )));
        }

        // 2. Foreign key check
        let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check;")?;
        let mut fk_rows = fk_stmt.query([])?;
        let mut fk_count = 0;
        while let Some(_) = fk_rows.next()? {
            fk_count += 1;
        }

        // 3. Required tables check
        let required_tables = ["library_metadata", "scan_roots", "indexed_files"];
        for table in &required_tables {
            if !conn_table_exists(&conn, table)? {
                return Err(io_error(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid backup: missing required metadata table '{table}'"),
                )));
            }
        }

        // 4. Schema version check
        let schema_version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(schema_version), 0) FROM library_metadata",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);

        if schema_version > SCHEMA_VERSION {
            return Err(io_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Incompatible backup version: backup schema version {schema_version} is newer than current LibOne version ({SCHEMA_VERSION})"
                ),
            )));
        }
        if schema_version < 1 {
            return Err(io_error(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid backup: schema version must be at least 1",
            )));
        }

        // 5. Metadata-only verification: ensure no document content columns exist in indexed_files
        let mut col_stmt = conn.prepare("PRAGMA table_info(indexed_files);")?;
        let cols: Vec<String> = col_stmt
            .query_map([], |r| r.get(1))?
            .collect::<Result<Vec<String>>>()?;
        for forbidden in &["content", "raw_data", "file_body", "binary_content"] {
            if cols.iter().any(|c| c.eq_ignore_ascii_case(forbidden)) {
                return Err(io_error(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Backup violation: indexed_files contains prohibited content column '{forbidden}'"),
                )));
            }
        }

        // 6. Summary counts & SHA-256
        let summary = Self::read_summary_from_conn(&conn)?;

        let file_bytes = std::fs::read(path).map_err(io_error)?;
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&file_bytes);
        let hash_str = format!("{:x}", hasher.finalize());

        let mut warnings = Vec::new();
        if fk_count > 0 {
            warnings.push(format!(
                "{fk_count} foreign key inconsistency warning(s) detected in backup."
            ));
        }

        Ok(BackupValidationResult {
            is_valid: true,
            schema_version,
            summary,
            file_size_bytes: metadata.len(),
            sha256_hash: hash_str,
            warnings,
        })
    }

    /// Read metadata summary counts from a connection
    fn read_summary_from_conn(conn: &Connection) -> Result<BackupSummary> {
        let schema_version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(schema_version), 0) FROM library_metadata",
                [],
                |r| r.get(0),
            )
            .unwrap_or(1);

        let total_roots: usize = conn
            .query_row("SELECT count(*) FROM scan_roots", [], |r| r.get(0))
            .unwrap_or(0);
        let total_files: usize = conn
            .query_row("SELECT count(*) FROM indexed_files", [], |r| r.get(0))
            .unwrap_or(0);
        let total_collections: usize = if conn_table_exists(conn, "collections")? {
            conn.query_row("SELECT count(*) FROM collections", [], |r| r.get(0))
                .unwrap_or(0)
        } else {
            0
        };
        let total_tags: usize = if conn_table_exists(conn, "tags")? {
            conn.query_row("SELECT count(*) FROM tags", [], |r| r.get(0))
                .unwrap_or(0)
        } else {
            0
        };
        let total_notes: usize = if conn_table_exists(conn, "file_notes")? {
            conn.query_row("SELECT count(*) FROM file_notes", [], |r| r.get(0))
                .unwrap_or(0)
        } else {
            0
        };
        let total_suggestions: usize = if conn_table_exists(conn, "organization_suggestions")? {
            conn.query_row("SELECT count(*) FROM organization_suggestions", [], |r| {
                r.get(0)
            })
            .unwrap_or(0)
        } else {
            0
        };
        let total_duplicate_groups: usize = if conn_table_exists(conn, "duplicate_groups")? {
            conn.query_row("SELECT count(*) FROM duplicate_groups", [], |r| r.get(0))
                .unwrap_or(0)
        } else {
            0
        };

        Ok(BackupSummary {
            schema_version,
            total_roots,
            total_files,
            total_collections,
            total_tags,
            total_notes,
            total_suggestions,
            total_duplicate_groups,
        })
    }

    /// Restores the metadata database from a verified backup.
    /// Creates a safety backup of the current database before replacing anything.
    /// Rolls back cleanly to the safety backup on any failure during replacement.
    /// Never modifies user document files on disk.
    pub fn restore_from_backup<P: AsRef<Path>>(&self, backup_path: P) -> Result<RestoreResult> {
        let path = backup_path.as_ref();
        // 1. Validate backup before replacing anything
        Self::validate_backup_file(path)?;

        let app_data_dir = self.db_path.parent().ok_or_else(|| {
            rusqlite::Error::InvalidPath(PathBuf::from("Database path has no parent directory"))
        })?;

        // 2. Flush current DB and create safety backup
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let safety_path = app_data_dir.join(format!("libone_safety_pre_restore_{timestamp}.db"));
        self.create_backup(&safety_path)?;

        // 3. Stage the restored database in a temporary file
        let staging_path = app_data_dir.join(format!(".libone_restore_staging_{timestamp}.tmp"));
        if staging_path.exists() {
            let _ = std::fs::remove_file(&staging_path);
        }
        std::fs::copy(path, &staging_path).map_err(io_error)?;

        // Verify staging file
        if let Err(e) = Self::validate_backup_file(&staging_path) {
            let _ = std::fs::remove_file(&staging_path);
            return Err(e);
        }

        // 4. Close/checkpoint active WAL and replace database
        {
            let conn = self.open()?;
            let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        let wal_path = self.db_path.with_extension("db-wal");
        let shm_path = self.db_path.with_extension("db-shm");
        let _ = std::fs::remove_file(&wal_path);
        let _ = std::fs::remove_file(&shm_path);

        // Atomically replace self.db_path with staging_path
        if let Err(_) = std::fs::rename(&staging_path, &self.db_path) {
            if let Err(copy_err) = std::fs::copy(&staging_path, &self.db_path) {
                // Rollback to safety backup immediately
                let _ = std::fs::copy(&safety_path, &self.db_path);
                let _ = std::fs::remove_file(&staging_path);
                return Err(io_error(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Restore failed and was rolled back to safety backup: {copy_err}"),
                )));
            }
            let _ = std::fs::remove_file(&staging_path);
        }

        // 5. Migrate restored database if needed (e.g. from v1, v2, v3 to v4)
        self.migrate()?;

        // 6. Read summary from restored database
        let restored_conn = self.open()?;
        let summary = Self::read_summary_from_conn(&restored_conn)?;

        Ok(RestoreResult {
            success: true,
            restored_schema_version: summary.schema_version,
            safety_backup_path: safety_path.to_string_lossy().to_string(),
            summary,
        })
    }

    /// Performs complete database integrity diagnostics:
    /// - SQLite B-Tree & page consistency check
    /// - Foreign key referential integrity
    /// - Metadata schema & required tables verification
    /// - Path validity and missing-file diagnostics
    /// Provides actionable repair guidance without deleting records.
    pub fn check_integrity(&self) -> Result<DatabaseIntegrityReport> {
        let conn = self.open()?;
        let mut issues: Vec<IntegrityIssue> = Vec::new();

        // 1. SQLite PRAGMA integrity_check
        let integrity_str: String = conn.query_row("PRAGMA integrity_check;", [], |r| r.get(0))?;
        let integrity_check_passed = integrity_str == "ok";
        if !integrity_check_passed {
            issues.push(IntegrityIssue {
                severity: "error".into(),
                category: "sqlite".into(),
                message: format!("SQLite page integrity check failed: {integrity_str}"),
                repair_guidance: "Database file corruption detected. Restore from a recent valid local backup or export your metadata.".into(),
            });
        }

        // 2. SQLite PRAGMA foreign_key_check
        let mut fk_violations = Vec::new();
        let mut fk_stmt = conn.prepare("PRAGMA foreign_key_check;")?;
        let mut fk_rows = fk_stmt.query([])?;
        while let Some(row) = fk_rows.next()? {
            fk_violations.push(ForeignKeyViolation {
                table_name: row.get(0)?,
                rowid: row.get(1)?,
                referenced_table: row.get(2)?,
                foreign_key_index: row.get(3)?,
            });
        }
        let foreign_key_check_passed = fk_violations.is_empty();
        if !foreign_key_check_passed {
            issues.push(IntegrityIssue {
                severity: "warning".into(),
                category: "foreign_key".into(),
                message: format!("{} broken foreign key association(s) detected in metadata.", fk_violations.len()),
                repair_guidance: "Some tag, collection, or suggestion links reference deleted items. Re-scan library folders or update tags in LibOne to reconcile associations without data loss.".into(),
            });
        }

        // 3. Schema & table presence check
        let required_tables = [
            "library_metadata",
            "schema_migrations",
            "scan_roots",
            "indexed_files",
            "collections",
            "tags",
            "file_collections",
            "file_tags",
            "file_notes",
            "organization_suggestions",
            "suggestion_files",
            "suggestion_tags",
            "suggestion_collections",
            "duplicate_groups",
            "duplicate_group_files",
        ];
        let mut missing_tables = Vec::new();
        for table in &required_tables {
            if !conn_table_exists(&conn, table)? {
                missing_tables.push(table.to_string());
            }
        }
        if !missing_tables.is_empty() {
            issues.push(IntegrityIssue {
                severity: "error".into(),
                category: "schema".into(),
                message: format!("Missing required metadata table(s): {}", missing_tables.join(", ")),
                repair_guidance: "Schema is incomplete. Restore from a valid local backup to reconstruct missing tables without losing records.".into(),
            });
        }

        // 4. Invalid paths check
        let invalid_root_paths: usize = conn
            .query_row(
                "SELECT count(*) FROM scan_roots WHERE length(trim(absolute_path)) = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let invalid_file_paths: usize = conn
            .query_row(
                "SELECT count(*) FROM indexed_files WHERE length(trim(absolute_path)) = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let invalid_paths_count = invalid_root_paths + invalid_file_paths;
        if invalid_paths_count > 0 {
            issues.push(IntegrityIssue {
                severity: "warning".into(),
                category: "paths".into(),
                message: format!("{invalid_paths_count} record(s) contain empty or invalid paths."),
                repair_guidance:
                    "Trigger a folder scan in Folders view to re-index current file paths.".into(),
            });
        }

        // 5. Missing files on disk check
        let missing_files_count: usize = conn
            .query_row(
                "SELECT count(*) FROM indexed_files WHERE status = 'missing'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if missing_files_count > 0 {
            issues.push(IntegrityIssue {
                severity: "info".into(),
                category: "missing_files".into(),
                message: format!("{missing_files_count} file(s) are currently marked as missing on disk."),
                repair_guidance: "Files may be on a disconnected drive or were moved. Reconnect storage or run a scan to update statuses. LibOne will never automatically delete user records.".into(),
            });
        }

        // 6. Summary counts
        let summary = Self::read_summary_from_conn(&conn)?;
        let is_healthy = issues.iter().all(|i| i.severity == "info");

        Ok(DatabaseIntegrityReport {
            is_healthy,
            schema_version: summary.schema_version,
            integrity_check_passed,
            foreign_key_check_passed,
            missing_tables,
            foreign_key_violations: fk_violations,
            invalid_paths_count,
            missing_files_count,
            total_roots: summary.total_roots,
            total_files: summary.total_files,
            total_collections: summary.total_collections,
            total_tags: summary.total_tags,
            total_notes: summary.total_notes,
            issues,
        })
    }

    /// Reads aggregate metrics for scan roots and indexed files for the diagnostics report.
    pub fn get_diagnostics_metrics(&self) -> Result<(ScanHealthInfo, FileCountsHealthInfo)> {
        let conn = self.open()?;

        // Scan roots health
        let (total_roots, enabled_roots, disabled_roots, max_started, max_completed): (
            usize,
            usize,
            usize,
            Option<String>,
            Option<String>,
        ) = conn.query_row(
            "SELECT 
                COUNT(*),
                COALESCE(SUM(CASE WHEN enabled = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN enabled = 0 THEN 1 ELSE 0 END), 0),
                MAX(last_scan_started_at),
                MAX(last_scan_completed_at)
             FROM scan_roots",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;

        let last_scan_status: Option<String> = conn
            .query_row(
                "SELECT last_scan_status FROM scan_roots WHERE last_scan_completed_at IS NOT NULL ORDER BY last_scan_completed_at DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;

        let scan_health = ScanHealthInfo {
            last_scan_started_at: max_started,
            last_scan_completed_at: max_completed,
            last_scan_status,
            enabled_roots_count: enabled_roots,
            disabled_roots_count: disabled_roots,
            total_roots_count: total_roots,
        };

        // File counts health
        let (indexed_count, missing_count, inaccessible_count, error_count, total_tracked): (
            usize,
            usize,
            usize,
            usize,
            usize,
        ) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN status = 'indexed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'missing' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'inaccessible' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status NOT IN ('indexed', 'missing', 'inaccessible') THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM indexed_files",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;

        let file_counts = FileCountsHealthInfo {
            indexed_files_count: indexed_count,
            missing_files_count: missing_count,
            inaccessible_files_count: inaccessible_count,
            error_files_count: error_count,
            total_tracked_files: total_tracked,
        };

        Ok((scan_health, file_counts))
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

    // ── Phase 2: Collections ──────────────────────────────────────────────────

    pub fn create_collection(
        &self,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
    ) -> Result<Collection> {
        let conn = self.open()?;
        let ts = now();
        conn.execute(
            "INSERT INTO collections(name, description_nullable, color_nullable, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, ?4)",
            params![name.trim(), description.map(|d| d.trim()), color.map(|c| c.trim()), ts],
        )?;
        let id = conn.last_insert_rowid();
        self.get_collection(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_collection(&self, id: i64) -> Result<Option<Collection>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description_nullable, color_nullable, created_at, updated_at FROM collections WHERE id=?1",
        )?;
        stmt.query_row(params![id], collection_from_row).optional()
    }

    pub fn get_collection_by_name(&self, name: &str) -> Result<Option<Collection>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description_nullable, color_nullable, created_at, updated_at FROM collections WHERE name=?1 COLLATE NOCASE",
        )?;
        stmt.query_row(params![name.trim()], collection_from_row)
            .optional()
    }

    pub fn list_collections(&self) -> Result<Vec<Collection>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, description_nullable, color_nullable, created_at, updated_at FROM collections ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], collection_from_row)?;
        rows.collect()
    }

    pub fn update_collection(
        &self,
        id: i64,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
    ) -> Result<Collection> {
        let conn = self.open()?;
        let ts = now();
        let rows = conn.execute(
            "UPDATE collections SET name=?1, description_nullable=?2, color_nullable=?3, updated_at=?4 WHERE id=?5",
            params![name.trim(), description.map(|d| d.trim()), color.map(|c| c.trim()), ts, id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        self.get_collection(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn delete_collection(&self, id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM collections WHERE id=?1", params![id])?;
        Ok(())
    }

    // ── Phase 2: Tags ─────────────────────────────────────────────────────────

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag> {
        let conn = self.open()?;
        let ts = now();
        conn.execute(
            "INSERT INTO tags(name, color_nullable, created_at) VALUES(?1, ?2, ?3)",
            params![name.trim(), color.map(|c| c.trim()), ts],
        )?;
        let id = conn.last_insert_rowid();
        self.get_tag(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_tag(&self, id: i64) -> Result<Option<Tag>> {
        let conn = self.open()?;
        let mut stmt =
            conn.prepare("SELECT id, name, color_nullable, created_at FROM tags WHERE id=?1")?;
        stmt.query_row(params![id], tag_from_row).optional()
    }

    pub fn get_tag_by_name(&self, name: &str) -> Result<Option<Tag>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, color_nullable, created_at FROM tags WHERE name=?1 COLLATE NOCASE",
        )?;
        stmt.query_row(params![name.trim()], tag_from_row)
            .optional()
    }

    pub fn list_tags(&self) -> Result<Vec<Tag>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, color_nullable, created_at FROM tags ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], tag_from_row)?;
        rows.collect()
    }

    pub fn update_tag(&self, id: i64, name: &str, color: Option<&str>) -> Result<Tag> {
        let conn = self.open()?;
        let rows = conn.execute(
            "UPDATE tags SET name=?1, color_nullable=?2 WHERE id=?3",
            params![name.trim(), color.map(|c| c.trim()), id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        self.get_tag(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn delete_tag(&self, id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM tags WHERE id=?1", params![id])?;
        Ok(())
    }

    // ── Phase 2: File-Collection Associations ─────────────────────────────────

    pub fn add_file_to_collection(
        &self,
        file_id: i64,
        collection_id: i64,
    ) -> Result<FileCollectionLink> {
        let conn = self.open()?;
        let ts = now();
        conn.execute(
            "INSERT INTO file_collections(file_id, collection_id, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(file_id, collection_id) DO NOTHING",
            params![file_id, collection_id, ts],
        )?;
        let created_at: String = conn.query_row(
            "SELECT created_at FROM file_collections WHERE file_id=?1 AND collection_id=?2",
            params![file_id, collection_id],
            |r| r.get(0),
        )?;
        Ok(FileCollectionLink {
            file_id,
            collection_id,
            created_at,
        })
    }

    pub fn remove_file_from_collection(&self, file_id: i64, collection_id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute(
            "DELETE FROM file_collections WHERE file_id=?1 AND collection_id=?2",
            params![file_id, collection_id],
        )?;
        Ok(())
    }

    pub fn list_file_collections(&self, file_id: i64) -> Result<Vec<Collection>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.name, c.description_nullable, c.color_nullable, c.created_at, c.updated_at
             FROM collections c
             INNER JOIN file_collections fc ON fc.collection_id = c.id
             WHERE fc.file_id = ?1
             ORDER BY c.name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map(params![file_id], collection_from_row)?;
        rows.collect()
    }

    pub fn list_file_collection_links(
        &self,
        file_id: Option<i64>,
        collection_id: Option<i64>,
    ) -> Result<Vec<FileCollectionLink>> {
        let conn = self.open()?;
        let mut sql =
            "SELECT file_id, collection_id, created_at FROM file_collections WHERE 1=1".to_string();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(fid) = file_id {
            sql.push_str(" AND file_id = ?");
            values.push(Box::new(fid));
        }
        if let Some(cid) = collection_id {
            sql.push_str(" AND collection_id = ?");
            values.push(Box::new(cid));
        }
        sql.push_str(" ORDER BY file_id ASC, collection_id ASC");
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(refs.as_slice(), |r| {
            Ok(FileCollectionLink {
                file_id: r.get(0)?,
                collection_id: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn bulk_add_file_collections(&self, file_id: i64, collection_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let ts = now();
        for &cid in collection_ids {
            tx.execute(
                "INSERT INTO file_collections(file_id, collection_id, created_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(file_id, collection_id) DO NOTHING",
                params![file_id, cid, ts],
            )?;
        }
        tx.commit()
    }

    pub fn bulk_remove_file_collections(&self, file_id: i64, collection_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        for &cid in collection_ids {
            tx.execute(
                "DELETE FROM file_collections WHERE file_id=?1 AND collection_id=?2",
                params![file_id, cid],
            )?;
        }
        tx.commit()
    }

    pub fn set_file_collections(&self, file_id: i64, collection_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM file_collections WHERE file_id=?1",
            params![file_id],
        )?;
        let ts = now();
        for &cid in collection_ids {
            tx.execute(
                "INSERT INTO file_collections(file_id, collection_id, created_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(file_id, collection_id) DO NOTHING",
                params![file_id, cid, ts],
            )?;
        }
        tx.commit()
    }

    // ── Phase 2: File-Tag Associations ────────────────────────────────────────

    pub fn add_file_to_tag(&self, file_id: i64, tag_id: i64) -> Result<FileTagLink> {
        let conn = self.open()?;
        let ts = now();
        conn.execute(
            "INSERT INTO file_tags(file_id, tag_id, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(file_id, tag_id) DO NOTHING",
            params![file_id, tag_id, ts],
        )?;
        let created_at: String = conn.query_row(
            "SELECT created_at FROM file_tags WHERE file_id=?1 AND tag_id=?2",
            params![file_id, tag_id],
            |r| r.get(0),
        )?;
        Ok(FileTagLink {
            file_id,
            tag_id,
            created_at,
        })
    }

    pub fn remove_file_from_tag(&self, file_id: i64, tag_id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute(
            "DELETE FROM file_tags WHERE file_id=?1 AND tag_id=?2",
            params![file_id, tag_id],
        )?;
        Ok(())
    }

    pub fn list_file_tags(&self, file_id: i64) -> Result<Vec<Tag>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.color_nullable, t.created_at
             FROM tags t
             INNER JOIN file_tags ft ON ft.tag_id = t.id
             WHERE ft.file_id = ?1
             ORDER BY t.name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map(params![file_id], tag_from_row)?;
        rows.collect()
    }

    pub fn list_file_tag_links(
        &self,
        file_id: Option<i64>,
        tag_id: Option<i64>,
    ) -> Result<Vec<FileTagLink>> {
        let conn = self.open()?;
        let mut sql = "SELECT file_id, tag_id, created_at FROM file_tags WHERE 1=1".to_string();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        if let Some(fid) = file_id {
            sql.push_str(" AND file_id = ?");
            values.push(Box::new(fid));
        }
        if let Some(tid) = tag_id {
            sql.push_str(" AND tag_id = ?");
            values.push(Box::new(tid));
        }
        sql.push_str(" ORDER BY file_id ASC, tag_id ASC");
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(refs.as_slice(), |r| {
            Ok(FileTagLink {
                file_id: r.get(0)?,
                tag_id: r.get(1)?,
                created_at: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    pub fn bulk_add_file_tags(&self, file_id: i64, tag_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let ts = now();
        for &tid in tag_ids {
            tx.execute(
                "INSERT INTO file_tags(file_id, tag_id, created_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(file_id, tag_id) DO NOTHING",
                params![file_id, tid, ts],
            )?;
        }
        tx.commit()
    }

    pub fn bulk_remove_file_tags(&self, file_id: i64, tag_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        for &tid in tag_ids {
            tx.execute(
                "DELETE FROM file_tags WHERE file_id=?1 AND tag_id=?2",
                params![file_id, tid],
            )?;
        }
        tx.commit()
    }

    pub fn set_file_tags(&self, file_id: i64, tag_ids: &[i64]) -> Result<()> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM file_tags WHERE file_id=?1", params![file_id])?;
        let ts = now();
        for &tid in tag_ids {
            tx.execute(
                "INSERT INTO file_tags(file_id, tag_id, created_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(file_id, tag_id) DO NOTHING",
                params![file_id, tid, ts],
            )?;
        }
        tx.commit()
    }

    // ── Phase 2: File Notes ───────────────────────────────────────────────────

    pub fn create_file_note(&self, file_id: i64, note_text: &str) -> Result<FileNote> {
        let conn = self.open()?;
        let ts = now();
        conn.execute(
            "INSERT INTO file_notes(file_id, note_text, created_at, updated_at) VALUES(?1, ?2, ?3, ?3)",
            params![file_id, note_text.trim(), ts],
        )?;
        let id = conn.last_insert_rowid();
        self.get_file_note(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn update_file_note(&self, note_id: i64, note_text: &str) -> Result<FileNote> {
        let conn = self.open()?;
        let ts = now();
        let rows = conn.execute(
            "UPDATE file_notes SET note_text=?1, updated_at=?2 WHERE id=?3",
            params![note_text.trim(), ts, note_id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        self.get_file_note(note_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_file_note(&self, note_id: i64) -> Result<Option<FileNote>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_id, note_text, created_at, updated_at FROM file_notes WHERE id=?1",
        )?;
        stmt.query_row(params![note_id], note_from_row).optional()
    }

    pub fn list_file_notes(&self, file_id: i64) -> Result<Vec<FileNote>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_id, note_text, created_at, updated_at FROM file_notes WHERE file_id=?1 ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![file_id], note_from_row)?;
        rows.collect()
    }

    /// All private notes in the library, used by the metadata-only exporter.
    pub fn list_all_file_notes(&self) -> Result<Vec<FileNote>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, file_id, note_text, created_at, updated_at FROM file_notes ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], note_from_row)?;
        rows.collect()
    }

    pub fn delete_file_note(&self, note_id: i64) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM file_notes WHERE id=?1", params![note_id])?;
        Ok(())
    }

    // ── Phase 3: Suggestions & Duplicates ────────────────────────────────────

    pub fn insert_organization_suggestion(
        &self,
        suggestion_type: &str,
        title: &str,
        rationale: &str,
        confidence: f64,
        file_ids: &[i64],
        suggested_tag_id: Option<i64>,
        suggested_tag_name: Option<&str>,
        suggested_collection_id: Option<i64>,
        suggested_collection_name: Option<&str>,
    ) -> Result<OrganizationSuggestion> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let ts = now();
        let clamped_conf = confidence.clamp(0.0, 1.0);

        tx.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            params![suggestion_type.trim(), title.trim(), rationale.trim(), clamped_conf, ts],
        )?;
        let id = tx.last_insert_rowid();

        for &fid in file_ids {
            tx.execute(
                "INSERT INTO suggestion_files (suggestion_id, file_id) VALUES (?1, ?2)
                 ON CONFLICT(suggestion_id, file_id) DO NOTHING",
                params![id, fid],
            )?;
        }

        if suggested_tag_id.is_some() || suggested_tag_name.is_some() {
            tx.execute(
                "INSERT INTO suggestion_tags (suggestion_id, tag_id_nullable, suggested_name_nullable)
                 VALUES (?1, ?2, ?3)",
                params![id, suggested_tag_id, suggested_tag_name.map(|n| n.trim())],
            )?;
        }

        if suggested_collection_id.is_some() || suggested_collection_name.is_some() {
            tx.execute(
                "INSERT INTO suggestion_collections (suggestion_id, collection_id_nullable, suggested_name_nullable)
                 VALUES (?1, ?2, ?3)",
                params![id, suggested_collection_id, suggested_collection_name.map(|n| n.trim())],
            )?;
        }

        tx.commit()?;

        self.get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_organization_suggestion(&self, id: i64) -> Result<Option<OrganizationSuggestion>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, suggestion_type, title, rationale, confidence, status, created_at, reviewed_at
             FROM organization_suggestions WHERE id=?1",
        )?;
        let suggestion = stmt
            .query_row(params![id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, f64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .optional()?;

        let (id, suggestion_type, title, rationale, confidence, status, created_at, reviewed_at) =
            match suggestion {
                Some(s) => s,
                None => return Ok(None),
            };

        let mut files_stmt = conn.prepare(
            "SELECT file_id FROM suggestion_files WHERE suggestion_id=?1 ORDER BY file_id ASC",
        )?;
        let affected_file_ids: Vec<i64> = files_stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        let mut tag_stmt = conn.prepare(
            "SELECT tag_id_nullable, suggested_name_nullable FROM suggestion_tags WHERE suggestion_id=?1 LIMIT 1",
        )?;
        let tag_info: Option<(Option<i64>, Option<String>)> = tag_stmt
            .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let (suggested_tag_id, suggested_tag_name) = match tag_info {
            Some((tid, tname)) => (tid, tname),
            None => (None, None),
        };

        let mut col_stmt = conn.prepare(
            "SELECT collection_id_nullable, suggested_name_nullable FROM suggestion_collections WHERE suggestion_id=?1 LIMIT 1",
        )?;
        let col_info: Option<(Option<i64>, Option<String>)> = col_stmt
            .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let (suggested_collection_id, suggested_collection_name) = match col_info {
            Some((cid, cname)) => (cid, cname),
            None => (None, None),
        };

        Ok(Some(OrganizationSuggestion {
            id,
            suggestion_type,
            title,
            rationale,
            confidence,
            status,
            created_at,
            reviewed_at,
            affected_file_ids,
            suggested_tag_id,
            suggested_tag_name,
            suggested_collection_id,
            suggested_collection_name,
        }))
    }

    pub fn list_organization_suggestions(
        &self,
        status_filter: Option<&str>,
        type_filter: Option<&str>,
    ) -> Result<Vec<OrganizationSuggestion>> {
        let conn = self.open()?;
        let mut sql = "SELECT id FROM organization_suggestions WHERE 1=1".to_string();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(st) = status_filter.filter(|s| !s.trim().is_empty() && *s != "all") {
            sql.push_str(" AND status = ?");
            values.push(Box::new(st.trim().to_lowercase()));
        }

        if let Some(tf) = type_filter.filter(|t| !t.trim().is_empty() && *t != "all") {
            sql.push_str(" AND suggestion_type = ?");
            values.push(Box::new(tf.trim().to_string()));
        }

        sql.push_str(" ORDER BY confidence DESC, id DESC");
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let ids: Vec<i64> = stmt
            .query_map(refs.as_slice(), |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        let mut list = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(s) = self.get_organization_suggestion(id)? {
                list.push(s);
            }
        }
        Ok(list)
    }

    pub fn update_suggestion_status(&self, id: i64, status: &str) -> Result<()> {
        let conn = self.open()?;
        let ts = now();
        let rows = conn.execute(
            "UPDATE organization_suggestions SET status=?1, reviewed_at=?2 WHERE id=?3",
            params![status.trim(), ts, id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub fn clear_pending_organization_suggestions(&self) -> Result<()> {
        let conn = self.open()?;
        conn.execute(
            "DELETE FROM organization_suggestions WHERE status='pending'",
            [],
        )?;
        Ok(())
    }

    pub fn insert_duplicate_group(
        &self,
        match_type: &str,
        confidence: f64,
        file_ids: &[i64],
    ) -> Result<DuplicateGroup> {
        let mut conn = self.open()?;
        let tx = conn.transaction()?;
        let ts = now();
        let clamped_conf = confidence.clamp(0.0, 1.0);

        tx.execute(
            "INSERT INTO duplicate_groups (match_type, confidence, created_at, status)
             VALUES (?1, ?2, ?3, 'pending')",
            params![match_type.trim(), clamped_conf, ts],
        )?;
        let id = tx.last_insert_rowid();

        for &fid in file_ids {
            tx.execute(
                "INSERT INTO duplicate_group_files (group_id, file_id) VALUES (?1, ?2)
                 ON CONFLICT(group_id, file_id) DO NOTHING",
                params![id, fid],
            )?;
        }

        tx.commit()?;

        self.get_duplicate_group(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_duplicate_group(&self, id: i64) -> Result<Option<DuplicateGroup>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT id, match_type, confidence, created_at, status FROM duplicate_groups WHERE id=?1",
        )?;
        let group = stmt
            .query_row(params![id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .optional()?;

        let (id, match_type, confidence, created_at, status) = match group {
            Some(g) => g,
            None => return Ok(None),
        };

        let mut files_stmt = conn.prepare(
            "SELECT file_id FROM duplicate_group_files WHERE group_id=?1 ORDER BY file_id ASC",
        )?;
        let file_ids: Vec<i64> = files_stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        Ok(Some(DuplicateGroup {
            id,
            match_type,
            confidence,
            created_at,
            status,
            file_ids,
        }))
    }

    pub fn list_duplicate_groups(
        &self,
        status_filter: Option<&str>,
    ) -> Result<Vec<DuplicateGroup>> {
        let conn = self.open()?;
        let mut sql = "SELECT id FROM duplicate_groups WHERE 1=1".to_string();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(st) = status_filter.filter(|s| !s.trim().is_empty() && *s != "all") {
            sql.push_str(" AND status = ?");
            values.push(Box::new(st.trim().to_lowercase()));
        }

        sql.push_str(" ORDER BY confidence DESC, id DESC");
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let ids: Vec<i64> = stmt
            .query_map(refs.as_slice(), |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        let mut list = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(g) = self.get_duplicate_group(id)? {
                list.push(g);
            }
        }
        Ok(list)
    }

    pub fn update_duplicate_group_status(&self, id: i64, status: &str) -> Result<()> {
        let conn = self.open()?;
        let rows = conn.execute(
            "UPDATE duplicate_groups SET status=?1 WHERE id=?2",
            params![status.trim(), id],
        )?;
        if rows == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        Ok(())
    }

    pub fn clear_pending_duplicate_groups(&self) -> Result<()> {
        let conn = self.open()?;
        conn.execute("DELETE FROM duplicate_groups WHERE status='pending'", [])?;
        Ok(())
    }

    pub fn list_all_suggestion_files(&self) -> Result<Vec<SuggestionFileLink>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT suggestion_id, file_id FROM suggestion_files ORDER BY suggestion_id ASC, file_id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SuggestionFileLink {
                suggestion_id: r.get(0)?,
                file_id: r.get(1)?,
            })
        })?;
        rows.collect()
    }

    pub fn list_all_duplicate_group_files(&self) -> Result<Vec<DuplicateGroupFileLink>> {
        let conn = self.open()?;
        let mut stmt = conn.prepare(
            "SELECT group_id, file_id FROM duplicate_group_files ORDER BY group_id ASC, file_id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DuplicateGroupFileLink {
                group_id: r.get(0)?,
                file_id: r.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Generates local organization suggestions and duplicate groups, persisting them into SQLite.
    pub fn generate_and_store_suggestions(
        &self,
        options: Option<crate::analyzer::SuggestionOptions>,
    ) -> Result<(usize, usize)> {
        let engine = crate::analyzer::SuggestionEngine::new(options.unwrap_or_default());
        engine.run_and_persist(self)
    }

    /// Approves an organization suggestion, creating or reusing Phase 2 metadata (collections, tags)
    /// and linking affected files without altering or moving physical files on disk.
    /// Operation is idempotent: repeated approval returns the approved suggestion safely.
    pub fn approve_suggestion(&self, id: i64) -> Result<OrganizationSuggestion> {
        let suggestion = self
            .get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if suggestion.status == "approved" {
            return Ok(suggestion);
        }

        // Validate that all affected file IDs exist in the indexed files table
        for &fid in &suggestion.affected_file_ids {
            if self.get_indexed_file(fid)?.is_none() {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }

        match suggestion.suggestion_type.as_str() {
            "suggest_collection" => {
                let col_id = if let Some(cid) = suggestion.suggested_collection_id {
                    if self.get_collection(cid)?.is_some() {
                        cid
                    } else if let Some(ref name) = suggestion.suggested_collection_name {
                        let existing = self
                            .list_collections()?
                            .into_iter()
                            .find(|c| c.name.eq_ignore_ascii_case(name));
                        match existing {
                            Some(c) => c.id,
                            None => self.create_collection(name, None, None)?.id,
                        }
                    } else {
                        return Err(rusqlite::Error::QueryReturnedNoRows);
                    }
                } else if let Some(ref name) = suggestion.suggested_collection_name {
                    let existing = self
                        .list_collections()?
                        .into_iter()
                        .find(|c| c.name.eq_ignore_ascii_case(name));
                    match existing {
                        Some(c) => c.id,
                        None => self.create_collection(name, None, None)?.id,
                    }
                } else {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                };

                for &fid in &suggestion.affected_file_ids {
                    let _ = self.add_file_to_collection(fid, col_id);
                }
            }
            "suggest_tag" => {
                let tag_id = if let Some(tid) = suggestion.suggested_tag_id {
                    if self.get_tag(tid)?.is_some() {
                        tid
                    } else if let Some(ref name) = suggestion.suggested_tag_name {
                        let existing = self
                            .list_tags()?
                            .into_iter()
                            .find(|t| t.name.eq_ignore_ascii_case(name));
                        match existing {
                            Some(t) => t.id,
                            None => self.create_tag(name, None)?.id,
                        }
                    } else {
                        return Err(rusqlite::Error::QueryReturnedNoRows);
                    }
                } else if let Some(ref name) = suggestion.suggested_tag_name {
                    let existing = self
                        .list_tags()?
                        .into_iter()
                        .find(|t| t.name.eq_ignore_ascii_case(name));
                    match existing {
                        Some(t) => t.id,
                        None => self.create_tag(name, None)?.id,
                    }
                } else {
                    return Err(rusqlite::Error::QueryReturnedNoRows);
                };

                for &fid in &suggestion.affected_file_ids {
                    let _ = self.add_file_to_tag(fid, tag_id);
                }
            }
            "likely_version" => {
                let tag_name = suggestion
                    .suggested_tag_name
                    .as_deref()
                    .unwrap_or("version-series");
                let existing = self
                    .list_tags()?
                    .into_iter()
                    .find(|t| t.name.eq_ignore_ascii_case(tag_name));
                let tag_id = match existing {
                    Some(t) => t.id,
                    None => self.create_tag(tag_name, None)?.id,
                };
                for &fid in &suggestion.affected_file_ids {
                    let _ = self.add_file_to_tag(fid, tag_id);
                }
            }
            "stale_file" => {
                let tag_name = suggestion
                    .suggested_tag_name
                    .as_deref()
                    .unwrap_or("archive");
                let existing = self
                    .list_tags()?
                    .into_iter()
                    .find(|t| t.name.eq_ignore_ascii_case(tag_name));
                let tag_id = match existing {
                    Some(t) => t.id,
                    None => self.create_tag(tag_name, None)?.id,
                };
                for &fid in &suggestion.affected_file_ids {
                    let _ = self.add_file_to_tag(fid, tag_id);
                }
            }
            "duplicate_files" => {
                let existing = self
                    .list_tags()?
                    .into_iter()
                    .find(|t| t.name.eq_ignore_ascii_case("duplicate"));
                let tag_id = match existing {
                    Some(t) => t.id,
                    None => self.create_tag("duplicate", None)?.id,
                };
                for &fid in &suggestion.affected_file_ids {
                    let _ = self.add_file_to_tag(fid, tag_id);
                }
            }
            _ => {}
        }

        self.update_suggestion_status(id, "approved")?;
        self.get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Rejects an organization suggestion. Idempotent.
    pub fn reject_suggestion(&self, id: i64) -> Result<OrganizationSuggestion> {
        let suggestion = self
            .get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if suggestion.status == "rejected" {
            return Ok(suggestion);
        }

        self.update_suggestion_status(id, "rejected")?;
        self.get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Dismisses an organization suggestion. Idempotent.
    pub fn dismiss_suggestion(&self, id: i64) -> Result<OrganizationSuggestion> {
        let suggestion = self
            .get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if suggestion.status == "dismissed" {
            return Ok(suggestion);
        }

        self.update_suggestion_status(id, "dismissed")?;
        self.get_organization_suggestion(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Approves a duplicate group without deleting or altering files.
    /// Tags duplicate files with "duplicate" and marks the preferred/canonical file with "canonical"
    /// and adds descriptive metadata notes. Idempotent.
    pub fn approve_duplicate_group(
        &self,
        group_id: i64,
        preferred_file_id: Option<i64>,
    ) -> Result<DuplicateGroup> {
        let group = self
            .get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if group.status == "approved" && preferred_file_id.is_none() {
            return Ok(group);
        }

        // Validate that all files exist in library
        for &fid in &group.file_ids {
            if self.get_indexed_file(fid)?.is_none() {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }

        // Validate preferred_file_id if provided
        if let Some(pref_id) = preferred_file_id {
            if !group.file_ids.contains(&pref_id) {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }

        // 1. Tag all group files as "duplicate"
        let dup_tag = match self
            .list_tags()?
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case("duplicate"))
        {
            Some(t) => t.id,
            None => self.create_tag("duplicate", None)?.id,
        };
        for &fid in &group.file_ids {
            let _ = self.add_file_to_tag(fid, dup_tag);
        }

        // 2. Mark canonical / preferred file in metadata only
        let canonical_id = preferred_file_id.unwrap_or(group.file_ids[0]);
        let canon_tag = match self
            .list_tags()?
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case("canonical"))
        {
            Some(t) => t.id,
            None => self.create_tag("canonical", None)?.id,
        };
        // Untag previous canonical if re-approving with different preference
        for &fid in &group.file_ids {
            if fid != canonical_id {
                let _ = self.remove_file_from_tag(fid, canon_tag);
            }
        }
        let _ = self.add_file_to_tag(canonical_id, canon_tag);

        // 3. Add private note to non-canonical duplicates linking back to the canonical item
        for &fid in &group.file_ids {
            if fid != canonical_id {
                let note = format!(
                    "[Duplicate Group #{group_id}] Canonical version is file #{canonical_id}"
                );
                let _ = self.create_file_note(fid, &note);
            }
        }

        self.update_duplicate_group_status(group_id, "approved")?;
        self.get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Fetches file comparison items for a list of file IDs.
    pub fn get_files_comparison(&self, file_ids: &[i64]) -> Result<Vec<FileComparisonItem>> {
        let mut items = Vec::new();
        let canon_tag_id = self
            .list_tags()?
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case("canonical"))
            .map(|t| t.id);

        for &fid in file_ids {
            if let Some(file) = self.get_indexed_file(fid)? {
                let root = self.get_scan_root(file.scan_root_id)?;
                let (root_name, root_enabled) = match root {
                    Some(r) => (r.display_name, r.enabled),
                    None => ("Unknown Root".to_string(), false),
                };
                let exists = Path::new(&file.absolute_path).exists();
                let is_canonical = if let Some(ctid) = canon_tag_id {
                    self.list_file_tags(fid)?.iter().any(|t| t.id == ctid)
                } else {
                    false
                };

                items.push(FileComparisonItem {
                    file,
                    root_display_name: root_name,
                    root_enabled,
                    exists_on_disk: exists,
                    is_canonical,
                });
            }
        }
        Ok(items)
    }

    /// Fetches a duplicate group with all associated comparison file items.
    pub fn get_duplicate_group_detail(
        &self,
        group_id: i64,
    ) -> Result<Option<DuplicateGroupDetail>> {
        let group = match self.get_duplicate_group(group_id)? {
            Some(g) => g,
            None => return Ok(None),
        };
        let files = self.get_files_comparison(&group.file_ids)?;
        Ok(Some(DuplicateGroupDetail { group, files }))
    }

    /// Explicitly marks a file in a duplicate group as preferred, updating metadata tags and notes.
    pub fn mark_duplicate_preferred(
        &self,
        group_id: i64,
        preferred_file_id: i64,
    ) -> Result<DuplicateGroupDetail> {
        let _ = self.approve_duplicate_group(group_id, Some(preferred_file_id))?;
        self.get_duplicate_group_detail(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Rejects a duplicate group. Idempotent.
    pub fn reject_duplicate_group(&self, group_id: i64) -> Result<DuplicateGroup> {
        let group = self
            .get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if group.status == "rejected" {
            return Ok(group);
        }

        self.update_duplicate_group_status(group_id, "rejected")?;
        self.get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Dismisses a duplicate group. Idempotent.
    pub fn dismiss_duplicate_group(&self, group_id: i64) -> Result<DuplicateGroup> {
        let group = self
            .get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

        if group.status == "dismissed" {
            return Ok(group);
        }

        self.update_duplicate_group_status(group_id, "dismissed")?;
        self.get_duplicate_group(group_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Bulk approves multiple suggestions. Returns structured result with successes and failures.
    pub fn bulk_approve_suggestions(&self, suggestion_ids: &[i64]) -> BulkSuggestionResult {
        let mut approved_ids = Vec::new();
        let mut failed_ids = Vec::new();
        let mut errors = Vec::new();

        for &id in suggestion_ids {
            match self.approve_suggestion(id) {
                Ok(_) => approved_ids.push(id),
                Err(e) => {
                    failed_ids.push(id);
                    errors.push(format!("Suggestion #{id}: {e}"));
                }
            }
        }

        BulkSuggestionResult {
            approved_ids,
            rejected_ids: Vec::new(),
            failed_ids,
            errors,
        }
    }

    /// Bulk rejects multiple suggestions. Returns structured result with successes and failures.
    pub fn bulk_reject_suggestions(&self, suggestion_ids: &[i64]) -> BulkSuggestionResult {
        let mut rejected_ids = Vec::new();
        let mut failed_ids = Vec::new();
        let mut errors = Vec::new();

        for &id in suggestion_ids {
            match self.reject_suggestion(id) {
                Ok(_) => rejected_ids.push(id),
                Err(e) => {
                    failed_ids.push(id);
                    errors.push(format!("Suggestion #{id}: {e}"));
                }
            }
        }

        BulkSuggestionResult {
            approved_ids: Vec::new(),
            rejected_ids,
            failed_ids,
            errors,
        }
    }
}

fn collection_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Collection> {
    Ok(Collection {
        id: r.get(0)?,
        name: r.get(1)?,
        description_nullable: r.get(2)?,
        color_nullable: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

fn tag_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get(0)?,
        name: r.get(1)?,
        color_nullable: r.get(2)?,
        created_at: r.get(3)?,
    })
}

fn note_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileNote> {
    Ok(FileNote {
        id: r.get(0)?,
        file_id: r.get(1)?,
        note_text: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
    })
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

fn conn_table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
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

/// Safely returns available and total disk bytes for the volume hosting `path`.
#[cfg(windows)]
pub fn get_disk_free_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;

    let dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };

    let mut wide: Vec<u16> = dir.as_os_str().encode_wide().collect();
    wide.push(0);

    let mut free_bytes_available: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free_bytes: u64 = 0;

    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }

    let ret = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available,
            &mut total_bytes,
            &mut total_free_bytes,
        )
    };

    if ret != 0 && total_bytes > 0 {
        Some((free_bytes_available, total_bytes))
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn get_disk_free_space(_path: &Path) -> Option<(u64, u64)> {
    None
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
            4
        );
        drop(c);
        std::fs::remove_dir_all(p).unwrap()
    }

    #[test]
    fn migration_v3_fresh_database_structure_and_constraints() {
        let (d, p) = db();
        let conn = d.open().unwrap();

        // 1. Verify schema version and migrations applied (at least v3)
        let schema_ver: i64 = conn
            .query_row(
                "SELECT schema_version FROM library_metadata WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(schema_ver >= 3);

        let versions: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(versions.contains(&1) && versions.contains(&2) && versions.contains(&3));

        // 2. Verify all new Phase 2 tables exist
        for table in &[
            "collections",
            "tags",
            "file_collections",
            "file_tags",
            "file_notes",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![table],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                > 0;
            assert!(exists, "Expected table '{table}' to exist");
        }

        // 3. Verify case-insensitive unique collection name
        conn.execute(
            "INSERT INTO collections (name, description_nullable, color_nullable, created_at, updated_at)
             VALUES ('Taxes', 'Tax docs', '#3b82f6', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let dup_col = conn.execute(
            "INSERT INTO collections (name, description_nullable, color_nullable, created_at, updated_at)
             VALUES ('taxes', 'duplicate', '#ff0000', datetime('now'), datetime('now'))",
            [],
        );
        assert!(
            dup_col.is_err(),
            "Expected duplicate case-insensitive collection name to fail"
        );

        // 4. Verify case-insensitive unique tag name
        conn.execute(
            "INSERT INTO tags (name, color_nullable, created_at) VALUES ('Invoice', '#10b981', datetime('now'))",
            [],
        )
        .unwrap();
        let dup_tag = conn.execute(
            "INSERT INTO tags (name, color_nullable, created_at) VALUES ('invoice', '#ff0000', datetime('now'))",
            [],
        );
        assert!(
            dup_tag.is_err(),
            "Expected duplicate case-insensitive tag name to fail"
        );

        // 5. Seed scan root and file
        let root = d.add_scan_root(r"C:\Docs", "Docs").unwrap();
        let file = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\Docs\sample.pdf".into(),
                normalized_path: normalize(r"C:\Docs\sample.pdf"),
                file_name: "sample.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 1024,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: Some("0001:0002".into()),
                status: "indexed".into(),
            })
            .unwrap();

        let col_id: i64 = conn
            .query_row("SELECT id FROM collections WHERE name='Taxes'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let tag_id: i64 = conn
            .query_row("SELECT id FROM tags WHERE name='Invoice'", [], |r| r.get(0))
            .unwrap();

        // 6. Link file to collection & tag, and add note
        conn.execute(
            "INSERT INTO file_collections (file_id, collection_id, created_at) VALUES (?1, ?2, datetime('now'))",
            params![file.id, col_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_tags (file_id, tag_id, created_at) VALUES (?1, ?2, datetime('now'))",
            params![file.id, tag_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_notes (file_id, note_text, created_at, updated_at) VALUES (?1, 'Important receipt', datetime('now'), datetime('now'))",
            params![file.id],
        )
        .unwrap();

        // 7. Verify unique pair constraints
        let dup_fc = conn.execute(
            "INSERT INTO file_collections (file_id, collection_id, created_at) VALUES (?1, ?2, datetime('now'))",
            params![file.id, col_id],
        );
        assert!(
            dup_fc.is_err(),
            "Expected duplicate file_collection to fail"
        );

        let dup_ft = conn.execute(
            "INSERT INTO file_tags (file_id, tag_id, created_at) VALUES (?1, ?2, datetime('now'))",
            params![file.id, tag_id],
        );
        assert!(dup_ft.is_err(), "Expected duplicate file_tag to fail");

        // 8. Verify cascade: deleting a collection removes only its links, never the file
        conn.execute("DELETE FROM collections WHERE id=?1", params![col_id])
            .unwrap();
        let fc_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_collections WHERE collection_id=?1",
                params![col_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fc_count, 0, "file_collections links should be removed");
        let file_still_exists = d.get_indexed_file(file.id).unwrap();
        assert!(
            file_still_exists.is_some(),
            "Indexed file must remain after collection deletion"
        );

        // 9. Verify cascade: deleting a tag removes only its links, never the file
        conn.execute("DELETE FROM tags WHERE id=?1", params![tag_id])
            .unwrap();
        let ft_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_tags WHERE tag_id=?1",
                params![tag_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ft_count, 0, "file_tags links should be removed");
        let file_still_exists2 = d.get_indexed_file(file.id).unwrap();
        assert!(
            file_still_exists2.is_some(),
            "Indexed file must remain after tag deletion"
        );

        // 10. Verify cascade: deleting file removes its notes
        conn.execute("DELETE FROM indexed_files WHERE id=?1", params![file.id])
            .unwrap();
        let note_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_notes WHERE file_id=?1",
                params![file.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            note_count, 0,
            "file_notes should be cascade removed when file is deleted"
        );

        drop(conn);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn migration_from_phase1_database_preserves_all_data() {
        let p = std::env::temp_dir().join(format!(
            "libone-p1-migration-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        let db_file = p.join("libone.db");

        // Manually build an exact Phase 1 (schema version 2) database with records
        {
            let conn = Connection::open(&db_file).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES (1, '2026-11-13T00:00:00Z');
                 INSERT INTO schema_migrations VALUES (2, '2026-11-13T00:01:00Z');

                 CREATE TABLE scan_roots (
                     id INTEGER PRIMARY KEY,
                     absolute_path TEXT NOT NULL,
                     normalized_path TEXT NOT NULL UNIQUE,
                     display_name TEXT NOT NULL,
                     enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1)),
                     created_at TEXT NOT NULL,
                     last_scan_started_at TEXT,
                     last_scan_completed_at TEXT,
                     last_scan_status TEXT NOT NULL DEFAULT 'never_scanned'
                 );

                 CREATE TABLE indexed_files (
                     id INTEGER PRIMARY KEY,
                     scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE RESTRICT,
                     absolute_path TEXT NOT NULL,
                     normalized_path TEXT NOT NULL UNIQUE,
                     file_name TEXT NOT NULL,
                     extension TEXT NOT NULL,
                     size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),
                     modified_at TEXT,
                     created_at_if_available TEXT,
                     discovered_at TEXT NOT NULL,
                     last_seen_at TEXT NOT NULL,
                     content_hash_nullable TEXT,
                     file_id_nullable TEXT,
                     status TEXT NOT NULL
                 );

                 CREATE TABLE library_metadata (
                     id INTEGER PRIMARY KEY CHECK(id=1),
                     schema_version INTEGER NOT NULL,
                     library_created_at TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );

                 INSERT INTO library_metadata VALUES (1, 2, '2026-11-13T00:00:00Z', '2026-11-13T00:01:00Z');

                 INSERT INTO scan_roots (id, absolute_path, normalized_path, display_name, enabled, created_at, last_scan_status)
                 VALUES (10, 'D:\\Documents', 'd:\\documents', 'My Documents', 1, '2026-11-13T00:00:00Z', 'completed');

                 INSERT INTO indexed_files (id, scan_root_id, absolute_path, normalized_path, file_name, extension, size_bytes, modified_at, discovered_at, last_seen_at, content_hash_nullable, file_id_nullable, status)
                 VALUES (100, 10, 'D:\\Documents\\Contract.pdf', 'd:\\documents\\contract.pdf', 'Contract.pdf', 'pdf', 204800, '2026-11-12T10:00:00Z', '2026-11-13T00:00:00Z', '2026-11-13T00:00:00Z', 'sha256_mock_hash_val', '0028a1f4:000000000014ef02', 'indexed');
                ",
            )
            .unwrap();
        }

        // Now open via LibOne Database::new, which executes migrate()
        let database = Database::new(&p).unwrap();
        let conn = database.open().unwrap();

        // 1. Verify schema version was updated from 2 to 4
        let schema_ver: i64 = conn
            .query_row(
                "SELECT schema_version FROM library_metadata WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(schema_ver, 4);

        // 2. Verify schema_migrations now includes 1, 2, 3, and 4
        let versions: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(versions, vec![1, 2, 3, 4]);

        // 3. Verify original Phase 1 scan root is completely intact
        let root = database
            .get_scan_root(10)
            .unwrap()
            .expect("Root 10 must exist");
        assert_eq!(root.id, 10);
        assert_eq!(root.absolute_path, "D:\\Documents");
        assert_eq!(root.display_name, "My Documents");
        assert!(root.enabled);
        assert_eq!(root.last_scan_status, "completed");

        // 4. Verify original Phase 1 file is completely intact
        let file = database
            .get_indexed_file(100)
            .unwrap()
            .expect("File 100 must exist");
        assert_eq!(file.id, 100);
        assert_eq!(file.scan_root_id, 10);
        assert_eq!(file.file_name, "Contract.pdf");
        assert_eq!(file.extension, "pdf");
        assert_eq!(file.size_bytes, 204800);
        assert_eq!(
            file.content_hash_nullable.as_deref(),
            Some("sha256_mock_hash_val")
        );
        assert_eq!(
            file.file_id_nullable.as_deref(),
            Some("0028a1f4:000000000014ef02")
        );
        assert_eq!(file.status, "indexed");

        // 5. Verify new Phase 2 tables can be used with the existing Phase 1 file
        conn.execute(
            "INSERT INTO collections (name, description_nullable, color_nullable, created_at, updated_at)
             VALUES ('Legal', 'Contracts and agreements', '#6366f1', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        let col_id: i64 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO file_collections (file_id, collection_id, created_at) VALUES (100, ?1, datetime('now'))",
            params![col_id],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO file_notes (file_id, note_text, created_at, updated_at) VALUES (100, 'Reviewed by counsel', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();

        let note_text: String = conn
            .query_row(
                "SELECT note_text FROM file_notes WHERE file_id=100",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(note_text, "Reviewed by counsel");

        // 6. Verify new Phase 3 tables can be used with the existing Phase 1 file
        conn.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES ('suggest_collection', 'Group Contracts', 'Found related files', 0.95, 'pending', datetime('now'))",
            [],
        )
        .unwrap();
        let sugg_id: i64 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO suggestion_files (suggestion_id, file_id) VALUES (?1, 100)",
            params![sugg_id],
        )
        .unwrap();

        drop(conn);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn migration_v4_fresh_database_structure_and_constraints() {
        let (d, p) = db();
        let conn = d.open().unwrap();

        // 1. Verify schema version and migrations applied
        let schema_ver: i64 = conn
            .query_row(
                "SELECT schema_version FROM library_metadata WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(schema_ver, 4);

        let versions: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(versions, vec![1, 2, 3, 4]);

        // 2. Verify all Phase 3 tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"organization_suggestions".to_string()));
        assert!(tables.contains(&"suggestion_files".to_string()));
        assert!(tables.contains(&"suggestion_tags".to_string()));
        assert!(tables.contains(&"suggestion_collections".to_string()));
        assert!(tables.contains(&"duplicate_groups".to_string()));
        assert!(tables.contains(&"duplicate_group_files".to_string()));

        // 3. Verify status constraint on organization_suggestions
        for valid_status in ["pending", "approved", "rejected", "dismissed"] {
            conn.execute(
                "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
                 VALUES ('suggest_tag', 'Tag', 'Rationale', 0.85, ?1, datetime('now'))",
                params![valid_status],
            )
            .unwrap();
        }
        let invalid_status = conn.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES ('suggest_tag', 'Tag', 'Rationale', 0.85, 'invalid_status', datetime('now'))",
            [],
        );
        assert!(
            invalid_status.is_err(),
            "Expected invalid suggestion status to fail"
        );

        // 4. Verify confidence range constraint (0.0 to 1.0)
        let invalid_conf_high = conn.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES ('suggest_tag', 'Tag', 'Rationale', 1.5, 'pending', datetime('now'))",
            [],
        );
        assert!(
            invalid_conf_high.is_err(),
            "Expected confidence > 1.0 to fail"
        );
        let invalid_conf_low = conn.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES ('suggest_tag', 'Tag', 'Rationale', -0.1, 'pending', datetime('now'))",
            [],
        );
        assert!(
            invalid_conf_low.is_err(),
            "Expected confidence < 0.0 to fail"
        );

        // 5. Verify status constraint on duplicate_groups
        for valid_status in ["pending", "approved", "rejected", "dismissed"] {
            conn.execute(
                "INSERT INTO duplicate_groups (match_type, confidence, created_at, status)
                 VALUES ('exact_hash', 1.0, datetime('now'), ?1)",
                params![valid_status],
            )
            .unwrap();
        }
        let invalid_dup_status = conn.execute(
            "INSERT INTO duplicate_groups (match_type, confidence, created_at, status)
             VALUES ('exact_hash', 1.0, datetime('now'), 'unknown')",
            [],
        );
        assert!(
            invalid_dup_status.is_err(),
            "Expected invalid duplicate_group status to fail"
        );

        // 6. Test foreign keys and cascade on suggestion_files
        let root = d.add_scan_root(r"C:\Docs", "Docs").unwrap();
        let file = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: r"C:\Docs\test.pdf".into(),
                normalized_path: normalize(r"C:\Docs\test.pdf"),
                file_name: "test.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 4096,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let sugg_id: i64 = conn
            .query_row("SELECT id FROM organization_suggestions LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();

        conn.execute(
            "INSERT INTO suggestion_files (suggestion_id, file_id) VALUES (?1, ?2)",
            params![sugg_id, file.id],
        )
        .unwrap();

        // Duplicate pair must fail
        let dup_sf = conn.execute(
            "INSERT INTO suggestion_files (suggestion_id, file_id) VALUES (?1, ?2)",
            params![sugg_id, file.id],
        );
        assert!(
            dup_sf.is_err(),
            "Expected duplicate (suggestion_id, file_id) to fail"
        );

        // Cascade delete on suggestion deletion
        conn.execute(
            "DELETE FROM organization_suggestions WHERE id=?1",
            params![sugg_id],
        )
        .unwrap();
        let sf_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM suggestion_files WHERE suggestion_id=?1",
                params![sugg_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sf_count, 0, "suggestion_files should cascade delete");

        // The original indexed file must remain untouched
        assert!(d.get_indexed_file(file.id).unwrap().is_some());

        // 7. Verify duplicate_groups and duplicate_group_files constraints
        let group_id: i64 = conn
            .query_row("SELECT id FROM duplicate_groups LIMIT 1", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO duplicate_group_files (group_id, file_id) VALUES (?1, ?2)",
            params![group_id, file.id],
        )
        .unwrap();

        // Duplicate pair in duplicate_group_files must fail
        let dup_dgf = conn.execute(
            "INSERT INTO duplicate_group_files (group_id, file_id) VALUES (?1, ?2)",
            params![group_id, file.id],
        );
        assert!(
            dup_dgf.is_err(),
            "Expected duplicate (group_id, file_id) to fail"
        );

        // Deleting duplicate_group cascades to duplicate_group_files
        conn.execute(
            "DELETE FROM duplicate_groups WHERE id=?1",
            params![group_id],
        )
        .unwrap();
        let dgf_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM duplicate_group_files WHERE group_id=?1",
                params![group_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dgf_count, 0, "duplicate_group_files should cascade delete");

        // The original indexed file must remain untouched
        assert!(d.get_indexed_file(file.id).unwrap().is_some());

        drop(conn);
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn migration_from_phase2_database_preserves_all_data() {
        let p = std::env::temp_dir().join(format!(
            "libone-p2-migration-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        let db_file = p.join("libone.db");

        // Manually build an exact Phase 2 (schema version 3) database with records
        {
            let conn = Connection::open(&db_file).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 INSERT INTO schema_migrations VALUES (1, '2026-11-13T00:00:00Z');
                 INSERT INTO schema_migrations VALUES (2, '2026-11-13T00:01:00Z');
                 INSERT INTO schema_migrations VALUES (3, '2026-11-13T00:02:00Z');

                 CREATE TABLE scan_roots (
                     id INTEGER PRIMARY KEY,
                     absolute_path TEXT NOT NULL,
                     normalized_path TEXT NOT NULL UNIQUE,
                     display_name TEXT NOT NULL,
                     enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1)),
                     created_at TEXT NOT NULL,
                     last_scan_started_at TEXT,
                     last_scan_completed_at TEXT,
                     last_scan_status TEXT NOT NULL DEFAULT 'never_scanned'
                 );

                 CREATE TABLE indexed_files (
                     id INTEGER PRIMARY KEY,
                     scan_root_id INTEGER NOT NULL REFERENCES scan_roots(id) ON DELETE RESTRICT,
                     absolute_path TEXT NOT NULL,
                     normalized_path TEXT NOT NULL UNIQUE,
                     file_name TEXT NOT NULL,
                     extension TEXT NOT NULL,
                     size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),
                     modified_at TEXT,
                     created_at_if_available TEXT,
                     discovered_at TEXT NOT NULL,
                     last_seen_at TEXT NOT NULL,
                     content_hash_nullable TEXT,
                     file_id_nullable TEXT,
                     status TEXT NOT NULL
                 );

                 CREATE TABLE library_metadata (
                     id INTEGER PRIMARY KEY CHECK(id=1),
                     schema_version INTEGER NOT NULL,
                     library_created_at TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );

                 INSERT INTO library_metadata VALUES (1, 3, '2026-11-13T00:00:00Z', '2026-11-13T00:02:00Z');

                 CREATE TABLE collections (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     description_nullable TEXT,
                     color_nullable TEXT,
                     created_at TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );

                 CREATE TABLE tags (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     name TEXT NOT NULL UNIQUE COLLATE NOCASE,
                     color_nullable TEXT,
                     created_at TEXT NOT NULL
                 );

                 CREATE TABLE file_collections (
                     file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                     collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                     created_at TEXT NOT NULL,
                     PRIMARY KEY (file_id, collection_id)
                 );

                 CREATE TABLE file_tags (
                     file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                     tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                     created_at TEXT NOT NULL,
                     PRIMARY KEY (file_id, tag_id)
                 );

                 CREATE TABLE file_notes (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     file_id INTEGER NOT NULL REFERENCES indexed_files(id) ON DELETE CASCADE,
                     note_text TEXT NOT NULL,
                     created_at TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );

                 INSERT INTO scan_roots (id, absolute_path, normalized_path, display_name, enabled, created_at, last_scan_status)
                 VALUES (20, 'E:\\Work', 'e:\\work', 'Work Folder', 1, '2026-11-14T00:00:00Z', 'completed');

                 INSERT INTO indexed_files (id, scan_root_id, absolute_path, normalized_path, file_name, extension, size_bytes, modified_at, discovered_at, last_seen_at, content_hash_nullable, file_id_nullable, status)
                 VALUES (200, 20, 'E:\\Work\\Report.pdf', 'e:\\work\\report.pdf', 'Report.pdf', 'pdf', 512000, '2026-11-14T10:00:00Z', '2026-11-14T10:00:00Z', '2026-11-14T10:00:00Z', 'hash_abc_123', 'vol:idx', 'indexed');

                 INSERT INTO collections (id, name, description_nullable, color_nullable, created_at, updated_at)
                 VALUES (5, 'Projects', 'Current active projects', '#10b981', '2026-11-14T10:05:00Z', '2026-11-14T10:05:00Z');

                 INSERT INTO tags (id, name, color_nullable, created_at)
                 VALUES (8, 'urgent', '#ef4444', '2026-11-14T10:06:00Z');

                 INSERT INTO file_collections (file_id, collection_id, created_at)
                 VALUES (200, 5, '2026-11-14T10:07:00Z');

                 INSERT INTO file_tags (file_id, tag_id, created_at)
                 VALUES (200, 8, '2026-11-14T10:08:00Z');

                 INSERT INTO file_notes (id, file_id, note_text, created_at, updated_at)
                 VALUES (12, 200, 'Initial draft note', '2026-11-14T10:09:00Z', '2026-11-14T10:09:00Z');
                ",
            )
            .unwrap();
        }

        // Open through LibOne Database::new, which runs migrate()
        let database = Database::new(&p).unwrap();
        let conn = database.open().unwrap();

        // 1. Verify schema version was bumped from 3 to 4
        let schema_ver: i64 = conn
            .query_row(
                "SELECT schema_version FROM library_metadata WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(schema_ver, 4);

        // 2. Verify schema_migrations has 1, 2, 3, 4
        let versions: Vec<i64> = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(versions, vec![1, 2, 3, 4]);

        // 3. Verify scan roots, files, collections, tags, links, and notes are preserved intact
        let root = database.get_scan_root(20).unwrap().expect("Root 20");
        assert_eq!(root.display_name, "Work Folder");

        let file = database.get_indexed_file(200).unwrap().expect("File 200");
        assert_eq!(file.file_name, "Report.pdf");

        let col = database.get_collection(5).unwrap().expect("Collection 5");
        assert_eq!(col.name, "Projects");

        let tag = database.get_tag(8).unwrap().expect("Tag 8");
        assert_eq!(tag.name, "urgent");

        let note = database.get_file_note(12).unwrap().expect("Note 12");
        assert_eq!(note.note_text, "Initial draft note");

        // 4. Verify Phase 3 suggestions can link to existing Phase 1/2 records
        conn.execute(
            "INSERT INTO organization_suggestions (suggestion_type, title, rationale, confidence, status, created_at)
             VALUES ('suggest_tag', 'Auto Tag urgent', 'High priority file', 0.9, 'pending', datetime('now'))",
            [],
        )
        .unwrap();
        let sugg_id: i64 = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO suggestion_files (suggestion_id, file_id) VALUES (?1, 200)",
            params![sugg_id],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO suggestion_tags (suggestion_id, tag_id_nullable, suggested_name_nullable)
             VALUES (?1, 8, 'urgent')",
            params![sugg_id],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO suggestion_collections (suggestion_id, collection_id_nullable, suggested_name_nullable)
             VALUES (?1, 5, 'Projects')",
            params![sugg_id],
        )
        .unwrap();

        drop(conn);
        std::fs::remove_dir_all(p).unwrap();
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

    #[test]
    fn collection_repository_crud_and_duplicates() {
        let (d, p) = db();

        // 1. Create
        let col = d
            .create_collection("Finance", Some("Financial documents"), Some("#10b981"))
            .unwrap();
        assert_eq!(col.name, "Finance");
        assert_eq!(
            col.description_nullable.as_deref(),
            Some("Financial documents")
        );
        assert_eq!(col.color_nullable.as_deref(), Some("#10b981"));

        // 2. Get by ID & Name
        let fetched = d
            .get_collection(col.id)
            .unwrap()
            .expect("Should find collection");
        assert_eq!(fetched.id, col.id);
        let fetched_by_name = d
            .get_collection_by_name("finance")
            .unwrap()
            .expect("Case-insensitive lookup");
        assert_eq!(fetched_by_name.id, col.id);

        // 3. Duplicate name returns Err
        let dup = d.create_collection("finance", None, None);
        assert!(dup.is_err(), "Duplicate collection name must fail");

        // 4. Update
        let updated = d
            .update_collection(
                col.id,
                "Accounting",
                Some("Tax and invoices"),
                Some("#3b82f6"),
            )
            .unwrap();
        assert_eq!(updated.name, "Accounting");
        assert_eq!(
            updated.description_nullable.as_deref(),
            Some("Tax and invoices")
        );
        assert_eq!(updated.color_nullable.as_deref(), Some("#3b82f6"));

        // 5. List
        let _col2 = d.create_collection("HR", None, None).unwrap();
        let list = d.list_collections().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Accounting");
        assert_eq!(list[1].name, "HR");

        // 6. Delete
        d.delete_collection(col.id).unwrap();
        assert!(d.get_collection(col.id).unwrap().is_none());
        assert_eq!(d.list_collections().unwrap().len(), 1);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn tag_repository_crud_and_duplicates() {
        let (d, p) = db();

        // 1. Create
        let tag = d.create_tag("Receipt", Some("#f59e0b")).unwrap();
        assert_eq!(tag.name, "Receipt");
        assert_eq!(tag.color_nullable.as_deref(), Some("#f59e0b"));

        // 2. Get by ID & Name
        let fetched = d.get_tag(tag.id).unwrap().expect("Should find tag");
        assert_eq!(fetched.id, tag.id);
        let fetched_by_name = d
            .get_tag_by_name("receipt")
            .unwrap()
            .expect("Case-insensitive lookup");
        assert_eq!(fetched_by_name.id, tag.id);

        // 3. Duplicate name returns Err
        let dup = d.create_tag("RECEIPT", None);
        assert!(dup.is_err(), "Duplicate tag name must fail");

        // 4. Update
        let updated = d.update_tag(tag.id, "Invoice", Some("#ef4444")).unwrap();
        assert_eq!(updated.name, "Invoice");
        assert_eq!(updated.color_nullable.as_deref(), Some("#ef4444"));

        // 5. List
        let _tag2 = d.create_tag("Contract", None).unwrap();
        let list = d.list_tags().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Contract");
        assert_eq!(list[1].name, "Invoice");

        // 6. Delete
        d.delete_tag(tag.id).unwrap();
        assert!(d.get_tag(tag.id).unwrap().is_none());
        assert_eq!(d.list_tags().unwrap().len(), 1);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn file_collections_and_tags_associations_and_bulk() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\Docs", "Docs").unwrap();
        let f1 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Docs\1.pdf".into(),
                normalized_path: normalize(r"C:\Docs\1.pdf"),
                file_name: "1.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let c1 = d.create_collection("Col 1", None, None).unwrap();
        let c2 = d.create_collection("Col 2", None, None).unwrap();
        let t1 = d.create_tag("Tag 1", None).unwrap();
        let t2 = d.create_tag("Tag 2", None).unwrap();

        // 1. Add file to collection & duplicate-safe check
        let link1 = d.add_file_to_collection(f1.id, c1.id).unwrap();
        assert_eq!(link1.file_id, f1.id);
        assert_eq!(link1.collection_id, c1.id);
        // Repeated call does not fail or duplicate
        let link1_dup = d.add_file_to_collection(f1.id, c1.id).unwrap();
        assert_eq!(link1_dup.collection_id, c1.id);

        let links = d.list_file_collections(f1.id).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, c1.id);

        // 2. Bulk add / remove collections
        d.bulk_add_file_collections(f1.id, &[c1.id, c2.id]).unwrap();
        assert_eq!(d.list_file_collections(f1.id).unwrap().len(), 2);

        d.bulk_remove_file_collections(f1.id, &[c1.id]).unwrap();
        let rem_cols = d.list_file_collections(f1.id).unwrap();
        assert_eq!(rem_cols.len(), 1);
        assert_eq!(rem_cols[0].id, c2.id);

        // 3. Set collections (replace)
        d.set_file_collections(f1.id, &[c1.id]).unwrap();
        let set_cols = d.list_file_collections(f1.id).unwrap();
        assert_eq!(set_cols.len(), 1);
        assert_eq!(set_cols[0].id, c1.id);

        // 4. File-tag associations & duplicate-safe check
        let tlink1 = d.add_file_to_tag(f1.id, t1.id).unwrap();
        assert_eq!(tlink1.tag_id, t1.id);
        let tlink1_dup = d.add_file_to_tag(f1.id, t1.id).unwrap();
        assert_eq!(tlink1_dup.tag_id, t1.id);

        d.bulk_add_file_tags(f1.id, &[t1.id, t2.id]).unwrap();
        assert_eq!(d.list_file_tags(f1.id).unwrap().len(), 2);

        d.bulk_remove_file_tags(f1.id, &[t2.id]).unwrap();
        assert_eq!(d.list_file_tags(f1.id).unwrap().len(), 1);

        d.set_file_tags(f1.id, &[t2.id]).unwrap();
        let set_tags = d.list_file_tags(f1.id).unwrap();
        assert_eq!(set_tags.len(), 1);
        assert_eq!(set_tags[0].id, t2.id);

        // 5. Query links table
        let clinks = d.list_file_collection_links(Some(f1.id), None).unwrap();
        assert_eq!(clinks.len(), 1);
        let tlinks = d.list_file_tag_links(Some(f1.id), None).unwrap();
        assert_eq!(tlinks.len(), 1);

        // 6. Deleting collection/tag removes association only, leaves file intact
        d.delete_collection(c1.id).unwrap();
        assert_eq!(d.list_file_collections(f1.id).unwrap().len(), 0);
        assert!(d.get_indexed_file(f1.id).unwrap().is_some());

        d.delete_tag(t2.id).unwrap();
        assert_eq!(d.list_file_tags(f1.id).unwrap().len(), 0);
        assert!(d.get_indexed_file(f1.id).unwrap().is_some());

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn file_notes_repository_lifecycle() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\Docs", "Docs").unwrap();
        let f1 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Docs\note_test.pdf".into(),
                normalized_path: normalize(r"C:\Docs\note_test.pdf"),
                file_name: "note_test.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        // 1. Create note
        let note = d.create_file_note(f1.id, "Initial note content").unwrap();
        assert_eq!(note.file_id, f1.id);
        assert_eq!(note.note_text, "Initial note content");

        // 2. Get and list
        let fetched = d.get_file_note(note.id).unwrap().expect("Should find note");
        assert_eq!(fetched.id, note.id);
        let notes = d.list_file_notes(f1.id).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_text, "Initial note content");

        // 3. Update note
        let updated = d.update_file_note(note.id, "Updated note content").unwrap();
        assert_eq!(updated.note_text, "Updated note content");
        assert_eq!(
            d.get_file_note(note.id).unwrap().unwrap().note_text,
            "Updated note content"
        );

        // 4. Delete note
        d.delete_file_note(note.id).unwrap();
        assert!(d.get_file_note(note.id).unwrap().is_none());
        assert_eq!(d.list_file_notes(f1.id).unwrap().len(), 0);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn file_notes_persist_across_reopen_and_support_multiple_notes() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\Notes", "Notes").unwrap();
        let f1 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Notes\persisted.pdf".into(),
                normalized_path: normalize(r"C:\Notes\persisted.pdf"),
                file_name: "persisted.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 10,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();
        let f2 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Notes\other.pdf".into(),
                normalized_path: normalize(r"C:\Notes\other.pdf"),
                file_name: "other.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 20,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let first = d.create_file_note(f1.id, "First note").unwrap();
        let second = d.create_file_note(f1.id, "Second note").unwrap();
        let other = d.create_file_note(f2.id, "Note on another file").unwrap();
        assert_eq!(first.created_at, first.updated_at);

        // Notes are scoped per file and listed newest first.
        let listed = d.list_file_notes(f1.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[1].id, first.id);
        assert_eq!(d.list_file_notes(f2.id).unwrap().len(), 1);

        let edited = d.update_file_note(first.id, "First note edited").unwrap();
        assert_eq!(edited.note_text, "First note edited");
        assert!(edited.updated_at >= edited.created_at);

        // Re-open the same SQLite file: notes must survive a reload.
        drop(d);
        let reopened = Database::new(&p).unwrap();
        let persisted = reopened.list_file_notes(f1.id).unwrap();
        assert_eq!(persisted.len(), 2);
        assert_eq!(
            persisted
                .iter()
                .find(|n| n.id == first.id)
                .unwrap()
                .note_text,
            "First note edited"
        );
        assert_eq!(reopened.list_file_notes(f2.id).unwrap().len(), 1);

        reopened.delete_file_note(second.id).unwrap();
        drop(reopened);

        let reopened_again = Database::new(&p).unwrap();
        assert_eq!(reopened_again.list_file_notes(f1.id).unwrap().len(), 1);
        assert!(reopened_again.get_file_note(second.id).unwrap().is_none());
        assert!(reopened_again.get_file_note(other.id).unwrap().is_some());

        // Repository-level invalid note IDs are reported safely.
        assert!(reopened_again.get_file_note(999_999).unwrap().is_none());
        assert!(reopened_again.update_file_note(999_999, "x").is_err());
        reopened_again.delete_file_note(999_999).unwrap();

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn combined_indexed_file_queries_search_filters_sorting_pagination() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\Library", "Library").unwrap();

        // Seed 3 files
        let f1 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Library\Annual_Report.pdf".into(),
                normalized_path: normalize(r"C:\Library\Annual_Report.pdf"),
                file_name: "Annual_Report.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: Some("2026-11-01T00:00:00Z".into()),
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let f2 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Library\Tax_Receipt.pdf".into(),
                normalized_path: normalize(r"C:\Library\Tax_Receipt.pdf"),
                file_name: "Tax_Receipt.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 200,
                modified_at: Some("2026-11-02T00:00:00Z".into()),
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let f3 = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\Library\Meeting_Notes.docx".into(),
                normalized_path: normalize(r"C:\Library\Meeting_Notes.docx"),
                file_name: "Meeting_Notes.docx".into(),
                extension: "docx".into(),
                size_bytes: 300,
                modified_at: Some("2026-11-03T00:00:00Z".into()),
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "changed".into(),
            })
            .unwrap();

        // Create collections & tags
        let col_a = d.create_collection("Finance", None, None).unwrap();
        let col_b = d.create_collection("General", None, None).unwrap();
        let tag_x = d.create_tag("urgent", None).unwrap();
        let tag_y = d.create_tag("reviewed", None).unwrap();

        // Link f1 to col_a, tag_x, tag_y, and add note
        d.add_file_to_collection(f1.id, col_a.id).unwrap();
        d.add_file_to_tag(f1.id, tag_x.id).unwrap();
        d.add_file_to_tag(f1.id, tag_y.id).unwrap();
        d.create_file_note(f1.id, "Audit ready").unwrap();

        // Link f2 to col_a, tag_x (no note)
        d.add_file_to_collection(f2.id, col_a.id).unwrap();
        d.add_file_to_tag(f2.id, tag_x.id).unwrap();

        // Link f3 to col_b, tag_y, and add note
        d.add_file_to_collection(f3.id, col_b.id).unwrap();
        d.add_file_to_tag(f3.id, tag_y.id).unwrap();
        d.create_file_note(f3.id, "Draft agenda").unwrap();

        // 1. Filter by collection: col_a -> f1, f2
        let res_col = d
            .query_files(&FileQueryParams {
                collection_id: Some(col_a.id),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_col.total, 2);
        assert!(res_col.files.iter().any(|f| f.id == f1.id));
        assert!(res_col.files.iter().any(|f| f.id == f2.id));

        // 2. Filter by single tag: tag_x -> f1, f2
        let res_tag_x = d
            .query_files(&FileQueryParams {
                tag_ids: Some(vec![tag_x.id]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_tag_x.total, 2);

        // 3. Filter by multiple tags with AND semantics: tag_x AND tag_y -> ONLY f1
        let res_tag_xy = d
            .query_files(&FileQueryParams {
                tag_ids: Some(vec![tag_x.id, tag_y.id]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_tag_xy.total, 1);
        assert_eq!(res_tag_xy.files[0].id, f1.id);

        // 4. Filter by has_notes
        let res_with_notes = d
            .query_files(&FileQueryParams {
                has_notes: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_with_notes.total, 2);
        assert!(res_with_notes.files.iter().any(|f| f.id == f1.id));
        assert!(res_with_notes.files.iter().any(|f| f.id == f3.id));

        let res_without_notes = d
            .query_files(&FileQueryParams {
                has_notes: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_without_notes.total, 1);
        assert_eq!(res_without_notes.files[0].id, f2.id);

        // 5. Combined filters: col_a + tag_x + has_notes: Some(true) -> ONLY f1
        let res_comb = d
            .query_files(&FileQueryParams {
                collection_id: Some(col_a.id),
                tag_ids: Some(vec![tag_x.id]),
                has_notes: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_comb.total, 1);
        assert_eq!(res_comb.files[0].id, f1.id);

        // 6. Phase 1 filters retain behavior: extension and query
        let res_ext = d
            .query_files(&FileQueryParams {
                extension: Some("pdf".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_ext.total, 2);

        let res_q = d
            .query_files(&FileQueryParams {
                query: Some("Annual".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_q.total, 1);
        assert_eq!(res_q.files[0].id, f1.id);

        // 7. Sorting by size DESC -> f3 (300), f2 (200), f1 (100)
        let res_sort = d
            .query_files(&FileQueryParams {
                sort_by: Some("size".into()),
                sort_order: Some("desc".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(res_sort.files.len(), 3);
        assert_eq!(res_sort.files[0].id, f3.id);
        assert_eq!(res_sort.files[1].id, f2.id);
        assert_eq!(res_sort.files[2].id, f1.id);

        // 8. Pagination: page 1 of 2 with page_size = 2
        let p1 = d
            .query_files(&FileQueryParams {
                sort_by: Some("size".into()),
                sort_order: Some("asc".into()),
                page: Some(1),
                page_size: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(p1.total, 3);
        assert_eq!(p1.page, 1);
        assert_eq!(p1.page_size, 2);
        assert_eq!(p1.total_pages, 2);
        assert_eq!(p1.files.len(), 2);
        assert_eq!(p1.files[0].id, f1.id);
        assert_eq!(p1.files[1].id, f2.id);

        let p2 = d
            .query_files(&FileQueryParams {
                sort_by: Some("size".into()),
                sort_order: Some("asc".into()),
                page: Some(2),
                page_size: Some(2),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(p2.files.len(), 1);
        assert_eq!(p2.files[0].id, f3.id);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn scan_style_updates_preserve_organization_metadata() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\ScanSafe", "Scan Safe").unwrap();
        let file = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\ScanSafe\report.pdf".into(),
                normalized_path: normalize(r"C:\ScanSafe\report.pdf"),
                file_name: "report.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let collection = d.create_collection("Safe Col", None, None).unwrap();
        let tag = d.create_tag("safe-tag", None).unwrap();
        d.add_file_to_collection(file.id, collection.id).unwrap();
        d.add_file_to_tag(file.id, tag.id).unwrap();
        let note = d.create_file_note(file.id, "Keep this note").unwrap();

        // The operations used by the scanner and watcher only update the file row.
        d.touch_indexed_file(file.id, "changed").unwrap();
        d.update_indexed_file(
            file.id,
            &NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\ScanSafe\report.pdf".into(),
                normalized_path: normalize(r"C:\ScanSafe\report.pdf"),
                file_name: "report.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 120,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            },
        )
        .unwrap();
        d.move_indexed_file(
            file.id,
            &NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: r"C:\ScanSafe\renamed.pdf".into(),
                normalized_path: normalize(r"C:\ScanSafe\renamed.pdf"),
                file_name: "renamed.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 120,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "moved".into(),
            },
        )
        .unwrap();
        d.mark_file_missing(file.id).unwrap();

        let updated = d.get_indexed_file(file.id).unwrap().unwrap();
        assert_eq!(updated.file_name, "renamed.pdf");
        assert_eq!(updated.status, "missing");
        assert_eq!(d.list_file_collections(file.id).unwrap().len(), 1);
        assert_eq!(d.list_file_tags(file.id).unwrap().len(), 1);
        assert_eq!(
            d.get_file_note(note.id).unwrap().unwrap().note_text,
            "Keep this note"
        );

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn export_library_contains_phase2_metadata_and_no_document_contents() {
        let (d, p) = db();
        let dir = std::env::temp_dir().join(format!(
            "libone-export-src-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let file_path = dir.join("export_target.txt");
        let document_body = "UNIQUE_DOCUMENT_BODY_SHOULD_NOT_APPEAR_IN_EXPORT_9f3a";
        std::fs::write(&file_path, document_body).unwrap();

        let root = d
            .add_scan_root(dir.to_str().unwrap(), "Export Root")
            .unwrap();
        let file = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: root.id,
                absolute_path: file_path.to_str().unwrap().into(),
                normalized_path: normalize(file_path.to_str().unwrap()),
                file_name: "export_target.txt".into(),
                extension: "txt".into(),
                size_bytes: document_body.len() as i64,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let collection = d
            .create_collection("Export Collection", None, None)
            .unwrap();
        let tag = d.create_tag("export-tag", None).unwrap();
        d.add_file_to_collection(file.id, collection.id).unwrap();
        d.add_file_to_tag(file.id, tag.id).unwrap();
        let note = d.create_file_note(file.id, "my own private note").unwrap();

        let export = d.export_library().unwrap();
        assert_eq!(export.format_version, EXPORT_FORMAT_VERSION);
        assert_eq!(export.collections.len(), 1);
        assert_eq!(export.tags.len(), 1);
        assert_eq!(export.file_collections.len(), 1);
        assert_eq!(export.file_collections[0].file_id, file.id);
        assert_eq!(export.file_collections[0].collection_id, collection.id);
        assert_eq!(export.file_tags.len(), 1);
        assert_eq!(export.file_tags[0].tag_id, tag.id);
        assert_eq!(export.file_notes.len(), 1);
        assert_eq!(export.file_notes[0].id, note.id);
        assert_eq!(export.file_notes[0].note_text, "my own private note");

        let json = serde_json::to_string(&export).unwrap();
        assert!(
            !json.contains(document_body),
            "export must never include document contents"
        );
        assert!(
            json.contains("my own private note"),
            "user-authored notes are metadata and should be exported"
        );

        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn query_files_combines_all_filters_with_tag_and_and_pagination() {
        let (d, p) = db();
        let r = d.add_scan_root(r"C:\Combined", "Combined").unwrap();

        let insert = |name: &str, ext: &str, status: &str| {
            let path = format!(r"C:\Combined\{name}");
            d.insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                normalized_path: normalize(&path),
                absolute_path: path,
                file_name: name.into(),
                extension: ext.into(),
                size_bytes: 10,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: status.into(),
            })
            .unwrap()
        };

        let f1 = insert("report_final.pdf", "pdf", "indexed");
        let f2 = insert("other.pdf", "pdf", "indexed");
        let f3 = insert("report_draft.txt", "txt", "indexed");

        let col = d.create_collection("Reports", None, None).unwrap();
        let tag_reviewed = d.create_tag("reviewed", None).unwrap();
        let tag_important = d.create_tag("important", None).unwrap();

        d.add_file_to_collection(f1.id, col.id).unwrap();
        d.add_file_to_collection(f2.id, col.id).unwrap();
        d.add_file_to_tag(f1.id, tag_reviewed.id).unwrap();
        d.add_file_to_tag(f1.id, tag_important.id).unwrap();
        d.add_file_to_tag(f2.id, tag_reviewed.id).unwrap();
        d.add_file_to_tag(f3.id, tag_reviewed.id).unwrap();
        d.add_file_to_tag(f3.id, tag_important.id).unwrap();

        // AND semantics: both tags are required.
        let both_tags = d
            .query_files(&FileQueryParams {
                tag_ids: Some(vec![tag_reviewed.id, tag_important.id]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(both_tags.total, 2);
        assert_eq!(both_tags.files.len(), 2);

        // Every Phase 1 + Phase 2 filter together, bounded to one result page.
        let combined = d
            .query_files(&FileQueryParams {
                query: Some("report".into()),
                root_id: Some(r.id),
                extension: Some("pdf".into()),
                status: Some("indexed".into()),
                collection_id: Some(col.id),
                tag_ids: Some(vec![tag_reviewed.id, tag_important.id]),
                sort_by: Some("name".into()),
                sort_order: Some("asc".into()),
                page: Some(1),
                page_size: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(combined.total, 1);
        assert_eq!(combined.files.len(), 1);
        assert_eq!(combined.files[0].id, f1.id);
        assert_eq!(combined.total_pages, 1);

        // Pagination stays parameterized and bounded; an oversized page is clamped.
        let page_two = d
            .query_files(&FileQueryParams {
                tag_ids: Some(vec![tag_reviewed.id, tag_important.id]),
                page: Some(2),
                page_size: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page_two.files.len(), 1);
        assert_eq!(page_two.files[0].id, f1.id);

        let oversized = d
            .query_files(&FileQueryParams {
                query: Some("x".repeat(MAX_QUERY_CHARS + 500)),
                page: Some(1),
                page_size: Some(500),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(oversized.page_size, 200, "page_size must stay bounded");
        assert_eq!(oversized.total, 0);

        // Repeated tag IDs are deduplicated before reaching SQLite.
        let deduped = d
            .query_files(&FileQueryParams {
                tag_ids: Some(vec![tag_reviewed.id; 100]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(deduped.total, 3);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_backup_valid_creates_atomic_sqlite_backup() {
        let (d, p) = db();
        let root_dir = p.join("test_root");
        std::fs::create_dir_all(&root_dir).unwrap();
        let r = d
            .add_scan_root(root_dir.to_str().unwrap(), "Test Root")
            .unwrap();

        let doc_path = root_dir.join("sample.pdf");
        std::fs::write(&doc_path, b"dummy pdf content").unwrap();

        let f = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: doc_path.to_str().unwrap().into(),
                normalized_path: normalize(doc_path.to_str().unwrap()),
                file_name: "sample.pdf".into(),
                extension: "pdf".into(),
                size_bytes: 17,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let col = d.create_collection("Research", None, None).unwrap();
        d.add_file_to_collection(f.id, col.id).unwrap();

        let tag = d.create_tag("Priority", None).unwrap();
        d.add_file_to_tag(f.id, tag.id).unwrap();

        d.create_file_note(f.id, "Important test note").unwrap();

        let backup_dir = p.join("backups");
        std::fs::create_dir_all(&backup_dir).unwrap();
        let backup_file = backup_dir.join("libone_test_backup.db");

        // Execute backup
        let backup_res = d.create_backup(&backup_file).unwrap();
        assert_eq!(
            backup_res.backup_path,
            backup_file.to_string_lossy().to_string()
        );
        assert!(backup_res.file_size_bytes > 0);
        assert_eq!(backup_res.summary.total_files, 1);
        assert_eq!(backup_res.summary.total_collections, 1);
        assert_eq!(backup_res.summary.total_tags, 1);
        assert_eq!(backup_res.summary.total_notes, 1);
        assert_eq!(backup_res.summary.total_roots, 1);

        // Verify validation succeeds
        let validation = Database::validate_backup_file(&backup_file).unwrap();
        assert!(validation.is_valid);
        assert_eq!(validation.schema_version, 4);
        assert_eq!(validation.summary.total_files, 1);
        assert_eq!(validation.summary.total_collections, 1);
        assert_eq!(validation.summary.total_tags, 1);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_backup_corrupt_rejected_by_validation() {
        let (_d, p) = db();
        let corrupt_path = p.join("corrupt_backup.db");
        std::fs::write(
            &corrupt_path,
            b"NOT A VALID SQLITE DATABASE FILE HEADER JUNK BYTES",
        )
        .unwrap();

        let val = Database::validate_backup_file(&corrupt_path);
        assert!(
            val.is_err(),
            "Corrupt backup must be rejected by validation"
        );

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_backup_incompatible_version_rejected() {
        let (_d, p) = db();
        let future_backup_path = p.join("future_schema_backup.db");

        // Create a valid SQLite db with incompatible schema version 999
        {
            let conn = Connection::open(&future_backup_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE library_metadata (
                    id INTEGER PRIMARY KEY,
                    schema_version INTEGER NOT NULL,
                    created_at TEXT NOT NULL
                );
                INSERT INTO library_metadata (id, schema_version, created_at) VALUES (1, 999, '2026-11-24');
                CREATE TABLE scan_roots (id INTEGER PRIMARY KEY);
                CREATE TABLE indexed_files (id INTEGER PRIMARY KEY);
                CREATE TABLE collections (id INTEGER PRIMARY KEY);
                CREATE TABLE tags (id INTEGER PRIMARY KEY);
                CREATE TABLE file_collections (file_id INTEGER, collection_id INTEGER);
                CREATE TABLE file_tags (file_id INTEGER, tag_id INTEGER);
                CREATE TABLE file_notes (file_id INTEGER PRIMARY KEY);
                "
            ).unwrap();
        }

        let val = Database::validate_backup_file(&future_backup_path);
        assert!(val.is_err(), "Incompatible backup version must be rejected");
        let reason = val.unwrap_err().to_string();
        assert!(
            reason.contains("Incompatible") || reason.contains("999"),
            "Expected incompatible schema version message, got: {reason}"
        );

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_backup_interrupted_write_cleans_up_and_fails_safely() {
        let (d, p) = db();
        // Point to an invalid target directory where parent cannot be created
        let bad_target = PathBuf::from("Z:\\non_existent_drive_libone\\backup.db");

        let res = d.create_backup(&bad_target);
        assert!(res.is_err(), "Expected backup to fail on non-existent path");
        assert!(!bad_target.exists());

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_restore_valid_backup_and_metadata_only_guarantee() {
        let (d1, p1) = db();
        let (d2, p2) = db();

        // 1. Create a root and physical file
        let doc_dir = p1.join("user_documents");
        std::fs::create_dir_all(&doc_dir).unwrap();
        let doc_path = doc_dir.join("financial_report.pdf");
        let original_doc_bytes = b"SECRET USER REPORT CONTENT - NEVER COPY TO DB OR MODIFY";
        std::fs::write(&doc_path, original_doc_bytes).unwrap();
        let original_mtime = std::fs::metadata(&doc_path).unwrap().modified().unwrap();

        // Register in DB 1
        let r1 = d1
            .add_scan_root(doc_dir.to_str().unwrap(), "User Docs")
            .unwrap();
        let f1 = d1
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r1.id,
                absolute_path: doc_path.to_str().unwrap().into(),
                normalized_path: normalize(doc_path.to_str().unwrap()),
                file_name: "financial_report.pdf".into(),
                extension: "pdf".into(),
                size_bytes: original_doc_bytes.len() as i64,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "indexed".into(),
            })
            .unwrap();

        let col1 = d1.create_collection("Finance", None, None).unwrap();
        d1.add_file_to_collection(f1.id, col1.id).unwrap();

        let tag1 = d1.create_tag("Confidential", None).unwrap();
        d1.add_file_to_tag(f1.id, tag1.id).unwrap();

        d1.create_file_note(f1.id, "Financial note 2026").unwrap();

        // 2. Create backup of DB 1
        let backup_file = p1.join("db1_backup.db");
        d1.create_backup(&backup_file).unwrap();

        // 3. Restore into DB 2
        let restore_res = d2.restore_from_backup(&backup_file).unwrap();
        assert_eq!(restore_res.summary.total_files, 1);
        assert_eq!(restore_res.summary.total_collections, 1);
        assert_eq!(restore_res.summary.total_tags, 1);
        assert_eq!(restore_res.summary.total_notes, 1);

        // 4. Verify DB 2 now has the restored metadata
        let cols = d2.list_collections().unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].name, "Finance");

        let tags = d2.list_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "Confidential");

        let detail = d2.get_file_detail(f1.id).unwrap().unwrap();
        assert_eq!(detail.file.file_name, "financial_report.pdf");
        let notes = d2.list_file_notes(f1.id).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_text, "Financial note 2026");

        // 5. Verify METADATA-ONLY guarantee:
        // Physical user file was never modified or touched
        let current_bytes = std::fs::read(&doc_path).unwrap();
        assert_eq!(current_bytes, original_doc_bytes);
        let current_mtime = std::fs::metadata(&doc_path).unwrap().modified().unwrap();
        assert_eq!(current_mtime, original_mtime);

        // Verify SQLite database stores no content blobs
        let conn2 = d2.open().unwrap();
        let column_names: Vec<String> = conn2
            .prepare("PRAGMA table_info(indexed_files)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!column_names.contains(&"content".to_string()));
        assert!(!column_names.contains(&"data".to_string()));
        assert!(!column_names.contains(&"body".to_string()));
        drop(conn2);

        std::fs::remove_dir_all(p1).unwrap();
        std::fs::remove_dir_all(p2).unwrap();
    }

    #[test]
    fn test_restore_rollback_on_failure_preserves_original_database() {
        let (d, p) = db();
        let root_dir = p.join("keep_root");
        std::fs::create_dir_all(&root_dir).unwrap();
        let r = d
            .add_scan_root(root_dir.to_str().unwrap(), "Keep Root")
            .unwrap();

        // Attempt restore from an invalid/corrupt file
        let corrupt_file = p.join("corrupt.db");
        std::fs::write(&corrupt_file, b"corrupted header").unwrap();

        let restore_res = d.restore_from_backup(&corrupt_file);
        assert!(restore_res.is_err(), "Corrupt backup restore must fail");

        // Verify original database is completely intact and accessible
        let roots = d.list_scan_roots().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, r.id);

        std::fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn test_integrity_check_detects_issues_without_deleting_records() {
        let (d, p) = db();
        let root_dir = p.join("integ_root");
        std::fs::create_dir_all(&root_dir).unwrap();
        let r = d
            .add_scan_root(root_dir.to_str().unwrap(), "Integ Root")
            .unwrap();

        // Register a file whose path does not exist on disk
        let missing_path = root_dir.join("missing_doc.txt");
        let f = d
            .insert_indexed_file(&NewIndexedFile {
                scan_root_id: r.id,
                absolute_path: missing_path.to_str().unwrap().into(),
                normalized_path: normalize(missing_path.to_str().unwrap()),
                file_name: "missing_doc.txt".into(),
                extension: "txt".into(),
                size_bytes: 100,
                modified_at: None,
                created_at_if_available: None,
                content_hash_nullable: None,
                file_id_nullable: None,
                status: "missing".into(),
            })
            .unwrap();

        // Insert foreign key violation with PRAGMA foreign_keys = OFF
        {
            let conn = d.open().unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys = OFF;
                 INSERT INTO file_tags (file_id, tag_id, created_at) VALUES (999999, 888888, datetime('now'));
                 PRAGMA foreign_keys = ON;",
            )
            .unwrap();
        }

        // Run integrity check
        let report = d.check_integrity().unwrap();
        assert!(!report.is_healthy, "Expected database to report issues");
        assert_eq!(report.missing_files_count, 1);
        assert_eq!(report.foreign_key_violations.len(), 2);
        assert_eq!(report.foreign_key_violations[0].table_name, "file_tags");

        // Verify actionable repair guidance is provided
        assert!(!report.issues.is_empty());
        let has_missing_file_issue = report.issues.iter().any(|i| i.category == "missing_files");
        let has_fk_issue = report.issues.iter().any(|i| i.category == "foreign_key");
        assert!(
            has_missing_file_issue,
            "Expected missing file issue in report"
        );
        assert!(has_fk_issue, "Expected FK violation issue in report");

        // Ensure records are NOT deleted
        let conn = d.open().unwrap();
        let fk_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_tags WHERE file_id = 999999",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fk_count, 1, "Foreign key violation row must not be deleted");

        let file_exists = d.get_file_detail(f.id).unwrap().is_some();
        assert!(file_exists, "Missing file record must not be deleted");
        drop(conn);

        std::fs::remove_dir_all(p).unwrap();
    }
}
