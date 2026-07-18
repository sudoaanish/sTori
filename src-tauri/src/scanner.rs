use crate::models::ScannedBook;
use roxmltree::Document;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use zip::ZipArchive;

#[cfg(test)]
pub fn scan_library(root: &Path) -> (Vec<ScannedBook>, Vec<String>) {
    scan_library_with_cache(root, None)
}

pub fn scan_library_with_cache(
    root: &Path,
    cover_cache: Option<&Path>,
) -> (Vec<ScannedBook>, Vec<String>) {
    let mut groups: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut warnings = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ["epub", "pdf"].contains(&ext.as_str()) {
            if let Some(parent) = path.parent() {
                groups
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(path.to_path_buf());
            }
        }
    }

    let mut books = Vec::new();
    for (dir, mut media) in groups {
        media.sort_by_key(|p| {
            match p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str()
            {
                "epub" => 0,
                "pdf" => 1,
                _ => 2,
            }
        });
        let primary = media[0].clone();
        let format = primary
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("epub")
            .to_ascii_lowercase();
        let opf_path = [dir.join("metadata.opf"), dir.join("content.opf")]
            .into_iter()
            .find(|p| p.exists());
        let mut metadata = opf_path.as_deref().and_then(parse_opf).unwrap_or_default();
        let embedded = (format == "epub").then(|| parse_epub(&primary)).flatten();
        if let Some(epub) = embedded.as_ref() {
            merge_metadata(&mut metadata, &epub.metadata);
        }

        let relative = dir.strip_prefix(root).unwrap_or(&dir);
        let parts: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        if metadata.title.is_empty() {
            metadata.title = clean_title(
                primary
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .unwrap_or("Untitled"),
            );
        }
        if metadata.authors.is_empty() {
            if let Some(author) = parts.first() {
                metadata.authors.push(author.clone());
            }
        }
        if metadata.series.is_none() && parts.len() >= 3 {
            metadata.series = parts.get(parts.len() - 2).cloned();
        }
        if metadata.series_index.is_none() {
            metadata.series_index =
                filename_index(primary.file_stem().and_then(|x| x.to_str()).unwrap_or(""));
        }

        let external_cover = [
            "cover.jpg",
            "cover.jpeg",
            "cover.png",
            "folder.jpg",
            "folder.jpeg",
            "poster.jpg",
        ]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.exists());
        let embedded_cover = embedded.as_ref().and_then(|epub| {
            epub.cover.as_ref().and_then(|cover| {
                cover_cache.and_then(|cache| cache_embedded_cover(&primary, cache, cover))
            })
        });
        let cover = external_cover.or(embedded_cover);

        if opf_path.is_none() && embedded.is_none() {
            warnings.push(format!("No readable metadata found: {}", primary.display()));
        }
        if cover.is_none() {
            warnings.push(format!("No cover found: {}", primary.display()));
        }

        let alternates = media
            .iter()
            .skip(1)
            .map(|path| {
                (
                    path.extension()
                        .and_then(|x| x.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    path.to_string_lossy().to_string(),
                )
            })
            .collect();
        books.push(ScannedBook {
            directory_path: dir.to_string_lossy().to_string(),
            title: metadata.title,
            subtitle: metadata.subtitle,
            authors: metadata.authors,
            description: metadata.description,
            published: metadata.published,
            tags: metadata.tags,
            identifier: metadata.identifier,
            format,
            file_path: primary.to_string_lossy().to_string(),
            file_name: primary
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            cover_path: cover.map(|path| path.to_string_lossy().to_string()),
            series_name: metadata.series,
            series_index: metadata.series_index,
            alternate_files: alternates,
        });
    }
    (books, warnings)
}

#[derive(Clone, Default)]
struct Metadata {
    title: String,
    subtitle: Option<String>,
    authors: Vec<String>,
    description: Option<String>,
    published: Option<String>,
    tags: Vec<String>,
    identifier: Option<String>,
    series: Option<String>,
    series_index: Option<f64>,
}

struct EmbeddedCover {
    bytes: Vec<u8>,
    extension: String,
}

struct EpubData {
    metadata: Metadata,
    cover: Option<EmbeddedCover>,
}

fn parse_opf(path: &Path) -> Option<Metadata> {
    parse_metadata_xml(&fs::read_to_string(path).ok()?)
}

fn parse_epub(path: &Path) -> Option<EpubData> {
    let file = fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let container = read_zip_text(&mut archive, "META-INF/container.xml")?;
    let container_doc = Document::parse(&container).ok()?;
    let opf_path = container_doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("rootfile"))?
        .attribute("full-path")?
        .to_string();
    let opf = read_zip_text(&mut archive, &opf_path)?;
    let metadata = parse_metadata_xml(&opf)?;
    let cover = cover_href(&opf).and_then(|href| {
        let entry = zip_join(&opf_path, &href);
        let bytes = read_zip_bytes(&mut archive, &entry)?;
        Some(EmbeddedCover {
            extension: cover_extension(&href, &bytes),
            bytes,
        })
    });
    Some(EpubData { metadata, cover })
}

fn read_zip_text(archive: &mut ZipArchive<fs::File>, name: &str) -> Option<String> {
    String::from_utf8(read_zip_bytes(archive, name)?).ok()
}

fn read_zip_bytes(archive: &mut ZipArchive<fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut entry = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn parse_metadata_xml(xml: &str) -> Option<Metadata> {
    let doc = Document::parse(xml).ok()?;
    let text = |name: &str| {
        doc.descendants()
            .find(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name))
            .and_then(|node| node.text())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let creators = doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("creator"))
        .filter_map(|node| node.text())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let tags = doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("subject"))
        .filter_map(|node| node.text())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let meta = |key: &str| {
        doc.descendants()
            .find(|node| {
                node.is_element()
                    && node.tag_name().name().eq_ignore_ascii_case("meta")
                    && node
                        .attribute("name")
                        .map(|value| value.eq_ignore_ascii_case(key))
                        .unwrap_or(false)
            })
            .and_then(|node| node.attribute("content"))
            .map(str::to_string)
    };
    Some(Metadata {
        title: text("title").unwrap_or_default(),
        subtitle: meta("calibre:subtitle"),
        authors: creators,
        description: text("description"),
        published: text("date"),
        tags,
        identifier: text("identifier"),
        series: meta("calibre:series"),
        series_index: meta("calibre:series_index").and_then(|value| value.parse().ok()),
    })
}

fn merge_metadata(target: &mut Metadata, fallback: &Metadata) {
    if target.title.is_empty() {
        target.title = fallback.title.clone();
    }
    if target.subtitle.is_none() {
        target.subtitle = fallback.subtitle.clone();
    }
    if target.authors.is_empty() {
        target.authors = fallback.authors.clone();
    }
    if target.description.is_none() {
        target.description = fallback.description.clone();
    }
    if target.published.is_none() {
        target.published = fallback.published.clone();
    }
    if target.tags.is_empty() {
        target.tags = fallback.tags.clone();
    }
    if target.identifier.is_none() {
        target.identifier = fallback.identifier.clone();
    }
    if target.series.is_none() {
        target.series = fallback.series.clone();
    }
    if target.series_index.is_none() {
        target.series_index = fallback.series_index;
    }
}

fn cover_href(opf: &str) -> Option<String> {
    let doc = Document::parse(opf).ok()?;
    let manifest = doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case("item"))
        .collect::<Vec<_>>();
    let meta_cover = doc
        .descendants()
        .find(|node| {
            node.is_element()
                && node.tag_name().name().eq_ignore_ascii_case("meta")
                && node
                    .attribute("name")
                    .map(|value| value.eq_ignore_ascii_case("cover"))
                    .unwrap_or(false)
        })
        .and_then(|node| node.attribute("content"));
    manifest
        .iter()
        .find(|node| {
            node.attribute("properties")
                .map(|value| {
                    value
                        .split_whitespace()
                        .any(|property| property == "cover-image")
                })
                .unwrap_or(false)
        })
        .or_else(|| {
            meta_cover.and_then(|id| {
                manifest
                    .iter()
                    .find(|node| node.attribute("id") == Some(id))
            })
        })
        .or_else(|| {
            manifest.iter().find(|node| {
                node.attribute("id")
                    .map(|id| id.to_ascii_lowercase().contains("cover"))
                    .unwrap_or(false)
            })
        })
        .and_then(|node| node.attribute("href"))
        .map(str::to_string)
}

fn zip_join(opf_path: &str, href: &str) -> String {
    let mut parts = opf_path
        .rsplit_once('/')
        .map(|(parent, _)| {
            parent
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let normalized_href = href.replace('\\', "/");
    for part in normalized_href.split('/') {
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

fn cover_extension(href: &str, bytes: &[u8]) -> String {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "png".into();
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return "jpg".into();
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return "webp".into();
    }
    match Path::new(href)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" | "png" | "webp" => Path::new(href)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap()
            .to_ascii_lowercase(),
        _ => "jpg".into(),
    }
}

fn cache_embedded_cover(book: &Path, cache_root: &Path, cover: &EmbeddedCover) -> Option<PathBuf> {
    fs::create_dir_all(cache_root).ok()?;
    let mut digest = Sha256::new();
    digest.update(book.to_string_lossy().as_bytes());
    if let Ok(metadata) = fs::metadata(book) {
        digest.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified().and_then(|time| {
            time.duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        }) {
            digest.update(modified.as_nanos().to_le_bytes());
        }
    }
    let path = cache_root.join(format!("{:x}.{}", digest.finalize(), cover.extension));
    if !path.exists() {
        fs::write(&path, &cover.bytes).ok()?;
    }
    Some(path)
}

fn clean_title(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '-' || c == '_')
        .replace('_', " ")
}
fn filename_index(value: &str) -> Option<f64> {
    let prefix = value.trim_start();
    let end = prefix
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(prefix.len());
    (end > 0).then(|| prefix[..end].parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cleans_numbered_title() {
        assert_eq!(clean_title("02 - Prince Caspian"), "Prince Caspian");
    }
    #[test]
    fn gets_series_index() {
        assert_eq!(filename_index("02 - Prince Caspian"), Some(2.0));
    }
    #[test]
    fn reads_embedded_epub_metadata_and_cover() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Books");
        fs::create_dir_all(&root).unwrap();
        let epub = root.join("example.epub");
        let file = fs::File::create(&epub).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();
        zip.start_file("OEBPS/content.opf", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><package><metadata><dc:title xmlns:dc="dc">Embedded title</dc:title><dc:creator xmlns:dc="dc">Embedded author</dc:creator><meta name="cover" content="cover-image"/></metadata><manifest><item id="cover-image" href="images/cover.png" media-type="image/png"/></manifest></package>"#).unwrap();
        zip.start_file("OEBPS/images/cover.png", options).unwrap();
        zip.write_all(b"not-a-real-png-but-a-cover-cache-test")
            .unwrap();
        zip.finish().unwrap();
        let cache = temp.path().join("cache");
        let (books, warnings) = scan_library_with_cache(&root, Some(&cache));
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Embedded title");
        assert_eq!(books[0].authors, vec!["Embedded author"]);
        assert!(books[0]
            .cover_path
            .as_ref()
            .is_some_and(|path| Path::new(path).is_file()));
        assert!(warnings.is_empty());
    }
}
