use crate::error::{AppError, Result};
use roxmltree::Document;
use std::{collections::{HashMap, HashSet}, fs::File, io::Read, path::{Component, Path}};
use zip::ZipArchive;

pub const MAX_ENTRIES: usize = 5_000;
pub const MAX_TOTAL_EXPANDED_BYTES: u64 = 250 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_COMPRESSION_RATIO: u64 = 100;
pub const MAX_XML_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_XHTML_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_CSS_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_RESOURCE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ValidatedEpub { pub opf_path: String, pub warnings: Vec<String> }

pub fn validate_new_epub(path: &Path) -> Result<ValidatedEpub> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|_| AppError::BadRequest("This file is not a valid EPUB archive.".into()))?;
    if archive.is_empty() { return Err(AppError::BadRequest("This file is not a valid EPUB archive.".into())); }
    if archive.len() > MAX_ENTRIES { return Err(limit_error()); }
    let mut names = HashSet::new(); let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| AppError::BadRequest("This file is not a valid EPUB archive.".into()))?;
        let name = normalize_archive_path(entry.name())?;
        if !names.insert(name.clone()) { return Err(AppError::BadRequest("The EPUB contains conflicting archive paths.".into())); }
        if entry.encrypted() { return Err(AppError::BadRequest("This EPUB is encrypted and is not supported.".into())); }
        if !entry.is_dir() {
            if entry.size() > MAX_ENTRY_BYTES { return Err(limit_error()); }
            total = total.saturating_add(entry.size());
            if total > MAX_TOTAL_EXPANDED_BYTES { return Err(limit_error()); }
            if entry.compressed_size() > 0 && entry.size() / entry.compressed_size() > MAX_COMPRESSION_RATIO { return Err(limit_error()); }
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".xhtml") || lower.ends_with(".html") { if entry.size() > MAX_XHTML_BYTES { return Err(limit_error()); } }
            if lower.ends_with(".css") && entry.size() > MAX_CSS_BYTES { return Err(limit_error()); }
            if is_resource(&lower) && entry.size() > MAX_RESOURCE_BYTES { return Err(limit_error()); }
        }
    }
    for name in &names {
        let mut prefix = String::new();
        for piece in name.split('/').take(name.split('/').count().saturating_sub(1)) { prefix.push_str(piece); prefix.push('/'); if names.contains(prefix.trim_end_matches('/')) { return Err(AppError::BadRequest("The EPUB contains conflicting archive paths.".into())); } }
    }
    let mimetype = read_text(&mut archive, "mimetype", MAX_XML_BYTES)?;
    if mimetype.trim() != "application/epub+zip" { return Err(AppError::BadRequest("This file is not a valid EPUB archive.".into())); }
    let container = read_text(&mut archive, "META-INF/container.xml", MAX_XML_BYTES).map_err(|_| AppError::BadRequest("The EPUB is missing or has an invalid container document.".into()))?;
    let container_doc = Document::parse(&container).map_err(|_| AppError::BadRequest("The EPUB has an invalid container document.".into()))?;
    let opf_path = container_doc.descendants().find(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("rootfile")).and_then(|n| n.attribute("full-path")).ok_or_else(|| AppError::BadRequest("The EPUB is missing its package document.".into()))?;
    let opf_path = normalize_archive_path(opf_path)?;
    if !names.contains(&opf_path) { return Err(AppError::BadRequest("The EPUB is missing its package document.".into())); }
    let opf = read_text(&mut archive, &opf_path, MAX_XML_BYTES).map_err(|_| AppError::BadRequest("The EPUB has an invalid package document.".into()))?;
    let package = Document::parse(&opf).map_err(|_| AppError::BadRequest("The EPUB has an invalid package document.".into()))?;
    let mut manifest = HashMap::new();
    for item in package.descendants().filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("item")) {
        let id = item.attribute("id").filter(|v| !v.trim().is_empty()).ok_or_else(|| AppError::BadRequest("The EPUB has an invalid manifest.".into()))?;
        let href = item.attribute("href").filter(|v| !v.trim().is_empty()).ok_or_else(|| AppError::BadRequest("The EPUB has an invalid manifest.".into()))?;
        let target = resolve_epub_path(&opf_path, href)?;
        if manifest.insert(id.to_string(), target).is_some() { return Err(AppError::BadRequest("The EPUB has an invalid manifest.".into())); }
    }
    if manifest.is_empty() { return Err(AppError::BadRequest("The EPUB has an invalid manifest.".into())); }
    let mut warnings = Vec::new();
    for itemref in package.descendants().filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("itemref")) {
        let idref = itemref.attribute("idref").ok_or_else(|| AppError::BadRequest("The EPUB has a broken reading order.".into()))?;
        let target = manifest.get(idref).ok_or_else(|| AppError::BadRequest("The EPUB has a broken reading order.".into()))?;
        if !names.contains(target) { return Err(AppError::BadRequest("The EPUB has a broken reading order.".into())); }
    }
    if !package.descendants().any(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("itemref")) { warnings.push("The EPUB has no declared reading order.".into()); }
    Ok(ValidatedEpub { opf_path, warnings })
}

pub fn read_validated_entry(path: &Path, name: &str, max: usize) -> Result<Vec<u8>> { let mut archive = ZipArchive::new(File::open(path)?).map_err(|_| AppError::BadRequest("This file is not a valid EPUB archive.".into()))?; let mut entry = archive.by_name(name).map_err(|_| AppError::BadRequest("The EPUB is missing a required file.".into()))?; if entry.size() > max as u64 { return Err(limit_error()); } let mut bytes = Vec::with_capacity(entry.size() as usize); entry.read_to_end(&mut bytes).map_err(AppError::from)?; Ok(bytes) }

fn read_text(archive: &mut ZipArchive<File>, name: &str, max: usize) -> Result<String> { let mut entry = archive.by_name(name).map_err(|_| AppError::BadRequest("The EPUB is missing a required file.".into()))?; if entry.size() > max as u64 { return Err(limit_error()); } let mut bytes = Vec::with_capacity(entry.size() as usize); entry.read_to_end(&mut bytes)?; String::from_utf8(bytes).map_err(|_| AppError::BadRequest("The EPUB contains invalid text data.".into())) }
fn normalize_archive_path(value: &str) -> Result<String> { if value.is_empty() || value.starts_with('/') || value.starts_with('\\') || value.starts_with("//") || (value.len() > 1 && value.as_bytes()[1] == b':') { return Err(AppError::BadRequest("The EPUB contains unsafe archive paths.".into())); } let normalized = value.replace('\\', "/"); let path = Path::new(&normalized); let mut parts = Vec::new(); for component in path.components() { match component { Component::Normal(part) => parts.push(part.to_string_lossy().to_string()), Component::CurDir => {}, Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(AppError::BadRequest("The EPUB contains unsafe archive paths.".into())) } } if parts.is_empty() { return Err(AppError::BadRequest("The EPUB contains unsafe archive paths.".into())); } Ok(parts.join("/")) }
fn resolve_epub_path(opf: &str, href: &str) -> Result<String> { if href.contains("://") || href.starts_with("//") { return Ok(href.to_string()); } let base = opf.rsplit_once('/').map(|(base, _)| base).unwrap_or(""); normalize_archive_path(&format!("{base}/{href}")) }
fn is_resource(name: &str) -> bool { name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg") || name.ends_with(".gif") || name.ends_with(".webp") || name.ends_with(".svg") || name.ends_with(".woff") || name.ends_with(".woff2") || name.ends_with(".ttf") || name.ends_with(".otf") }
fn limit_error() -> AppError { AppError::BadRequest("This EPUB exceeds sTori’s safe import limits.".into()) }

#[cfg(test)]
mod tests {
    use super::*; use std::io::Write;
    fn epub(entries: Vec<(&str, Vec<u8>)>) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap(); let mut zip = zip::ZipWriter::new(file.reopen().unwrap()); let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries { zip.start_file(name, options).unwrap(); zip.write_all(&bytes).unwrap(); } zip.finish().unwrap(); file
    }
    fn valid(version: &str) -> tempfile::NamedTempFile { epub(vec![
        ("mimetype", b"application/epub+zip".to_vec()),
        ("META-INF/container.xml", b"<container><rootfiles><rootfile full-path=\"OPS/book.opf\"/></rootfiles></container>".to_vec()),
        ("OPS/book.opf", format!("<package version=\"{version}\"><manifest><item id=\"chapter\" href=\"chapter.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"chapter\"/></spine></package>").into_bytes()),
        ("OPS/chapter.xhtml", b"<html><body>ok</body></html>".to_vec())]) }
    #[test] fn accepts_minimal_epub2_and_epub3() { for version in ["2.0", "3.0"] { assert!(validate_new_epub(valid(version).path()).is_ok()); } }
    #[test] fn rejects_unsafe_archive_and_structure() {
        assert!(validate_new_epub(epub(vec![("../escape", vec![1])]).path()).is_err());
        assert!(validate_new_epub(epub(vec![("C:/escape", vec![1])]).path()).is_err());
        assert!(validate_new_epub(epub(vec![("mimetype", b"application/epub+zip".to_vec())]).path()).is_err());
        let broken = epub(vec![("mimetype", b"application/epub+zip".to_vec()), ("META-INF/container.xml", b"<container".to_vec())]); assert!(validate_new_epub(broken.path()).is_err());
    }
    #[test] fn rejects_broken_spine_and_compression_bomb_shape() {
        let broken = epub(vec![("mimetype", b"application/epub+zip".to_vec()), ("META-INF/container.xml", b"<container><rootfile full-path=\"book.opf\"/></container>".to_vec()), ("book.opf", b"<package><manifest><item id=\"a\" href=\"a.xhtml\"/></manifest><spine><itemref idref=\"missing\"/></spine></package>".to_vec())]); assert!(validate_new_epub(broken.path()).is_err());
        let bomb = epub(vec![("mimetype", b"application/epub+zip".to_vec()), ("META-INF/container.xml", b"<container><rootfile full-path=\"book.opf\"/></container>".to_vec()), ("book.opf", b"<package><manifest><item id=\"a\" href=\"a.xhtml\"/></manifest><spine><itemref idref=\"a\"/></spine></package>".to_vec()), ("a.xhtml", vec![b'x'; (MAX_XHTML_BYTES + 1) as usize])]); assert!(validate_new_epub(bomb.path()).is_err());
    }
}
