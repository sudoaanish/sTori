use crate::{
    db::Database,
    epub_validation::{read_validated_entry, validate_new_epub, MAX_RESOURCE_BYTES},
    error::{AppError, Result},
    models::{CatalogBookDto, CatalogSearchDto, DownloadJobDto},
    scanner::scan_library_with_cache,
};
use futures_util::StreamExt;
use parking_lot::Mutex;
use reqwest::{header, Client, Url};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::io::AsyncWriteExt;

const MAX_EPUB_BYTES: u64 = 200 * 1024 * 1024;

fn starter_books() -> [CatalogBookDto; 2] {
    [
        CatalogBookDto {
            provider: "Standard Ebooks".into(),
            provider_id: "f-scott-fitzgerald/the-great-gatsby".into(),
            title: "The Great Gatsby".into(),
            authors: vec!["F. Scott Fitzgerald".into()],
            description: Some("A vivid portrait of ambition, longing, and the American Dream in the Jazz Age.".into()),
            language: Some("en-US".into()),
            published: Some("1925".into()),
            subjects: vec!["Fiction".into()],
            cover_url: None,
            download_url: "https://standardebooks.org/ebooks/f-scott-fitzgerald/the-great-gatsby/downloads/f-scott-fitzgerald_the-great-gatsby.epub?source=download".into(),
            source_url: Some("https://standardebooks.org/ebooks/f-scott-fitzgerald/the-great-gatsby".into()),
            license_label: "Public-domain edition — Standard Ebooks".into(),
        },
        CatalogBookDto {
            provider: "Standard Ebooks".into(),
            provider_id: "mary-shelley/frankenstein".into(),
            title: "Frankenstein".into(),
            authors: vec!["Mary Shelley".into()],
            description: Some("Mary Shelley’s foundational tale of creation, responsibility, and belonging.".into()),
            language: Some("en-US".into()),
            published: Some("1818".into()),
            subjects: vec!["Horror".into(), "Science Fiction".into()],
            cover_url: None,
            download_url: "https://standardebooks.org/ebooks/mary-shelley/frankenstein/downloads/mary-shelley_frankenstein.epub?source=download".into(),
            source_url: Some("https://standardebooks.org/ebooks/mary-shelley/frankenstein".into()),
            license_label: "Public-domain edition — Standard Ebooks".into(),
        },
    ]
}

#[derive(Default)]
struct JobControl {
    pause: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Clone)]
pub struct DownloadManager {
    db: Database,
    client: Client,
    controls: Arc<Mutex<HashMap<String, Arc<JobControl>>>>,
    managed_library_path: PathBuf,
    cover_cache: PathBuf,
}

impl DownloadManager {
    pub fn new(db: Database, managed_library_path: PathBuf, cover_cache: PathBuf) -> Self {
        let client = Client::builder()
            .user_agent(concat!(
                "sTori/",
                env!("CARGO_PKG_VERSION"),
                " (+personal ebook server)"
            ))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if allowed_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .expect("valid HTTP client");
        Self {
            db,
            client,
            controls: Arc::new(Mutex::new(HashMap::new())),
            managed_library_path,
            cover_cache,
        }
    }

    pub async fn bootstrap(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.managed_library_path).await?;
        self.cleanup_interrupted_imports().await?;
        let library = self.db.ensure_managed_library(&self.managed_library_path)?;
        let scan_root = self.managed_library_path.clone();
        let cover_cache = self.cover_cache.clone();
        let (existing_books, _) = tokio::task::spawn_blocking(move || {
            scan_library_with_cache(&scan_root, Some(&cover_cache))
        })
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;
        self.db.store_scan(library.id, &existing_books)?;
        if self.db.setting("starter_pack_version")?.as_deref() != Some("1") {
            for book in starter_books() {
                if !starter_file_exists(&self.managed_library_path, &book)
                    && self
                        .db
                        .download_job_for(library.id, &book.provider, &book.provider_id)?
                        .is_none()
                {
                    self.queue(library.id, book)?;
                }
            }
            self.db.set_setting("starter_pack_version", "1")?;
        }
        Ok(())
    }

    async fn cleanup_interrupted_imports(&self) -> Result<()> {
        let staging = self.managed_library_path.join(".stori-imports");
        if staging.exists() {
            tokio::fs::remove_dir_all(&staging).await?;
        }
        tokio::fs::create_dir_all(&staging).await?;
        let part_root = std::env::temp_dir().join("stori-downloads");
        if !part_root.exists() {
            return Ok(());
        }
        let known = self
            .db
            .download_jobs()?
            .into_iter()
            .map(|job| job.id)
            .collect::<std::collections::HashSet<_>>();
        for entry in std::fs::read_dir(part_root)? {
            let path = entry?.path();
            let id = path
                .file_name()
                .and_then(|value| value.to_str())
                .and_then(|value| value.strip_suffix(".epub.part"));
            if id.map(|value| !known.contains(value)).unwrap_or(false) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }

    pub async fn search(&self, query: &str, page: u32) -> Result<CatalogSearchDto> {
        let query = query.trim();
        if query.len() < 2 {
            return Err(AppError::BadRequest("Enter at least two characters".into()));
        }
        let page = page.max(1);
        let mut last_error = None;
        for _ in 0..2 {
            match tokio::time::timeout(std::time::Duration::from_secs(12), self.search_gutenberg(query, page)).await {
                Ok(Ok((results, next_page))) => return Ok(CatalogSearchDto { results, warnings: Vec::new(), next_page }),
                Ok(Err(error)) => last_error = Some(error),
                Err(_) => last_error = Some(AppError::Internal("Project Gutenberg took too long to respond. Please try again.".into())),
            }
        }
        Err(last_error.unwrap_or_else(|| AppError::Internal("Project Gutenberg is temporarily unavailable. Please try again.".into())))
    }

    async fn search_gutenberg(&self, query: &str, page: u32) -> Result<(Vec<CatalogBookDto>, Option<u32>)> {
        let response: Value = self
            .client
            .get("https://gutendex.com/books")
            .query(&[("search", query), ("page", &page.to_string())])
            .send()
            .await
            .map_err(http_error)?
            .error_for_status()
            .map_err(http_error)?
            .json()
            .await
            .map_err(http_error)?;
        let mut out = Vec::new();
        for item in response["results"]
            .as_array()
            .into_iter()
            .flatten()
            .take(20)
        {
            let formats = item["formats"].as_object();
            let download_url = ["application/epub+zip", "application/epub3+zip"]
                .into_iter()
                .find_map(|key| formats.and_then(|f| f.get(key)).and_then(Value::as_str))
                .or_else(|| {
                    formats.and_then(|f| {
                        f.iter()
                            .find(|(k, v)| k.contains("epub") && v.as_str().is_some())
                            .and_then(|(_, v)| v.as_str())
                    })
                });
            let Some(download_url) = download_url else {
                continue;
            };
            let id = item["id"].as_i64().unwrap_or_default().to_string();
            out.push(CatalogBookDto {
                provider: "Project Gutenberg".into(),
                provider_id: id.clone(),
                title: string(&item["title"], "Untitled"),
                authors: item["authors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a["name"].as_str().map(str::to_string))
                    .collect(),
                description: None,
                language: item["languages"]
                    .as_array()
                    .and_then(|x| x.first())
                    .and_then(Value::as_str)
                    .map(str::to_string),
                published: None,
                subjects: strings(&item["subjects"]),
                cover_url: formats
                    .and_then(|f| f.get("image/jpeg"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                download_url: download_url.to_string(),
                source_url: Some(format!("https://www.gutenberg.org/ebooks/{id}")),
                license_label: "Public domain — Project Gutenberg".into(),
            });
        }
        Ok((out, response["next"].as_str().map(|_| page + 1)))
    }

    pub fn jobs(&self) -> Result<Vec<DownloadJobDto>> {
        self.db.download_jobs()
    }

    pub fn queue(&self, library_id: i64, book: CatalogBookDto) -> Result<DownloadJobDto> {
        validate_download_url(&book.download_url)?;
        self.db.library_path(library_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        let job = self.db.create_download_job(&id, library_id, &book)?;
        self.start(id);
        Ok(job)
    }

    pub fn pause(&self, id: &str) -> Result<()> {
        if let Some(control) = self.controls.lock().get(id) {
            control.pause.store(true, Ordering::Relaxed);
        }
        let job = self.db.download_job(id)?;
        if job.status == "queued" {
            self.db.update_download_job(
                id,
                "paused",
                "Paused",
                job.progress,
                job.bytes_downloaded,
                job.total_bytes,
                None,
                None,
            )?;
        }
        Ok(())
    }

    pub fn cancel(&self, id: &str) -> Result<()> {
        if let Some(control) = self.controls.lock().get(id) {
            control.cancel.store(true, Ordering::Relaxed);
        }
        let job = self.db.download_job(id)?;
        if !matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
            self.db.update_download_job(
                id,
                "cancelled",
                "Cancelled",
                job.progress,
                job.bytes_downloaded,
                job.total_bytes,
                None,
                None,
            )?;
        }
        Ok(())
    }

    pub fn resume(&self, id: &str) -> Result<()> {
        let job = self.db.download_job(id)?;
        if !matches!(job.status.as_str(), "paused" | "failed") {
            return Err(AppError::BadRequest(
                "Only paused or failed downloads can be resumed".into(),
            ));
        }
        self.db.update_download_job(
            id,
            "queued",
            "Waiting to download",
            job.progress,
            job.bytes_downloaded,
            job.total_bytes,
            None,
            None,
        )?;
        self.start(id.to_string());
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        self.db.delete_download_job(id)
    }

    pub fn clear_finished(&self) -> Result<usize> {
        self.db.clear_finished_downloads()
    }

    fn start(&self, id: String) {
        let control = Arc::new(JobControl::default());
        self.controls.lock().insert(id.clone(), control.clone());
        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(error) = manager.run_job(&id, &control).await {
                let part = std::env::temp_dir().join("stori-downloads").join(format!("{id}.epub.part"));
                let _ = tokio::fs::remove_file(part).await;
                if !control.pause.load(Ordering::Relaxed) && !control.cancel.load(Ordering::Relaxed)
                {
                    let job = manager.db.download_job(&id).ok();
                    let _ = manager.db.update_download_job(
                        &id,
                        "failed",
                        "Download failed",
                        job.as_ref().map(|j| j.progress).unwrap_or(0.0),
                        job.as_ref().map(|j| j.bytes_downloaded).unwrap_or(0),
                        job.and_then(|j| j.total_bytes),
                        None,
                        Some(&error.to_string()),
                    );
                }
            }
            manager.controls.lock().remove(&id);
        });
    }

    async fn run_job(&self, id: &str, control: &JobControl) -> Result<()> {
        let job = self.db.download_job(id)?;
        let book = self.db.download_book(id)?;
        validate_download_url(&book.download_url)?;
        let part = std::env::temp_dir()
            .join("stori-downloads")
            .join(format!("{id}.epub.part"));
        tokio::fs::create_dir_all(part.parent().unwrap()).await?;
        let existing = tokio::fs::metadata(&part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = self.client.get(&book.download_url);
        if existing > 0 {
            request = request.header(header::RANGE, format!("bytes={existing}-"));
        }
        self.db.update_download_job(
            id,
            "downloading",
            "Downloading EPUB",
            job.progress,
            existing as i64,
            None,
            None,
            None,
        )?;
        let response = request
            .send()
            .await
            .map_err(http_error)?
            .error_for_status()
            .map_err(http_error)?;
        let append = existing > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let start = if append { existing } else { 0 };
        let total = start + response.content_length().unwrap_or(0);
        if total > MAX_EPUB_BYTES {
            return Err(AppError::BadRequest(
                "EPUB is larger than the 200 MB safety limit".into(),
            ));
        }
        let root = PathBuf::from(self.db.library_path(job.library_id)?);
        let available = fs2::available_space(&root)?;
        let required = total.saturating_sub(start).saturating_add(16 * 1024 * 1024);
        if available < required {
            return Err(AppError::BadRequest(format!(
                "Not enough disk space. sTori needs about {} MB free to finish this import.",
                (required + 1024 * 1024 - 1) / (1024 * 1024)
            )));
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .await?;
        let mut downloaded = start;
        let mut persisted_at = std::time::Instant::now();
        let mut persisted_bytes = start;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if control.cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                self.db.update_download_job(
                    id,
                    "cancelled",
                    "Cancelled",
                    0.0,
                    0,
                    None,
                    None,
                    None,
                )?;
                return Ok(());
            }
            if control.pause.load(Ordering::Relaxed) {
                file.flush().await?;
                self.db.update_download_job(
                    id,
                    "paused",
                    "Paused",
                    ratio(downloaded, total),
                    downloaded as i64,
                    Some(total as i64),
                    None,
                    None,
                )?;
                return Ok(());
            }
            let chunk = chunk.map_err(http_error)?;
            downloaded += chunk.len() as u64;
            if downloaded > MAX_EPUB_BYTES {
                return Err(AppError::BadRequest(
                    "EPUB exceeded the 200 MB safety limit".into(),
                ));
            }
            file.write_all(&chunk).await?;
            if persisted_at.elapsed() >= std::time::Duration::from_millis(300)
                || downloaded.saturating_sub(persisted_bytes) >= 512 * 1024
            {
                self.db.update_download_job(
                    id,
                    "downloading",
                    "Downloading EPUB",
                    ratio(downloaded, total),
                    downloaded as i64,
                    Some(total as i64),
                    None,
                    None,
                )?;
                persisted_at = std::time::Instant::now();
                persisted_bytes = downloaded;
            }
        }
        file.flush().await?;
        self.db.update_download_job(
            id,
            "verifying",
            "Checking EPUB integrity",
            0.96,
            downloaded as i64,
            Some(total as i64),
            None,
            None,
        )?;
        let verify_path = part.clone();
        let fallback = book.clone();
        let imported = tokio::task::spawn_blocking(move || inspect_epub(&verify_path, &fallback))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??;
        let checksum_path = part.clone();
        let checksum = tokio::task::spawn_blocking(move || sha256_file(&checksum_path))
            .await
            .map_err(|e| AppError::Internal(e.to_string()))??;
        self.db.set_download_checksum(id, &checksum)?;
        if let Some(existing_path) = existing_content_path(&root, &checksum).or_else(|| {
            self.db
                .completed_download_by_checksum(&checksum, id)
                .ok()
                .flatten()
                .and_then(|job| job.local_path)
                .map(PathBuf::from)
                .filter(|path| path.exists())
        }) {
            let _ = tokio::fs::remove_file(&part).await;
            self.db.update_download_job(
                id,
                "completed",
                "Already in library",
                1.0,
                downloaded as i64,
                Some(total as i64),
                Some(&existing_path.to_string_lossy()),
                None,
            )?;
            return Ok(());
        }
        self.db.update_download_job(
            id,
            "importing",
            "Adding to library",
            0.98,
            downloaded as i64,
            Some(total as i64),
            None,
            None,
        )?;
        let author = imported
            .authors
            .first()
            .cloned()
            .unwrap_or_else(|| "Unknown Author".into());
        let preferred_dir = root
            .join(safe_name(&author))
            .join(safe_name(&imported.title));
        let dir = unique_directory(preferred_dir);
        let staging = root.join(".stori-imports").join(id);
        if staging.exists() {
            tokio::fs::remove_dir_all(&staging).await?;
        }
        tokio::fs::create_dir_all(&staging).await?;
        let staged_epub = unique_path(
            &staging,
            &format!("{} - {}", safe_name(&imported.title), safe_name(&author)),
            "epub",
        );
        tokio::fs::copy(&part, &staged_epub).await?;
        tokio::fs::write(staging.join("metadata.opf"), metadata_opf(&imported, &book)).await?;
        tokio::fs::write(staging.join(".stori.sha256"), format!("{checksum}\n")).await?;
        if let Some((extension, bytes)) = &imported.cover {
            tokio::fs::write(staging.join(format!("cover.{extension}")), bytes).await?;
        } else if let Some(url) = &book.cover_url {
            let _ = self.save_cover(url, &staging.join("cover.jpg")).await;
        }
        tokio::fs::create_dir_all(
            dir.parent()
                .ok_or_else(|| AppError::Internal("Invalid library destination".into()))?,
        )
        .await?;
        tokio::fs::rename(&staging, &dir).await?;
        let epub = dir.join(
            staged_epub
                .file_name()
                .ok_or_else(|| AppError::Internal("Invalid EPUB filename".into()))?,
        );
        let _ = tokio::fs::remove_file(&part).await;
        self.db.update_download_job(
            id,
            "indexing",
            "Refreshing library",
            0.99,
            downloaded as i64,
            Some(total as i64),
            Some(&epub.to_string_lossy()),
            None,
        )?;
        let scan_dir = dir.clone();
        let cover_cache = self.cover_cache.clone();
        let (books, _) = tokio::task::spawn_blocking(move || {
            scan_library_with_cache(&scan_dir, Some(&cover_cache))
        })
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
        self.db.store_incremental(job.library_id, &books)?;
        self.db.update_download_job(
            id,
            "completed",
            "Added to library",
            1.0,
            downloaded as i64,
            Some(total as i64),
            Some(&epub.to_string_lossy()),
            None,
        )?;
        Ok(())
    }

    async fn save_cover(&self, url: &str, path: &Path) -> Result<()> {
        validate_download_url(url)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(http_error)?
            .error_for_status()
            .map_err(http_error)?;
        if response.content_length().unwrap_or(0) > 10 * 1024 * 1024 {
            return Ok(());
        }
        let bytes = response.bytes().await.map_err(http_error)?;
        if bytes.len() <= 10 * 1024 * 1024 {
            tokio::fs::write(path, bytes).await?;
        }
        Ok(())
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn existing_content_path(root: &Path, checksum: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry.file_name() == ".stori.sha256"
                && std::fs::read_to_string(entry.path())
                    .ok()
                    .map(|value| value.trim() == checksum)
                    .unwrap_or(false)
        })
        .and_then(|entry| entry.path().parent().map(Path::to_path_buf))
        .and_then(|directory| {
            std::fs::read_dir(directory)
                .ok()?
                .filter_map(|entry| entry.ok().map(|value| value.path()))
                .find(|path| {
                    path.extension()
                        .and_then(|value| value.to_str())
                        .map(|value| value.eq_ignore_ascii_case("epub"))
                        .unwrap_or(false)
                })
        })
}

fn unique_directory(preferred: PathBuf) -> PathBuf {
    if !preferred.exists() {
        return preferred;
    }
    let parent = preferred.parent().unwrap_or_else(|| Path::new("."));
    let name = preferred
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Book");
    for index in 2..1000 {
        let candidate = parent.join(format!("{name} ({index})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{name}-{}", uuid::Uuid::new_v4()))
}

fn starter_file_exists(root: &Path, book: &CatalogBookDto) -> bool {
    let author = book
        .authors
        .first()
        .map(String::as_str)
        .unwrap_or("Unknown Author");
    let directory = root.join(safe_name(author)).join(safe_name(&book.title));
    std::fs::read_dir(directory)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("epub"))
                .unwrap_or(false)
        })
}

#[derive(Debug)]
struct ImportedMetadata {
    title: String,
    authors: Vec<String>,
    description: Option<String>,
    language: String,
    published: Option<String>,
    subjects: Vec<String>,
    identifier: String,
    series: Option<String>,
    series_index: Option<f64>,
    cover: Option<(String, Vec<u8>)>,
}

fn inspect_epub(path: &Path, fallback: &CatalogBookDto) -> Result<ImportedMetadata> {
    let validated = validate_new_epub(path)?;
    for warning in validated.warnings { tracing::warn!(%warning, "EPUB accepted with recoverable structural warning"); }
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::BadRequest(format!("Invalid EPUB archive: {e}")))?;
    let opf_path = validated.opf_path;
    let opf_xml = String::from_utf8(read_validated_entry(path, &opf_path, crate::epub_validation::MAX_XML_BYTES)?)
        .map_err(|_| AppError::BadRequest("The EPUB has an invalid package document.".into()))?;
    let package = roxmltree::Document::parse(&opf_xml)
        .map_err(|e| AppError::BadRequest(format!("Invalid EPUB package metadata: {e}")))?;
    let texts = |name: &str| {
        package
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(name))
            .filter_map(|n| n.text())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let first = |name: &str| texts(name).into_iter().next();
    let meta = |name: &str| {
        package
            .descendants()
            .find(|node| {
                node.is_element()
                    && node.tag_name().name().eq_ignore_ascii_case("meta")
                    && node
                        .attribute("name")
                        .map(|value| value.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
            })
            .and_then(|node| node.attribute("content"))
            .map(str::to_string)
    };
    let cover_id = meta("cover");
    let cover_item = package.descendants().find(|node| {
        node.is_element()
            && node.tag_name().name().eq_ignore_ascii_case("item")
            && (node
                .attribute("properties")
                .map(|value| value.split_whitespace().any(|part| part == "cover-image"))
                .unwrap_or(false)
                || cover_id
                    .as_deref()
                    .zip(node.attribute("id"))
                    .map(|(wanted, actual)| wanted == actual)
                    .unwrap_or(false))
    });
    let cover_path = cover_item
        .and_then(|node| node.attribute("href"))
        .map(|href| resolve_epub_path(&opf_path, href));
    let cover_media = cover_item
        .and_then(|node| node.attribute("media-type"))
        .unwrap_or("image/jpeg");
    let cover_extension = if cover_media.contains("png") {
        "png"
    } else if cover_media.contains("webp") {
        "webp"
    } else {
        "jpg"
    };
    let title = first("title").unwrap_or_else(|| fallback.title.clone());
    let authors = {
        let values = texts("creator");
        if values.is_empty() {
            fallback.authors.clone()
        } else {
            values
        }
    };
    let description = first("description").or_else(|| fallback.description.clone());
    let language = first("language")
        .or_else(|| fallback.language.clone())
        .unwrap_or_else(|| "en".into());
    let published = first("date").or_else(|| fallback.published.clone());
    let subjects = {
        let values = texts("subject");
        if values.is_empty() {
            fallback.subjects.clone()
        } else {
            values
        }
    };
    let identifier = first("identifier")
        .unwrap_or_else(|| format!("{}:{}", fallback.provider, fallback.provider_id));
    let series = meta("calibre:series");
    let series_index = meta("calibre:series_index").and_then(|value| value.parse().ok());
    drop(package);
    let cover = if let Some(cover_path) = cover_path {
        let mut bytes = Vec::new();
        match archive.by_name(&cover_path) {
            Ok(mut file) if file.size() <= MAX_RESOURCE_BYTES => {
                file.read_to_end(&mut bytes)?;
                Some((cover_extension.into(), bytes))
            }
            _ => None,
        }
    } else {
        None
    };
    Ok(ImportedMetadata {
        title,
        authors,
        description,
        language,
        published,
        subjects,
        identifier,
        series,
        series_index,
        cover,
    })
}

fn resolve_epub_path(opf_path: &str, href: &str) -> String {
    let base = opf_path
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or("");
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    parts.join("/")
}

fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str().unwrap_or_default(),
            "gutendex.com"
                | "gutenberg.org"
                | "www.gutenberg.org"
                | "standardebooks.org"
                | "www.standardebooks.org"
        )
}
fn validate_download_url(value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|_| AppError::BadRequest("Invalid download URL".into()))?;
    if allowed_url(&url) {
        Ok(())
    } else {
        Err(AppError::BadRequest("Download host is not approved".into()))
    }
}
fn http_error(error: reqwest::Error) -> AppError {
    AppError::Internal(error.to_string())
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}
fn string(value: &Value, fallback: &str) -> String {
    value.as_str().unwrap_or(fallback).to_string()
}
fn ratio(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).min(0.95)
    }
}
fn safe_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    cleaned
        .trim()
        .trim_end_matches('.')
        .chars()
        .take(100)
        .collect::<String>()
        .trim()
        .to_string()
        .chars()
        .collect::<String>()
        .pipe(|s| if s.is_empty() { "Untitled".into() } else { s })
}
fn unique_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{extension}"));
    if !first.exists() {
        return first;
    }
    for i in 2..1000 {
        let path = dir.join(format!("{stem} ({i}).{extension}"));
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{stem}-{}.{}", uuid::Uuid::new_v4(), extension))
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn metadata_opf(metadata: &ImportedMetadata, source: &CatalogBookDto) -> String {
    let creators = metadata
        .authors
        .iter()
        .map(|a| format!("    <dc:creator>{}</dc:creator>\n", xml(a)))
        .collect::<String>();
    let subjects = metadata
        .subjects
        .iter()
        .map(|s| format!("    <dc:subject>{}</dc:subject>\n", xml(s)))
        .collect::<String>();
    let published = metadata
        .published
        .as_deref()
        .map(|value| format!("    <dc:date>{}</dc:date>\n", xml(value)))
        .unwrap_or_default();
    let series = metadata
        .series
        .as_deref()
        .map(|value| {
            format!(
                "    <meta name=\"calibre:series\" content=\"{}\"/>\n",
                xml(value)
            )
        })
        .unwrap_or_default();
    let series_index = metadata
        .series_index
        .map(|value| format!("    <meta name=\"calibre:series_index\" content=\"{value}\"/>\n"))
        .unwrap_or_default();
    let source_url = source
        .source_url
        .as_deref()
        .map(|value| {
            format!(
                "    <meta name=\"stori:source\" content=\"{}\"/>\n",
                xml(value)
            )
        })
        .unwrap_or_default();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\n  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n    <dc:identifier id=\"id\">{}</dc:identifier>\n    <dc:title>{}</dc:title>\n{}{}{}    <dc:language>{}</dc:language>\n    <dc:description>{}</dc:description>\n{}{}{}  </metadata>\n</package>\n",xml(&metadata.identifier),xml(&metadata.title),creators,subjects,published,xml(&metadata.language),xml(metadata.description.as_deref().unwrap_or("")),series,series_index,source_url)
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sanitizes_windows_names() {
        assert_eq!(safe_name("A: Book?"), "A_ Book_");
    }
    #[test]
    fn rejects_unapproved_downloads() {
        assert!(validate_download_url("http://example.com/a.epub").is_err());
    }
    #[test]
    fn escapes_metadata() {
        assert!(xml("A & <B>").contains("&amp;"));
    }

    #[tokio::test]
    #[ignore = "performs a real public-domain EPUB download"]
    async fn downloads_validates_and_indexes_into_a_temporary_library() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("test.db")).unwrap();
        let library = db
            .add_library("Temporary", temp.path().to_str().unwrap())
            .unwrap();
        let manager = DownloadManager::new(
            db.clone(),
            temp.path().join("sTori Books"),
            temp.path().join("cover-cache"),
        );
        let job = manager
            .queue(
                library.id,
                CatalogBookDto {
                    provider: "Project Gutenberg".into(),
                    provider_id: "1342".into(),
                    title: "Pride and Prejudice".into(),
                    authors: vec!["Jane Austen".into()],
                    description: None,
                    language: Some("en".into()),
                    published: None,
                    subjects: vec!["Fiction".into()],
                    cover_url: None,
                    download_url: "https://www.gutenberg.org/ebooks/1342.epub3.images".into(),
                    source_url: Some("https://www.gutenberg.org/ebooks/1342".into()),
                    license_label: "Public domain — Project Gutenberg".into(),
                },
            )
            .unwrap();
        for _ in 0..120 {
            let current = manager.db.download_job(&job.id).unwrap();
            if current.status == "completed" {
                assert!(Path::new(current.local_path.as_ref().unwrap()).exists());
                assert!(!db.books("Pride", None).unwrap().is_empty());
                return;
            }
            if current.status == "failed" {
                panic!("{}", current.error.unwrap_or(current.message));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        panic!("download did not finish in time");
    }

    #[tokio::test]
    #[ignore = "downloads the two real Standard Ebooks starter editions"]
    async fn bootstraps_the_starter_shelf_exactly_once() {
        let temp = tempfile::tempdir().unwrap();
        let library_path = temp.path().join("Downloads").join("sTori Books");
        let db = Database::open(&temp.path().join("starter.db")).unwrap();
        let manager = DownloadManager::new(
            db.clone(),
            library_path.clone(),
            temp.path().join("cover-cache"),
        );
        manager.bootstrap().await.unwrap();
        for _ in 0..160 {
            let jobs = manager.jobs().unwrap();
            if jobs.iter().any(|job| job.status == "failed") {
                panic!(
                    "starter download failed: {:?}",
                    jobs.iter()
                        .find(|job| job.status == "failed")
                        .and_then(|job| job.error.clone())
                );
            }
            if jobs.len() == 2 && jobs.iter().all(|job| job.status == "completed") {
                manager.bootstrap().await.unwrap();
                assert_eq!(manager.jobs().unwrap().len(), 2);
                assert_eq!(db.books("", None).unwrap().len(), 2);
                assert!(library_path
                    .join("Mary Shelley")
                    .join("Frankenstein")
                    .join("cover.jpg")
                    .exists());
                let second_db = Database::open(&temp.path().join("second.db")).unwrap();
                let second_manager = DownloadManager::new(
                    second_db.clone(),
                    library_path.clone(),
                    temp.path().join("second-cover-cache"),
                );
                second_manager.bootstrap().await.unwrap();
                assert!(second_manager.jobs().unwrap().is_empty());
                assert_eq!(second_db.books("", None).unwrap().len(), 2);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        panic!("starter shelf did not finish in time");
    }
}
