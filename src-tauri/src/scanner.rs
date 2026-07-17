use crate::models::ScannedBook;
use roxmltree::Document;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn scan_library(root: &Path) -> (Vec<ScannedBook>, Vec<String>) {
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
        let cover = [
            "cover.jpg",
            "cover.jpeg",
            "cover.png",
            "folder.jpg",
            "folder.jpeg",
            "poster.jpg",
        ]
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists());
        if opf_path.is_none() {
            warnings.push(format!("No OPF metadata: {}", dir.display()));
        }
        if cover.is_none() {
            warnings.push(format!("No external cover: {}", dir.display()));
        }
        let alternates = media
            .iter()
            .skip(1)
            .map(|p| {
                (
                    p.extension()
                        .and_then(|x| x.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase(),
                    p.to_string_lossy().to_string(),
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
            cover_path: cover.map(|p| p.to_string_lossy().to_string()),
            series_name: metadata.series,
            series_index: metadata.series_index,
            alternate_files: alternates,
        });
    }
    (books, warnings)
}

#[derive(Default)]
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

fn parse_opf(path: &Path) -> Option<Metadata> {
    let xml = fs::read_to_string(path).ok()?;
    let doc = Document::parse(&xml).ok()?;
    let text = |name: &str| {
        doc.descendants()
            .find(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case(name))
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let creators = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("creator"))
        .filter_map(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let tags = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name().eq_ignore_ascii_case("subject"))
        .filter_map(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let meta = |key: &str| {
        doc.descendants()
            .find(|n| {
                n.is_element()
                    && n.tag_name().name().eq_ignore_ascii_case("meta")
                    && n.attribute("name")
                        .map(|v| v.eq_ignore_ascii_case(key))
                        .unwrap_or(false)
            })
            .and_then(|n| n.attribute("content"))
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
        series_index: meta("calibre:series_index").and_then(|v| v.parse().ok()),
    })
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
    if end == 0 {
        None
    } else {
        prefix[..end].parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleans_numbered_title() {
        assert_eq!(clean_title("02 - Prince Caspian"), "Prince Caspian");
    }
    #[test]
    fn gets_series_index() {
        assert_eq!(filename_index("02 - Prince Caspian"), Some(2.0));
    }
}
