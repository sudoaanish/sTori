use crate::{
    db::Database,
    downloads::DownloadManager,
    error::{AppError, Result},
    models::*,
    scanner::scan_library,
};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path as AxPath, Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

pub const PORT: u16 = 1822;

#[derive(Clone)]
pub struct ServerState {
    pub db: Database,
    pub downloads: DownloadManager,
    pairings: Arc<Mutex<HashMap<String, Instant>>>,
    pairing_attempts: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
    managed_library: PathBuf,
}
impl ServerState {
    pub fn new(db: Database, managed_library: PathBuf) -> Self {
        let downloads = DownloadManager::new(db.clone(), managed_library.clone());
        Self {
            db,
            downloads,
            pairings: Arc::new(Mutex::new(HashMap::new())),
            pairing_attempts: Arc::new(Mutex::new(HashMap::new())),
            managed_library,
        }
    }
}

pub async fn run_with_std_listener(
    state: ServerState,
    dist: PathBuf,
    listener: std::net::TcpListener,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    run_with_listener(state, dist, listener).await
}

async fn run_with_listener(
    state: ServerState,
    dist: PathBuf,
    listener: tokio::net::TcpListener,
) -> std::io::Result<()> {
    if let Err(error) = state.downloads.bootstrap().await {
        tracing::warn!("Could not prepare starter shelf: {error}");
    }
    spawn_library_monitor(state.clone());
    let index = dist.join("index.html");
    let static_service = ServeDir::new(dist).not_found_service(ServeFile::new(index));
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/pair", post(pair))
        .route("/api/auth/session", get(session))
        .route("/api/admin/libraries", get(libraries).post(add_library))
        .route("/api/admin/libraries/{id}/scan", post(scan))
        .route("/api/admin/connectivity", get(connectivity))
        .route("/api/admin/pairing", post(create_pairing))
        .route("/api/admin/devices", get(paired_devices))
        .route("/api/admin/devices/revoke-all", post(revoke_all_devices))
        .route("/api/admin/devices/{id}/revoke", post(revoke_device))
        .route("/api/admin/diagnostics", get(diagnostics))
        .route("/api/admin/backups", get(backups).post(create_backup))
        .route("/api/admin/catalog/search", get(catalog_search))
        .route(
            "/api/admin/downloads",
            get(download_jobs).post(queue_download),
        )
        .route("/api/admin/downloads/{id}/pause", post(pause_download))
        .route("/api/admin/downloads/{id}/resume", post(resume_download))
        .route("/api/admin/downloads/{id}/cancel", post(cancel_download))
        .route("/api/admin/downloads/{id}/delete", post(delete_download))
        .route(
            "/api/admin/downloads/clear-finished",
            post(clear_finished_downloads),
        )
        .route("/api/library/rescan", post(rescan_all))
        .route("/api/books", get(books))
        .route("/api/books/{id}", get(book))
        .route("/api/books/{id}/file", get(book_file))
        .route("/api/books/{id}/cover", get(book_cover))
        .route("/api/books/{id}/progress", get(progress).put(save_progress))
        .route(
            "/api/books/{id}/annotations",
            get(annotations).post(add_annotation),
        )
        .route("/api/series", get(series))
        .route("/api/series/{id}", get(series_by_id))
        .route("/api/collections", get(collections).post(create_collection))
        .route(
            "/api/collections/{id}",
            get(collection).put(update_collection),
        )
        .fallback_service(static_service)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("Shutdown requested; finishing active requests");
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}))
}
async fn session(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(json!({"ok":true})))
}
fn is_local(addr: SocketAddr) -> bool {
    addr.ip().is_loopback()
}
fn bearer(headers: &HeaderMap, query_token: Option<&str>) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| query_token.map(str::to_string))
}
fn authorize(
    state: &ServerState,
    addr: SocketAddr,
    headers: &HeaderMap,
    token: Option<&str>,
) -> Result<()> {
    if is_local(addr) {
        return Ok(());
    }
    let token = bearer(headers, token).ok_or(AppError::Unauthorized)?;
    if state
        .db
        .validate_and_touch_reader_token(&token, Some(&addr.ip().to_string()))
    {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}
fn admin(addr: SocketAddr) -> Result<()> {
    if is_local(addr) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

async fn libraries(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<Vec<LibraryDto>>> {
    admin(a)?;
    Ok(Json(s.db.libraries()?))
}
async fn add_library(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    Json(r): Json<AddLibraryRequest>,
) -> Result<Json<LibraryDto>> {
    admin(a)?;
    Ok(Json(s.db.add_library(&r.name, &r.path)?))
}

#[derive(Deserialize)]
struct CatalogQuery {
    q: String,
}

async fn catalog_search(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    Query(q): Query<CatalogQuery>,
) -> Result<Json<CatalogSearchDto>> {
    admin(a)?;
    Ok(Json(s.downloads.search(&q.q).await?))
}

async fn download_jobs(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<Vec<DownloadJobDto>>> {
    admin(a)?;
    Ok(Json(s.downloads.jobs()?))
}

async fn queue_download(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    Json(r): Json<QueueDownloadRequest>,
) -> Result<Json<DownloadJobDto>> {
    admin(a)?;
    Ok(Json(s.downloads.queue(r.library_id, r.book)?))
}

async fn pause_download(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    s.downloads.pause(&id)?;
    Ok(Json(json!({"ok":true})))
}
async fn resume_download(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    s.downloads.resume(&id)?;
    Ok(Json(json!({"ok":true})))
}
async fn cancel_download(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    s.downloads.cancel(&id)?;
    Ok(Json(json!({"ok":true})))
}
async fn delete_download(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    s.downloads.delete(&id)?;
    Ok(Json(json!({"ok":true})))
}
async fn clear_finished_downloads(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    Ok(Json(json!({"removed":s.downloads.clear_finished()?})))
}
async fn scan(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<i64>,
) -> Result<Json<ScanResult>> {
    admin(a)?;
    let root = s.db.library_path(id)?;
    let (books, warnings) = tokio::task::spawn_blocking(move || scan_library(Path::new(&root)))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let indexed = books.len();
    s.db.store_scan(id, &books)?;
    Ok(Json(ScanResult { indexed, warnings }))
}

async fn rescan_all(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>> {
    authorize(&s, a, &headers, None)?;
    let libraries = s.db.libraries()?;
    let mut indexed = 0usize;
    let mut warnings = Vec::new();
    for library in &libraries {
        let root = library.path.clone();
        let (books, library_warnings) =
            tokio::task::spawn_blocking(move || scan_library(Path::new(&root)))
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
        indexed += books.len();
        warnings.extend(library_warnings);
        s.db.store_scan(library.id, &books)?;
    }
    Ok(Json(json!({
        "indexed": indexed,
        "warnings": warnings,
        "libraries": libraries.len()
    })))
}

#[derive(Deserialize)]
struct BookQuery {
    q: Option<String>,
    status: Option<String>,
}
async fn books(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<BookQuery>,
) -> Result<Json<Vec<BookDto>>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.books(
        q.q.as_deref().unwrap_or(""),
        q.status.as_deref(),
    )?))
}
async fn book(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
) -> Result<Json<BookDto>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.book(id)?))
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}
async fn book_file(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
    Query(q): Query<TokenQuery>,
) -> Result<Response> {
    authorize(&s, a, &headers, q.token.as_deref())?;
    let (path, _, format) = s.db.book_paths(id)?;
    serve_file(
        &path,
        mime_guess::from_ext(&format)
            .first_or_octet_stream()
            .as_ref(),
    )
    .await
}
async fn book_cover(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
    Query(q): Query<TokenQuery>,
) -> Result<Response> {
    authorize(&s, a, &headers, q.token.as_deref())?;
    let (_, cover, _) = s.db.book_paths(id)?;
    let path = cover.ok_or(AppError::NotFound)?;
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    serve_file(&path, mime.as_ref()).await
}
async fn serve_file(path: &str, mime: &str) -> Result<Response> {
    let data = tokio::fs::read(path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        Body::from(data),
    )
        .into_response())
}

async fn series(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<GroupDto>>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.series()?))
}
async fn series_by_id(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
) -> Result<Json<GroupDto>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.series_by_id(id)?))
}
async fn collections(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<Vec<GroupDto>>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.collections()?))
}
async fn collection(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
) -> Result<Json<GroupDto>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.collection(id)?))
}
async fn create_collection(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    Json(r): Json<CollectionRequest>,
) -> Result<Json<GroupDto>> {
    admin(a)?;
    Ok(Json(s.db.save_collection(None, &r)?))
}
async fn update_collection(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<i64>,
    Json(r): Json<CollectionRequest>,
) -> Result<Json<GroupDto>> {
    admin(a)?;
    Ok(Json(s.db.save_collection(Some(id), &r)?))
}
async fn progress(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
) -> Result<Json<Option<ReadingStateDto>>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.reading_state(id)?))
}
async fn save_progress(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
    Json(r): Json<ProgressRequest>,
) -> Result<Json<ReadingStateDto>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.save_progress(id, &r)?))
}
async fn annotations(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
) -> Result<Json<Vec<AnnotationDto>>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.annotations(id)?))
}
async fn add_annotation(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    AxPath(id): AxPath<i64>,
    Json(r): Json<AnnotationRequest>,
) -> Result<Json<AnnotationDto>> {
    authorize(&s, a, &headers, None)?;
    Ok(Json(s.db.add_annotation(id, &r)?))
}

#[derive(Clone, Serialize)]
struct UrlInfo {
    url: String,
    label: String,
    recommended: bool,
}
#[derive(Serialize)]
struct Connectivity {
    server_name: String,
    version: String,
    port: u16,
    urls: Vec<UrlInfo>,
    pairing_code: Option<String>,
}
fn urls() -> Vec<UrlInfo> {
    let mut rows = vec![UrlInfo {
        url: format!("http://127.0.0.1:{PORT}"),
        label: "This PC".into(),
        recommended: false,
    }];
    if let Ok(map) = local_ip_address::list_afinet_netifas() {
        for (name, ip) in map {
            if let IpAddr::V4(v4) = ip {
                if v4.is_private() && !v4.is_loopback() {
                    let hotspot = v4.octets()[0..3] == [192, 168, 137];
                    rows.push(UrlInfo {
                        url: format!("http://{v4}:{PORT}"),
                        label: if hotspot {
                            "Windows Hotspot / Recommended".into()
                        } else {
                            name
                        },
                        recommended: hotspot,
                    });
                }
            }
        }
    }
    rows.sort_by_key(|x| !x.recommended);
    rows
}
async fn connectivity(ConnectInfo(a): ConnectInfo<SocketAddr>) -> Result<Json<Connectivity>> {
    admin(a)?;
    Ok(Json(Connectivity {
        server_name: "sTori".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PORT,
        urls: urls(),
        pairing_code: None,
    }))
}
async fn create_pairing(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<Connectivity>> {
    admin(a)?;
    s.pairings
        .lock()
        .retain(|_, expires| *expires > Instant::now());
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000));
    s.pairings
        .lock()
        .insert(code.clone(), Instant::now() + Duration::from_secs(600));
    Ok(Json(Connectivity {
        server_name: "sTori".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PORT,
        urls: urls(),
        pairing_code: Some(code),
    }))
}
async fn pair(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(r): Json<PairRequest>,
) -> Result<Json<serde_json::Value>> {
    rate_limit_pairing(&s, a.ip())?;
    let valid = s
        .pairings
        .lock()
        .remove(&r.code)
        .map(|expires| expires > Instant::now())
        .unwrap_or(false);
    if !valid {
        return Err(AppError::BadRequest(
            "Pairing code is invalid or expired".into(),
        ));
    }
    let token = uuid::Uuid::new_v4().to_string();
    let requested_name = r.device_name.as_deref().unwrap_or("iPhone / Safari").trim();
    let name = if requested_name.is_empty() {
        "iPhone / Safari"
    } else {
        requested_name
    }
    .chars()
    .take(80)
    .collect::<String>();
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let device =
        s.db.save_reader_token(&token, &name, Some(&a.ip().to_string()), user_agent)?;
    Ok(Json(json!({"token":token,"device":device})))
}

fn rate_limit_pairing(state: &ServerState, ip: IpAddr) -> Result<()> {
    let now = Instant::now();
    let cutoff = now - Duration::from_secs(300);
    let mut attempts = state.pairing_attempts.lock();
    let entries = attempts.entry(ip).or_default();
    entries.retain(|attempt| *attempt > cutoff);
    if entries.len() >= 8 {
        return Err(AppError::BadRequest(
            "Too many pairing attempts. Wait five minutes and try again.".into(),
        ));
    }
    entries.push(now);
    Ok(())
}

async fn paired_devices(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<Vec<PairedDeviceDto>>> {
    admin(a)?;
    Ok(Json(s.db.paired_devices()?))
}
async fn revoke_device(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
    AxPath(id): AxPath<String>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    if !s.db.revoke_device(&id)? {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({"ok":true})))
}
async fn revoke_all_devices(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    admin(a)?;
    Ok(Json(json!({"revoked":s.db.revoke_all_devices()?})))
}
async fn diagnostics(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<DiagnosticsDto>> {
    admin(a)?;
    Ok(Json(s.db.database_diagnostics(&s.managed_library)?))
}
async fn backups(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<Vec<BackupDto>>> {
    admin(a)?;
    Ok(Json(s.db.backups()?))
}
async fn create_backup(
    State(s): State<ServerState>,
    ConnectInfo(a): ConnectInfo<SocketAddr>,
) -> Result<Json<BackupDto>> {
    admin(a)?;
    Ok(Json(s.db.create_manual_backup()?))
}

fn spawn_library_monitor(state: ServerState) {
    tokio::spawn(async move {
        let mut fingerprints: HashMap<i64, (u64, u64)> = HashMap::new();
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let libraries = match state.db.libraries() {
                Ok(rows) => rows,
                Err(_) => continue,
            };
            for library in libraries {
                let path = library.path.clone();
                let fingerprint =
                    tokio::task::spawn_blocking(move || library_fingerprint(Path::new(&path)))
                        .await
                        .ok();
                if let Some(fingerprint) = fingerprint {
                    let changed = fingerprints
                        .insert(library.id, fingerprint)
                        .map(|old| old != fingerprint)
                        .unwrap_or(false);
                    if changed {
                        let root = library.path.clone();
                        let id = library.id;
                        let db = state.db.clone();
                        tokio::task::spawn_blocking(move || {
                            let (books, _) = scan_library(Path::new(&root));
                            let _ = db.store_scan(id, &books);
                        });
                    }
                }
            }
        }
    });
}

fn library_fingerprint(root: &Path) -> (u64, u64) {
    let mut count = 0;
    let mut newest = 0;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let ext = entry
            .path()
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ["epub", "pdf", "opf", "jpg", "jpeg", "png"].contains(&ext.as_str()) {
            count += 1;
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(ts) = modified.duration_since(std::time::UNIX_EPOCH) {
                        newest = newest.max(ts.as_secs());
                    }
                }
            }
        }
    }
    (count, newest)
}
