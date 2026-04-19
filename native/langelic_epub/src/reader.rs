//! EPUB reader backed by iepub for structure + custom zip/OPF reads for
//! raw content and fields iepub silently drops.
//!
//! iepub is great at walking the spine, TOC tree, and cover detection, but:
//!   * `book.language()` / `book.rights()` return `None` even when the OPF
//!     sets them — iepub's `BookInfo` doesn't have those fields. We recover
//!     them via an OPF re-parse (`crate::opf`).
//!   * `book.creator()` collapses multiple `<dc:creator>` entries into a
//!     single comma-joined string. Again we use OPF re-parse for a proper
//!     `Vec<String>`.
//!   * iepub's lazy chapter loader (`data_mut()`) returns only the `<body>`
//!     content, not the full XHTML file. We want raw XHTML bytes so callers
//!     can modify whatever they like, so we read each file directly from the
//!     zip.
//!
//! File paths exposed on the returned `Document` are **OPF-relative** — the
//! same paths you'd see in the OPF `<manifest>` `href` attributes. This keeps
//! round-tripping clean: `epub-builder` also expects OPF-relative paths.

use crate::error::AppError;
use crate::opf::{self, ManifestItem};
use crate::types::{Asset, Bytes, Chapter, Document, NavItem};
use iepub::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

pub fn parse(bytes: &[u8]) -> Result<Document, AppError> {
    let (opf_bytes, opf_path) = opf::extract_from_zip(bytes)?;
    let extras = opf::parse_extras(&opf_bytes, &opf_path)?;

    let book = read_from_vec(bytes.to_vec())
        .map_err(|e| AppError::MalformedOpf(format!("{:?}", e)))?;

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::InvalidZip(e.to_string()))?;

    // Manifest lookups: href -> id (for id recovery from file names).
    // Both opf-relative href and archive-absolute path keyed for lookup.
    let mut href_to_id: HashMap<String, String> = HashMap::new();
    for (id, mi) in &extras.manifest {
        href_to_id.insert(strip_fragment(&mi.href).to_string(), id.clone());
        href_to_id.insert(resolve_path(&extras.opf_dir, &mi.href), id.clone());
    }

    let spine_ids: HashSet<String> = extras.spine.iter().cloned().collect();

    let mut spine: Vec<Chapter> = Vec::new();
    let mut spine_opf_hrefs: HashSet<String> = HashSet::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for html in book.chapters() {
        let archive_file_name = html.file_name().to_string();
        let archive_path = resolve_path(&extras.opf_dir, &archive_file_name);
        let opf_relative = to_opf_relative(&extras.opf_dir, &archive_file_name);

        // iepub adds a chapter per <itemref>; some EPUBs have multiple
        // <itemref>s pointing at the same file via fragment identifiers, and
        // iepub doesn't collapse them. Dedupe so each unique file appears
        // once in the spine.
        if !spine_opf_hrefs.insert(opf_relative.clone()) {
            continue;
        }

        let id = href_to_id
            .get(&opf_relative)
            .or_else(|| href_to_id.get(&archive_path))
            .cloned()
            .unwrap_or_else(|| derive_id_from_href(&opf_relative));
        if !seen_ids.insert(id.clone()) {
            continue;
        }

        let data = read_file_bytes(&mut archive, &archive_path)?;
        let title = {
            let t = html.title();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        let media_type = extras
            .manifest
            .get(&id)
            .map(|mi| mi.media_type.clone())
            .unwrap_or_else(|| "application/xhtml+xml".to_string());
        spine.push(Chapter {
            id,
            file_name: opf_relative,
            title,
            media_type,
            data: Bytes(data),
        });
    }

    let mut assets: Vec<Asset> = Vec::new();
    for (id, mi) in &extras.manifest {
        if spine_ids.contains(id) {
            continue;
        }
        let opf_relative = strip_fragment(&mi.href).to_string();
        if spine_opf_hrefs.contains(&opf_relative) {
            continue;
        }
        if is_nav_or_ncx(mi) {
            continue;
        }
        let archive_path = resolve_path(&extras.opf_dir, &mi.href);
        let data = read_file_bytes(&mut archive, &archive_path)?;
        assets.push(Asset {
            id: id.clone(),
            file_name: opf_relative,
            media_type: mi.media_type.clone(),
            data: Bytes(data),
        });
    }

    // Cover: prefer OPF-derived id; fall back to iepub detection by matching
    // the cover asset's path against the manifest.
    let cover_asset_id = extras.cover_id.clone().or_else(|| {
        book.cover().and_then(|c| {
            let archive_path = resolve_path(&extras.opf_dir, c.file_name());
            let opf_rel = to_opf_relative(&extras.opf_dir, c.file_name());
            href_to_id
                .get(&opf_rel)
                .or_else(|| href_to_id.get(&archive_path))
                .cloned()
        })
    });

    let toc: Vec<NavItem> = book.nav().map(|n| convert_nav(n, &extras.opf_dir)).collect();

    let title = extras
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| book.title().to_string());
    let identifier = extras
        .identifier
        .clone()
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| book.identifier().to_string());

    let creators = if extras.creators.is_empty() {
        book.creator()
            .map(split_creators)
            .unwrap_or_default()
    } else {
        extras.creators.clone()
    };

    let metadata = extras.other_dc.clone();
    let version = extras
        .version
        .clone()
        .unwrap_or_else(|| book.version().to_string());

    Ok(Document {
        title,
        creators,
        language: extras.language.clone(),
        identifier,
        publisher: extras
            .publisher
            .clone()
            .or_else(|| book.publisher().map(String::from)),
        date: extras
            .date
            .clone()
            .or_else(|| book.date().map(String::from)),
        description: extras
            .description
            .clone()
            .or_else(|| book.description().map(String::from)),
        rights: extras.rights.clone(),
        metadata,
        spine,
        assets,
        toc,
        cover_asset_id,
        version,
    })
}

fn convert_nav(nav: &EpubNav, opf_dir: &str) -> NavItem {
    NavItem {
        title: nav.title().to_string(),
        href: to_opf_relative(opf_dir, nav.file_name()),
        children: nav.child().map(|c| convert_nav(c, opf_dir)).collect(),
    }
}

fn read_file_bytes(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    path: &str,
) -> Result<Vec<u8>, AppError> {
    let mut f = archive
        .by_name(path)
        .map_err(|e| AppError::Io(format!("reading {}: {}", path, e)))?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf)
        .map_err(|e| AppError::Io(format!("reading {}: {}", path, e)))?;
    Ok(buf)
}

fn strip_fragment(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

fn resolve_path(opf_dir: &str, href: &str) -> String {
    let href = strip_fragment(href);
    if href.starts_with('/') {
        return normalize_path(href);
    }
    if opf_dir.is_empty() {
        return normalize_path(href);
    }
    normalize_path(&format!("{}{}", opf_dir, href))
}

fn to_opf_relative(opf_dir: &str, archive_path: &str) -> String {
    let archive_path = strip_fragment(archive_path);
    let normalized = normalize_path(archive_path);
    if opf_dir.is_empty() {
        return normalized;
    }
    let dir = opf_dir.trim_end_matches('/');
    if normalized == dir {
        return String::new();
    }
    if let Some(rest) = normalized.strip_prefix(&format!("{}/", dir)) {
        return rest.to_string();
    }
    normalized
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn derive_id_from_href(href: &str) -> String {
    href.replace(['/', '.', ' '], "_")
}

fn is_nav_or_ncx(mi: &ManifestItem) -> bool {
    if mi.media_type == "application/x-dtbncx+xml" {
        return true;
    }
    if let Some(props) = &mi.properties {
        if props.split_whitespace().any(|p| p == "nav") {
            return true;
        }
    }
    false
}

fn split_creators(joined: &str) -> Vec<String> {
    joined
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
