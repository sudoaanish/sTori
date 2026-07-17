use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct LibraryDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub book_count: i64,
    pub last_scanned_at: Option<String>,
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookDto {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub published: Option<String>,
    pub tags: Vec<String>,
    pub identifier: Option<String>,
    pub format: String,
    pub file_name: String,
    pub series_id: Option<i64>,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
    pub progress: f64,
    pub updated_at: String,
    pub has_cover: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupDto {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub books: Vec<BookDto>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadingStateDto {
    pub book_id: i64,
    pub locator: String,
    pub progress: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationDto {
    pub id: i64,
    pub book_id: i64,
    pub kind: String,
    pub locator: String,
    pub text: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AddLibraryRequest {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct CollectionRequest {
    pub name: String,
    pub description: Option<String>,
    pub book_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ProgressRequest {
    pub locator: String,
    pub progress: f64,
}

#[derive(Debug, Deserialize)]
pub struct AnnotationRequest {
    pub kind: String,
    pub locator: String,
    pub text: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PairRequest {
    pub code: String,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairedDeviceDto {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub last_ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupDto {
    pub file_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryDiagnosticDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub available: bool,
    pub book_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsDto {
    pub database_status: String,
    pub schema_version: i64,
    pub database_path: String,
    pub backup_directory: String,
    pub managed_library_free_bytes: Option<u64>,
    pub firewall_rule_detected: Option<bool>,
    pub firewall_guidance: String,
    pub libraries: Vec<LibraryDiagnosticDto>,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub indexed: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogBookDto {
    pub provider: String,
    pub provider_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub published: Option<String>,
    pub subjects: Vec<String>,
    pub cover_url: Option<String>,
    pub download_url: String,
    pub source_url: Option<String>,
    pub license_label: String,
}

#[derive(Debug, Serialize)]
pub struct CatalogSearchDto {
    pub results: Vec<CatalogBookDto>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueueDownloadRequest {
    pub library_id: i64,
    pub book: CatalogBookDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadJobDto {
    pub id: String,
    pub library_id: i64,
    pub provider: String,
    pub provider_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub status: String,
    pub message: String,
    pub progress: f64,
    pub bytes_downloaded: i64,
    pub total_bytes: Option<i64>,
    pub local_path: Option<String>,
    pub error: Option<String>,
    pub content_sha256: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ScannedBook {
    pub directory_path: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub published: Option<String>,
    pub tags: Vec<String>,
    pub identifier: Option<String>,
    pub format: String,
    pub file_path: String,
    pub file_name: String,
    pub cover_path: Option<String>,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
    pub alternate_files: Vec<(String, String)>,
}
