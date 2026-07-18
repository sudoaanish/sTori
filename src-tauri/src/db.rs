use crate::{
    error::{AppError, Result},
    models::*,
};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::HashSet;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const LATEST_SCHEMA_VERSION: i64 = 5;

#[derive(Clone)]
pub struct Database(pub Arc<Mutex<Connection>>, Arc<PathBuf>);

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existed = path.exists() && std::fs::metadata(path)?.len() > 0;
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        verify_integrity(&conn)?;
        migrate(&mut conn, path, existed)?;
        conn.execute("UPDATE download_jobs SET status='paused',message='Interrupted when sTori closed. Resume when ready.' WHERE status IN ('queued','downloading','verifying','importing','indexing')", [])?;
        Ok(Self(
            Arc::new(Mutex::new(conn)),
            Arc::new(path.to_path_buf()),
        ))
    }

    pub fn libraries(&self) -> Result<Vec<LibraryDto>> {
        let conn = self.0.lock();
        let mut stmt=conn.prepare("SELECT l.id,l.name,l.path,l.last_scanned_at,(SELECT COUNT(*) FROM books b WHERE b.library_id=l.id),l.id=CAST(COALESCE((SELECT value FROM app_settings WHERE key='managed_download_library_id'),'0') AS INTEGER) FROM libraries l ORDER BY l.name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(LibraryDto {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: display_path(r.get(2)?),
                    last_scanned_at: r.get(3)?,
                    book_count: r.get(4)?,
                    managed: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn add_library(&self, name: &str, path: &str) -> Result<LibraryDto> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| AppError::BadRequest(format!("Library folder does not exist: {path}")))?;
        if !canonical.is_dir() {
            return Err(AppError::BadRequest("A library must be a directory".into()));
        }
        let path = canonical.to_string_lossy().to_string();
        let conn = self.0.lock();
        conn.execute("INSERT INTO libraries(name,path) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET name=excluded.name",params![name.trim(),path])?;
        let id: i64 = conn.query_row("SELECT id FROM libraries WHERE path=?1", [&path], |r| {
            r.get(0)
        })?;
        drop(conn);
        self.libraries()?
            .into_iter()
            .find(|x| x.id == id)
            .ok_or(AppError::NotFound)
    }

    pub fn ensure_managed_library(&self, path: &Path) -> Result<LibraryDto> {
        std::fs::create_dir_all(path)?;
        let library = self.add_library("sTori Books", &path.to_string_lossy())?;
        self.set_setting("managed_download_library_id", &library.id.to_string())?;
        self.libraries()?
            .into_iter()
            .find(|row| row.id == library.id)
            .ok_or(AppError::NotFound)
    }

    pub fn rename_library(&self, id: i64, name: &str) -> Result<LibraryDto> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest("A library needs a name".into()));
        }
        let changed = self.0.lock().execute(
            "UPDATE libraries SET name=?1 WHERE id=?2",
            params![name, id],
        )?;
        if changed == 0 {
            return Err(AppError::NotFound);
        }
        self.libraries()?
            .into_iter()
            .find(|library| library.id == id)
            .ok_or(AppError::NotFound)
    }

    pub fn remove_library(&self, id: i64) -> Result<()> {
        let managed_library_id = id.to_string();
        if self.setting("managed_download_library_id")?.as_deref()
            == Some(managed_library_id.as_str())
        {
            return Err(AppError::BadRequest(
                "The managed sTori Books library cannot be removed.".into(),
            ));
        }
        let changed = self
            .0
            .lock()
            .execute("DELETE FROM libraries WHERE id=?1", [id])?;
        if changed == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().execute("INSERT INTO app_settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
        Ok(())
    }

    pub fn library_path(&self, id: i64) -> Result<String> {
        self.0
            .lock()
            .query_row("SELECT path FROM libraries WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or(AppError::NotFound)
    }

    pub fn store_scan(&self, library_id: i64, books: &[ScannedBook]) -> Result<()> {
        self.store_scan_mode(library_id, books, true)
    }

    pub fn store_incremental(&self, library_id: i64, books: &[ScannedBook]) -> Result<()> {
        self.store_scan_mode(library_id, books, false)
    }

    fn store_scan_mode(
        &self,
        library_id: i64,
        books: &[ScannedBook],
        prune_missing: bool,
    ) -> Result<()> {
        let mut conn = self.0.lock();
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        let seen: HashSet<&str> = books.iter().map(|b| b.directory_path.as_str()).collect();
        let existing = {
            let mut stmt = tx.prepare("SELECT id,directory_path FROM books WHERE library_id=?1")?;
            let rows = stmt
                .query_map([library_id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if prune_missing {
            for (id, path) in existing {
                if !seen.contains(path.as_str()) {
                    tx.execute("DELETE FROM books WHERE id=?1", [id])?;
                }
            }
        }
        for book in books {
            let series_id = if let Some(name) = book.series_name.as_deref() {
                tx.execute("INSERT INTO series(name,description,updated_at) VALUES(?1,NULL,?2) ON CONFLICT(name) DO UPDATE SET updated_at=excluded.updated_at",params![name,now])?;
                Some(
                    tx.query_row("SELECT id FROM series WHERE name=?1", [name], |r| {
                        r.get::<_, i64>(0)
                    })?,
                )
            } else {
                None
            };
            tx.execute("INSERT INTO books(library_id,directory_path,title,subtitle,authors_json,description,published,tags_json,identifier,format,file_path,file_name,cover_path,series_id,series_index,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16) ON CONFLICT(library_id,directory_path) DO UPDATE SET title=excluded.title,subtitle=excluded.subtitle,authors_json=excluded.authors_json,description=excluded.description,published=excluded.published,tags_json=excluded.tags_json,identifier=excluded.identifier,format=excluded.format,file_path=excluded.file_path,file_name=excluded.file_name,cover_path=excluded.cover_path,series_id=excluded.series_id,series_index=excluded.series_index,updated_at=excluded.updated_at",params![library_id,book.directory_path,book.title,book.subtitle,serde_json::to_string(&book.authors).unwrap(),book.description,book.published,serde_json::to_string(&book.tags).unwrap(),book.identifier,book.format,book.file_path,book.file_name,book.cover_path,series_id,book.series_index,now])?;
            let book_id = tx.query_row(
                "SELECT id FROM books WHERE library_id=?1 AND directory_path=?2",
                params![library_id, book.directory_path],
                |r| r.get::<_, i64>(0),
            )?;
            tx.execute("DELETE FROM book_files WHERE book_id=?1", [book_id])?;
            tx.execute(
                "INSERT INTO book_files(book_id,format,path) VALUES(?1,?2,?3)",
                params![book_id, book.format, book.file_path],
            )?;
            for (format, path) in &book.alternate_files {
                tx.execute(
                    "INSERT OR IGNORE INTO book_files(book_id,format,path) VALUES(?1,?2,?3)",
                    params![book_id, format, path],
                )?;
            }
        }
        tx.execute(
            "UPDATE libraries SET last_scanned_at=?1 WHERE id=?2",
            params![now, library_id],
        )?;
        tx.execute("DELETE FROM books_fts", [])?;
        tx.execute("INSERT INTO books_fts(rowid,title,authors,tags,series) SELECT b.id,b.title,b.authors_json,b.tags_json,COALESCE(s.name,'') FROM books b LEFT JOIN series s ON s.id=b.series_id",[])?;
        tx.commit()?;
        Ok(())
    }

    pub fn books(&self, query: &str, status: Option<&str>) -> Result<Vec<BookDto>> {
        let conn = self.0.lock();
        let sql = if query.trim().is_empty() {
            "SELECT_BOOKS WHERE (?1='' OR (?1='reading' AND COALESCE(r.progress,0)>0 AND COALESCE(r.progress,0)<0.995)) ORDER BY b.title COLLATE NOCASE".replace("SELECT_BOOKS",SELECT_BOOKS)
        } else {
            "SELECT_BOOKS JOIN books_fts f ON f.rowid=b.id WHERE books_fts MATCH ?1 ORDER BY rank"
                .replace("SELECT_BOOKS", SELECT_BOOKS)
        };
        let value = if query.trim().is_empty() {
            status.unwrap_or("").to_string()
        } else {
            format!("{}*", query.trim().replace('"', ""))
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map([value], book_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn book(&self, id: i64) -> Result<BookDto> {
        self.0
            .lock()
            .query_row(
                &format!("{SELECT_BOOKS} WHERE b.id=?1"),
                [id],
                book_from_row,
            )
            .optional()?
            .ok_or(AppError::NotFound)
    }
    pub fn book_paths(&self, id: i64) -> Result<(String, Option<String>, String)> {
        self.0
            .lock()
            .query_row(
                "SELECT file_path,cover_path,format FROM books WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or(AppError::NotFound)
    }

    pub fn series(&self) -> Result<Vec<GroupDto>> {
        self.groups(
            "SELECT id,name,description,updated_at FROM series ORDER BY name",
            None,
        )
    }
    pub fn series_by_id(&self, id: i64) -> Result<GroupDto> {
        self.groups(
            "SELECT id,name,description,updated_at FROM series WHERE id=?1",
            Some(id),
        )?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound)
    }
    pub fn collections(&self) -> Result<Vec<GroupDto>> {
        self.groups(
            "SELECT id,name,description,updated_at FROM collections ORDER BY updated_at DESC",
            None,
        )
    }
    pub fn collection(&self, id: i64) -> Result<GroupDto> {
        self.groups(
            "SELECT id,name,description,updated_at FROM collections WHERE id=?1",
            Some(id),
        )?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound)
    }
    fn groups(&self, sql: &str, id: Option<i64>) -> Result<Vec<GroupDto>> {
        let conn = self.0.lock();
        let mut stmt = conn.prepare(sql)?;
        let rows = if let Some(id) = id {
            stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<(i64, String, Option<String>, Option<String>)>>>()?
        } else {
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<(i64, String, Option<String>, Option<String>)>>>()?
        };
        drop(stmt);
        let mut out = Vec::new();
        for (gid, name, desc, updated) in rows {
            let book_sql = if sql.contains("series") {
                format!("{SELECT_BOOKS} WHERE b.series_id=?1 ORDER BY COALESCE(b.series_index,9999),b.title")
            } else {
                format!("{SELECT_BOOKS} JOIN collection_books cb ON cb.book_id=b.id WHERE cb.collection_id=?1 ORDER BY cb.position")
            };
            let mut bs = conn.prepare(&book_sql)?;
            let books = bs
                .query_map([gid], book_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.push(GroupDto {
                id: gid,
                name,
                description: desc,
                books,
                updated_at: updated,
            });
        }
        Ok(out)
    }

    pub fn save_collection(&self, id: Option<i64>, req: &CollectionRequest) -> Result<GroupDto> {
        if req.name.trim().is_empty() {
            return Err(AppError::BadRequest("Collection name is required".into()));
        }
        let mut conn = self.0.lock();
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        let id = if let Some(id) = id {
            tx.execute(
                "UPDATE collections SET name=?1,description=?2,updated_at=?3 WHERE id=?4",
                params![req.name.trim(), req.description, now, id],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO collections(name,description,updated_at) VALUES(?1,?2,?3)",
                params![req.name.trim(), req.description, now],
            )?;
            tx.last_insert_rowid()
        };
        tx.execute("DELETE FROM collection_books WHERE collection_id=?1", [id])?;
        for (pos, book_id) in req.book_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO collection_books(collection_id,book_id,position) VALUES(?1,?2,?3)",
                params![id, book_id, pos as i64],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.collection(id)
    }

    pub fn reading_state(&self, book_id: i64) -> Result<Option<ReadingStateDto>> {
        Ok(self
            .0
            .lock()
            .query_row(
                "SELECT book_id,locator,progress,updated_at FROM reading_state WHERE book_id=?1",
                [book_id],
                |r| {
                    Ok(ReadingStateDto {
                        book_id: r.get(0)?,
                        locator: r.get(1)?,
                        progress: r.get(2)?,
                        updated_at: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }
    pub fn save_progress(&self, book_id: i64, req: &ProgressRequest) -> Result<ReadingStateDto> {
        let progress = req.progress.clamp(0.0, 1.0);
        let now = chrono::Utc::now().to_rfc3339();
        self.0.lock().execute("INSERT INTO reading_state(book_id,locator,progress,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(book_id) DO UPDATE SET locator=excluded.locator,progress=excluded.progress,updated_at=excluded.updated_at",params![book_id,req.locator,progress,now])?;
        self.reading_state(book_id)?.ok_or(AppError::NotFound)
    }
    pub fn annotations(&self, book_id: i64) -> Result<Vec<AnnotationDto>> {
        let conn = self.0.lock();
        let mut stmt=conn.prepare("SELECT id,book_id,kind,locator,text,note,created_at FROM annotations WHERE book_id=?1 ORDER BY created_at DESC")?;
        let rows = stmt
            .query_map([book_id], |r| {
                Ok(AnnotationDto {
                    id: r.get(0)?,
                    book_id: r.get(1)?,
                    kind: r.get(2)?,
                    locator: r.get(3)?,
                    text: r.get(4)?,
                    note: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn add_annotation(&self, book_id: i64, req: &AnnotationRequest) -> Result<AnnotationDto> {
        if !["bookmark", "highlight", "note"].contains(&req.kind.as_str()) {
            return Err(AppError::BadRequest("Invalid annotation kind".into()));
        }
        // Installed PWAs from before the dedicated bookmark endpoint still use this route.
        // Keep them compatible with the bookmark uniqueness guarantee instead of leaking a SQLite 500.
        if req.kind == "bookmark" {
            return self.add_bookmark(book_id, &req.locator, req.text.clone());
        }
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.0.lock();
        conn.execute("INSERT INTO annotations(book_id,kind,locator,text,note,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![book_id,req.kind,req.locator,req.text,req.note,now])?;
        let id = conn.last_insert_rowid();
        Ok(AnnotationDto {
            id,
            book_id,
            kind: req.kind.clone(),
            locator: req.locator.clone(),
            text: req.text.clone(),
            note: req.note.clone(),
            created_at: now,
        })
    }
    pub fn bookmarks(&self, book_id: i64) -> Result<Vec<AnnotationDto>> {
        self.book(book_id)?;
        let conn = self.0.lock();
        let mut stmt = conn.prepare("SELECT id,book_id,kind,locator,text,note,created_at FROM annotations WHERE book_id=?1 AND kind='bookmark' ORDER BY created_at DESC")?;
        let rows = stmt.query_map([book_id], |r| Ok(AnnotationDto { id: r.get(0)?, book_id: r.get(1)?, kind: r.get(2)?, locator: r.get(3)?, text: r.get(4)?, note: r.get(5)?, created_at: r.get(6)? }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn add_bookmark(&self, book_id: i64, locator: &str, text: Option<String>) -> Result<AnnotationDto> {
        let locator = locator.trim();
        if locator.is_empty() { return Err(AppError::BadRequest("A bookmark location is required".into())); }
        self.book(book_id)?;
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.0.lock();
        let existing: Option<i64> = conn.query_row("SELECT id FROM annotations WHERE book_id=?1 AND kind='bookmark' AND locator=?2", params![book_id, locator], |r| r.get(0)).optional()?;
        if let Some(id) = existing {
            if text.is_some() { conn.execute("UPDATE annotations SET text=?1 WHERE id=?2", params![text, id])?; }
        } else {
            conn.execute("INSERT INTO annotations(book_id,kind,locator,text,note,created_at) VALUES(?1,'bookmark',?2,?3,NULL,?4)", params![book_id, locator, text, now])?;
        }
        conn.query_row("SELECT id,book_id,kind,locator,text,note,created_at FROM annotations WHERE book_id=?1 AND kind='bookmark' AND locator=?2", params![book_id, locator], |r| Ok(AnnotationDto { id: r.get(0)?, book_id: r.get(1)?, kind: r.get(2)?, locator: r.get(3)?, text: r.get(4)?, note: r.get(5)?, created_at: r.get(6)? })).map_err(Into::into)
    }
    pub fn delete_bookmark(&self, book_id: i64, bookmark_id: i64) -> Result<()> {
        self.book(book_id)?;
        let changed = self.0.lock().execute("DELETE FROM annotations WHERE id=?1 AND book_id=?2 AND kind='bookmark'", params![bookmark_id, book_id])?;
        if changed == 0 { return Err(AppError::NotFound); }
        Ok(())
    }
    pub fn save_reader_token(
        &self,
        token: &str,
        name: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<PairedDeviceDto> {
        let now = chrono::Utc::now().to_rfc3339();
        let device_id = uuid::Uuid::new_v4().to_string();
        self.0.lock().execute(
            "INSERT INTO reader_tokens(token,created_at,device_id,name,last_seen_at,last_ip,user_agent,revoked_at) VALUES(?1,?2,?3,?4,?2,?5,?6,NULL)",
            params![token, now, device_id, name, ip, user_agent],
        )?;
        self.paired_devices()?
            .into_iter()
            .find(|device| device.id == device_id)
            .ok_or(AppError::NotFound)
    }
    pub fn paired_devices(&self) -> Result<Vec<PairedDeviceDto>> {
        let conn = self.0.lock();
        let mut statement = conn.prepare("SELECT device_id,name,created_at,last_seen_at,last_ip,user_agent FROM reader_tokens WHERE revoked_at IS NULL ORDER BY last_seen_at DESC")?;
        let rows = statement
            .query_map([], |row| {
                Ok(PairedDeviceDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    last_seen_at: row.get(3)?,
                    last_ip: row.get(4)?,
                    user_agent: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn revoke_device(&self, id: &str) -> Result<bool> {
        Ok(self.0.lock().execute(
            "UPDATE reader_tokens SET revoked_at=?2 WHERE device_id=?1 AND revoked_at IS NULL",
            params![id, chrono::Utc::now().to_rfc3339()],
        )? > 0)
    }
    pub fn revoke_all_devices(&self) -> Result<usize> {
        Ok(self.0.lock().execute(
            "UPDATE reader_tokens SET revoked_at=?1 WHERE revoked_at IS NULL",
            [chrono::Utc::now().to_rfc3339()],
        )?)
    }
    pub fn validate_and_touch_reader_token(&self, token: &str, ip: Option<&str>) -> bool {
        let conn = self.0.lock();
        let last_seen = conn
            .query_row(
                "SELECT last_seen_at FROM reader_tokens WHERE token=?1 AND revoked_at IS NULL",
                [token],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten();
        let Some(last_seen) = last_seen else {
            return false;
        };
        let stale = chrono::DateTime::parse_from_rfc3339(&last_seen)
            .map(|value| {
                chrono::Utc::now()
                    .signed_duration_since(value.with_timezone(&chrono::Utc))
                    .num_minutes()
                    >= 5
            })
            .unwrap_or(true);
        if stale {
            let _ = conn.execute(
                "UPDATE reader_tokens SET last_seen_at=?2,last_ip=COALESCE(?3,last_ip) WHERE token=?1 AND revoked_at IS NULL",
                params![token, chrono::Utc::now().to_rfc3339(), ip],
            );
        }
        true
    }
    pub fn database_diagnostics(&self, managed_library: &Path) -> Result<DiagnosticsDto> {
        let conn = self.0.lock();
        let database_status: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        drop(conn);
        let libraries = self
            .libraries()?
            .into_iter()
            .map(|library| LibraryDiagnosticDto {
                id: library.id,
                name: library.name,
                path: library.path.clone(),
                available: Path::new(&library.path).is_dir(),
                book_count: library.book_count,
            })
            .collect();
        Ok(DiagnosticsDto {
            database_status,
            schema_version,
            database_path: display_path(self.1.to_string_lossy().to_string()),
            backup_directory: display_path(self.backup_directory().to_string_lossy().to_string()),
            managed_library_free_bytes: fs2::available_space(managed_library).ok(),
            firewall_rule_detected: firewall_rule_detected(),
            firewall_guidance: firewall_guidance(),
            libraries,
        })
    }
    pub fn backup_directory(&self) -> PathBuf {
        self.1
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("backups")
    }
    pub fn create_manual_backup(&self) -> Result<BackupDto> {
        let conn = self.0.lock();
        conn.execute_batch("PRAGMA wal_checkpoint(FULL)")?;
        let directory = self.backup_directory();
        std::fs::create_dir_all(&directory)?;
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let destination = directory.join(format!("stori-manual-{stamp}.db"));
        std::fs::copy(self.1.as_ref(), &destination)?;
        drop(conn);
        prune_backups(&directory, "stori-manual-", 10)?;
        backup_dto(&destination)
    }
    pub fn backups(&self) -> Result<Vec<BackupDto>> {
        let directory = self.backup_directory();
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut backups = std::fs::read_dir(directory)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("db"))
            .filter_map(|path| backup_dto(&path).ok())
            .collect::<Vec<_>>();
        backups.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(backups)
    }

    pub fn create_download_job(
        &self,
        id: &str,
        library_id: i64,
        book: &CatalogBookDto,
    ) -> Result<DownloadJobDto> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.0.lock();
        let duplicate: Option<String> = conn.query_row("SELECT id FROM download_jobs WHERE library_id=?1 AND provider=?2 AND provider_id=?3", params![library_id,book.provider,book.provider_id], |r| r.get(0)).optional()?;
        if duplicate.is_some() {
            return Err(AppError::BadRequest(
                "This edition is already in the download queue".into(),
            ));
        }
        conn.execute("INSERT INTO download_jobs(id,library_id,provider,provider_id,title,authors_json,book_json,status,message,progress,bytes_downloaded,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'queued','Waiting to download',0,0,?8,?8)", params![id,library_id,book.provider,book.provider_id,book.title,serde_json::to_string(&book.authors).unwrap_or_else(|_| "[]".into()),serde_json::to_string(book).map_err(|e| AppError::Internal(e.to_string()))?,now])?;
        drop(conn);
        self.download_job(id)
    }

    pub fn download_job(&self, id: &str) -> Result<DownloadJobDto> {
        self.0
            .lock()
            .query_row(
                &(DOWNLOAD_JOB_SELECT.to_owned() + " WHERE id=?1"),
                [id],
                download_job_from_row,
            )
            .optional()?
            .ok_or(AppError::NotFound)
    }

    pub fn download_job_for(
        &self,
        library_id: i64,
        provider: &str,
        provider_id: &str,
    ) -> Result<Option<DownloadJobDto>> {
        Ok(self
            .0
            .lock()
            .query_row(
                &(DOWNLOAD_JOB_SELECT.to_owned()
                    + " WHERE library_id=?1 AND provider=?2 AND provider_id=?3"),
                params![library_id, provider, provider_id],
                download_job_from_row,
            )
            .optional()?)
    }

    pub fn download_book(&self, id: &str) -> Result<CatalogBookDto> {
        let json: String = self
            .0
            .lock()
            .query_row(
                "SELECT book_json FROM download_jobs WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(AppError::NotFound)?;
        serde_json::from_str(&json).map_err(|e| AppError::Internal(e.to_string()))
    }

    pub fn download_jobs(&self) -> Result<Vec<DownloadJobDto>> {
        let conn = self.0.lock();
        let mut stmt =
            conn.prepare(&(DOWNLOAD_JOB_SELECT.to_owned() + " ORDER BY created_at DESC"))?;
        let rows = stmt
            .query_map([], download_job_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_download_job(
        &self,
        id: &str,
        status: &str,
        message: &str,
        progress: f64,
        bytes: i64,
        total: Option<i64>,
        local_path: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.0.lock().execute("UPDATE download_jobs SET status=?2,message=?3,progress=?4,bytes_downloaded=?5,total_bytes=COALESCE(?6,total_bytes),local_path=COALESCE(?7,local_path),error=?8,updated_at=?9 WHERE id=?1", params![id,status,message,progress.clamp(0.0,1.0),bytes,total,local_path,error,chrono::Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn set_download_checksum(&self, id: &str, checksum: &str) -> Result<()> {
        self.0.lock().execute(
            "UPDATE download_jobs SET content_sha256=?2,updated_at=?3 WHERE id=?1",
            params![id, checksum, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn completed_download_by_checksum(
        &self,
        checksum: &str,
        excluding: &str,
    ) -> Result<Option<DownloadJobDto>> {
        Ok(self.0.lock().query_row(
            &(DOWNLOAD_JOB_SELECT.to_owned()+" WHERE content_sha256=?1 AND id<>?2 AND status='completed' ORDER BY updated_at DESC LIMIT 1"),
            params![checksum, excluding],
            download_job_from_row,
        ).optional()?)
    }

    pub fn delete_download_job(&self, id: &str) -> Result<()> {
        self.0.lock().execute("DELETE FROM download_jobs WHERE id=?1 AND status IN ('completed','cancelled','failed')", [id])?;
        Ok(())
    }

    pub fn clear_finished_downloads(&self) -> Result<usize> {
        Ok(self.0.lock().execute(
            "DELETE FROM download_jobs WHERE status IN ('completed','cancelled')",
            [],
        )?)
    }
}

#[cfg(windows)]
fn hidden_windows_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(windows)]
fn firewall_rule_detected() -> Option<bool> {
    let executable = std::env::current_exe()
        .ok()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let output = hidden_windows_command("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=all",
            "verbose",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rules = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    Some(rules.contains(&executable))
}

#[cfg(not(windows))]
fn firewall_rule_detected() -> Option<bool> {
    None
}

#[cfg(windows)]
fn firewall_guidance() -> String {
    "For iPhone access, allow sTori through Windows Defender Firewall when prompted. Private networks are preferred; Windows Mobile Hotspot may require the Public profile on some PCs.".into()
}

#[cfg(not(windows))]
fn firewall_guidance() -> String {
    "Allow incoming connections to sTori on TCP port 1822 in your system firewall.".into()
}

fn display_path(path: String) -> String {
    #[cfg(windows)]
    {
        if let Some(value) = path.strip_prefix(r"\\?\") {
            return value.to_string();
        }
    }
    path
}

fn download_job_from_row(r: &Row) -> rusqlite::Result<DownloadJobDto> {
    Ok(DownloadJobDto {
        id: r.get(0)?,
        library_id: r.get(1)?,
        provider: r.get(2)?,
        provider_id: r.get(3)?,
        title: r.get(4)?,
        authors: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
        status: r.get(6)?,
        message: r.get(7)?,
        progress: r.get(8)?,
        bytes_downloaded: r.get(9)?,
        total_bytes: r.get(10)?,
        local_path: r.get(11)?,
        error: r.get(12)?,
        content_sha256: r.get(13)?,
        created_at: r.get(14)?,
        updated_at: r.get(15)?,
    })
}

fn book_from_row(r: &Row) -> rusqlite::Result<BookDto> {
    Ok(BookDto {
        id: r.get(0)?,
        library_id: r.get(1)?,
        title: r.get(2)?,
        subtitle: r.get(3)?,
        authors: serde_json::from_str::<Vec<String>>(&r.get::<_, String>(4)?).unwrap_or_default(),
        description: r.get(5)?,
        published: r.get(6)?,
        tags: serde_json::from_str::<Vec<String>>(&r.get::<_, String>(7)?).unwrap_or_default(),
        identifier: r.get(8)?,
        format: r.get(9)?,
        file_name: r.get(10)?,
        series_id: r.get(11)?,
        series_name: r.get(12)?,
        series_index: r.get(13)?,
        progress: r.get(14)?,
        updated_at: r.get(15)?,
        has_cover: r.get::<_, Option<String>>(16)?.is_some(),
    })
}

const SELECT_BOOKS:&str="SELECT b.id,b.library_id,b.title,b.subtitle,b.authors_json,b.description,b.published,b.tags_json,b.identifier,b.format,b.file_name,b.series_id,s.name,b.series_index,COALESCE(r.progress,0),b.updated_at,b.cover_path FROM books b LEFT JOIN series s ON s.id=b.series_id LEFT JOIN reading_state r ON r.book_id=b.id";
const DOWNLOAD_JOB_SELECT: &str = "SELECT id,library_id,provider,provider_id,title,authors_json,status,message,progress,bytes_downloaded,total_bytes,local_path,error,content_sha256,created_at,updated_at FROM download_jobs";
const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS libraries(id INTEGER PRIMARY KEY,name TEXT NOT NULL,path TEXT NOT NULL UNIQUE,last_scanned_at TEXT);
CREATE TABLE IF NOT EXISTS series(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE,description TEXT,updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS books(id INTEGER PRIMARY KEY,library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,directory_path TEXT NOT NULL,title TEXT NOT NULL,subtitle TEXT,authors_json TEXT NOT NULL DEFAULT '[]',description TEXT,published TEXT,tags_json TEXT NOT NULL DEFAULT '[]',identifier TEXT,format TEXT NOT NULL,file_path TEXT NOT NULL,file_name TEXT NOT NULL,cover_path TEXT,series_id INTEGER REFERENCES series(id),series_index REAL,updated_at TEXT NOT NULL,UNIQUE(library_id,directory_path));
CREATE TABLE IF NOT EXISTS book_files(id INTEGER PRIMARY KEY,book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,format TEXT NOT NULL,path TEXT NOT NULL,UNIQUE(book_id,path));
CREATE VIRTUAL TABLE IF NOT EXISTS books_fts USING fts5(title,authors,tags,series);
CREATE TABLE IF NOT EXISTS reading_state(book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,locator TEXT NOT NULL,progress REAL NOT NULL DEFAULT 0,updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS annotations(id INTEGER PRIMARY KEY,book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,kind TEXT NOT NULL,locator TEXT NOT NULL,text TEXT,note TEXT,created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS collections(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE,description TEXT,updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS collection_books(collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,position INTEGER NOT NULL,PRIMARY KEY(collection_id,book_id));
CREATE TABLE IF NOT EXISTS reader_tokens(token TEXT PRIMARY KEY,created_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS app_settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS download_jobs(id TEXT PRIMARY KEY,library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,provider TEXT NOT NULL,provider_id TEXT NOT NULL,title TEXT NOT NULL,authors_json TEXT NOT NULL DEFAULT '[]',book_json TEXT NOT NULL,status TEXT NOT NULL,message TEXT NOT NULL,progress REAL NOT NULL DEFAULT 0,bytes_downloaded INTEGER NOT NULL DEFAULT 0,total_bytes INTEGER,local_path TEXT,error TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,UNIQUE(library_id,provider,provider_id));
"#;

fn verify_integrity(conn: &Connection) -> Result<()> {
    let result: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(AppError::Internal(format!(
            "Database integrity check failed: {result}"
        )));
    }
    Ok(())
}

fn migrate(conn: &mut Connection, path: &Path, existed: bool) -> Result<()> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > LATEST_SCHEMA_VERSION {
        return Err(AppError::Internal(format!(
            "This database belongs to a newer sTori version (schema {version})"
        )));
    }
    if version < LATEST_SCHEMA_VERSION && existed {
        backup_before_migration(conn, path, version)?;
    }
    if version < 1 {
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
        version = 1;
    }
    if version < 2 {
        let tx = conn.transaction()?;
        tx.execute_batch("ALTER TABLE download_jobs ADD COLUMN content_sha256 TEXT; CREATE INDEX IF NOT EXISTS idx_download_jobs_sha256 ON download_jobs(content_sha256);")?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
        version = 2;
    }
    if version < 3 {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "ALTER TABLE reader_tokens ADD COLUMN device_id TEXT;
             ALTER TABLE reader_tokens ADD COLUMN name TEXT NOT NULL DEFAULT 'Previously paired device';
             ALTER TABLE reader_tokens ADD COLUMN last_seen_at TEXT;
             ALTER TABLE reader_tokens ADD COLUMN last_ip TEXT;
             ALTER TABLE reader_tokens ADD COLUMN user_agent TEXT;
             ALTER TABLE reader_tokens ADD COLUMN revoked_at TEXT;
             UPDATE reader_tokens SET device_id=lower(hex(randomblob(16))),last_seen_at=created_at WHERE device_id IS NULL;
             CREATE UNIQUE INDEX IF NOT EXISTS idx_reader_tokens_device_id ON reader_tokens(device_id);
             CREATE INDEX IF NOT EXISTS idx_reader_tokens_active ON reader_tokens(revoked_at);",
        )?;
        tx.pragma_update(None, "user_version", 3)?;
        tx.commit()?;
        version = 3;
    }
    if version < 4 {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM books WHERE lower(format)='mobi'", [])?;
        tx.execute("DELETE FROM books_fts", [])?;
        tx.execute("INSERT INTO books_fts(rowid,title,authors,tags,series) SELECT b.id,b.title,b.authors_json,b.tags_json,COALESCE(s.name,'') FROM books b LEFT JOIN series s ON s.id=b.series_id", [])?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.commit()?;
    }
    if version < 5 {
        let tx = conn.transaction()?;
        tx.execute_batch("DELETE FROM annotations WHERE kind='bookmark' AND id NOT IN (SELECT MIN(id) FROM annotations WHERE kind='bookmark' GROUP BY book_id, locator); CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmarks_unique_location ON annotations(book_id, kind, locator) WHERE kind='bookmark';")?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.commit()?;
    }
    verify_integrity(conn)
}

fn backup_dto(path: &Path) -> Result<BackupDto> {
    let metadata = std::fs::metadata(path)?;
    let created = metadata
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    Ok(BackupDto {
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("backup.db")
            .to_string(),
        path: display_path(path.to_string_lossy().to_string()),
        size_bytes: metadata.len(),
        created_at: chrono::DateTime::<chrono::Utc>::from(created).to_rfc3339(),
    })
}

fn backup_before_migration(conn: &Connection, path: &Path, from_version: i64) -> Result<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(FULL)")?;
    let backup_dir = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups");
    std::fs::create_dir_all(&backup_dir)?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("stori");
    let destination = backup_dir.join(format!("{name}-schema-{from_version}-{stamp}.db"));
    std::fs::copy(path, destination)?;
    prune_backups(&backup_dir, name, 5)?;
    Ok(())
}

fn prune_backups(directory: &Path, prefix: &str, keep: usize) -> Result<()> {
    let mut files = std::fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with(prefix) && value.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect::<Vec<PathBuf>>();
    files.sort();
    let remove_count = files.len().saturating_sub(keep);
    for path in files.into_iter().take(remove_count) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_library;

    #[test]
    fn migrates_a_legacy_database_and_keeps_a_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("legacy.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute(
                "INSERT INTO app_settings(key,value) VALUES('kept','yes')",
                [],
            )
            .unwrap();
            conn.execute("INSERT INTO reader_tokens(token,created_at) VALUES('old-phone','2026-01-01T00:00:00Z')", []).unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.setting("kept").unwrap().as_deref(), Some("yes"));
        let version: i64 =
            db.0.lock()
                .query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);
        let columns = {
            let conn = db.0.lock();
            let mut statement = conn.prepare("PRAGMA table_info(download_jobs)").unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert!(columns.iter().any(|column| column == "content_sha256"));
        let devices = db.paired_devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Previously paired device");
        assert!(db.validate_and_touch_reader_token("old-phone", Some("192.168.137.2")));
        assert!(db.revoke_device(&devices[0].id).unwrap());
        assert!(!db.validate_and_touch_reader_token("old-phone", None));
        assert_eq!(
            std::fs::read_dir(temp.path().join("backups"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn creates_a_manual_backup_without_changing_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("stori.db");
        let db = Database::open(&path).unwrap();
        db.set_setting("backup-test", "preserved").unwrap();
        let backup = db.create_manual_backup().unwrap();
        assert!(Path::new(&backup.path).exists());
        let copy = Database::open(Path::new(&backup.path)).unwrap();
        assert_eq!(
            copy.setting("backup-test").unwrap().as_deref(),
            Some("preserved")
        );
    }

    #[test]
    fn bookmarks_are_unique_per_book_and_can_be_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("bookmarks.db")).unwrap();
        db.0.lock().execute_batch("INSERT INTO libraries(id,name,path) VALUES(1,'Test','C:/Test'); INSERT INTO books(id,library_id,directory_path,title,authors_json,tags_json,format,file_path,file_name,updated_at) VALUES(1,1,'one','One','[]','[]','epub','one.epub','one.epub','now'),(2,1,'two','Two','[]','[]','epub','two.epub','two.epub','now');").unwrap();
        let first = db.add_bookmark(1, "epubcfi(/6/2)", Some("10%".into())).unwrap();
        let duplicate = db.add_bookmark(1, "epubcfi(/6/2)", Some("changed".into())).unwrap();
        assert_eq!(first.id, duplicate.id);
        let legacy = db.add_annotation(1, &AnnotationRequest { kind: "bookmark".into(), locator: "epubcfi(/6/2)".into(), text: Some("legacy".into()), note: None }).unwrap();
        assert_eq!(first.id, legacy.id);
        assert_eq!(db.bookmarks(1).unwrap().len(), 1);
        assert!(db.bookmarks(2).unwrap().is_empty());
        assert!(db.add_bookmark(1, "", None).is_err());
        assert!(db.add_bookmark(999, "epubcfi(/6/2)", None).is_err());
        db.delete_bookmark(1, first.id).unwrap();
        assert!(db.bookmarks(1).unwrap().is_empty());
    }

    #[test]
    fn renames_and_removes_only_unmanaged_libraries() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("stori.db")).unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        let library = db
            .add_library("Original name", source.to_str().unwrap())
            .unwrap();
        assert_eq!(
            db.rename_library(library.id, "Renamed").unwrap().name,
            "Renamed"
        );
        db.remove_library(library.id).unwrap();
        assert!(
            source.is_dir(),
            "removing a library must not delete its folder"
        );

        let managed = db
            .ensure_managed_library(&temp.path().join("managed"))
            .unwrap();
        assert!(db.remove_library(managed.id).is_err());
    }

    #[test]
    fn indexes_an_optional_external_collection_read_only() {
        let Some(root) = std::env::var_os("STORI_TEST_LIBRARY").map(PathBuf::from) else {
            return;
        };
        if !root.exists() {
            return;
        }
        let (books, _) = scan_library(&root);
        assert!(
            !books.is_empty(),
            "the optional test library contains no supported books"
        );
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("test.db")).unwrap();
        let library = db.add_library("My Books", root.to_str().unwrap()).unwrap();
        db.store_scan(library.id, &books).unwrap();
        assert_eq!(db.books("", None).unwrap().len(), books.len());
    }
}
