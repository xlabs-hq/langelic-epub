//! EPUB reader built on a pure `zip` + `quick_xml` stack.
//!
//! All structural data (manifest, spine, metadata, cover hint) comes from
//! `opf::parse_extras`; the table of contents comes from `toc::build`. We
//! validate the `mimetype` envelope leniently — trailing whitespace, CRLF,
//! and a leading UTF-8 BOM are all accepted, since Calibre, fan exporters,
//! and several conversion tools emit them and other readers (Apple Books,
//! ADE) tolerate them silently.
//!
//! File paths exposed on the returned `Document` are **OPF-relative** — the
//! same paths you'd see in the OPF `<manifest>` `href` attributes. This keeps
//! round-tripping clean: `epub-builder` also expects OPF-relative paths.

use crate::error::AppError;
use crate::opf::{self, ManifestItem};
use crate::toc;
use crate::types::{Asset, Bytes, Chapter, Document};
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

pub fn parse(bytes: &[u8]) -> Result<Document, AppError> {
    opf::validate_mimetype(bytes)?;

    let (opf_bytes, opf_path) = opf::extract_from_zip(bytes)?;
    let extras = opf::parse_extras(&opf_bytes, &opf_path)?;

    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| AppError::InvalidZip(e.to_string()))?;

    let mut href_to_id: HashMap<String, String> = HashMap::new();
    for (id, mi) in &extras.manifest {
        href_to_id.insert(strip_fragment(&mi.href).to_string(), id.clone());
        href_to_id.insert(resolve_path(&extras.opf_dir, &mi.href), id.clone());
    }

    let spine_ids: HashSet<String> = extras.spine.iter().cloned().collect();

    let mut spine_opf_hrefs: HashSet<String> = HashSet::new();
    let spine = build_spine_from_extras(&mut archive, &extras, &mut spine_opf_hrefs)?;

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

    let cover_asset_id = resolve_cover_asset_id(&extras, &href_to_id);
    let toc = toc::build(&mut archive, &extras)?;

    Ok(Document {
        title: extras.title.clone().unwrap_or_default(),
        creators: extras.creators.clone(),
        language: extras.language.clone(),
        identifier: extras.identifier.clone().unwrap_or_default(),
        publisher: extras.publisher.clone(),
        date: extras.date.clone(),
        description: extras.description.clone(),
        rights: extras.rights.clone(),
        metadata: extras.other_dc.clone(),
        spine,
        assets,
        toc,
        cover_asset_id,
        version: extras.version.clone().unwrap_or_default(),
    })
}

fn build_spine_from_extras(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    extras: &opf::OpfExtras,
    spine_opf_hrefs: &mut HashSet<String>,
) -> Result<Vec<Chapter>, AppError> {
    let mut spine: Vec<Chapter> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for idref in &extras.spine {
        let Some(mi) = extras.manifest.get(idref) else {
            continue;
        };
        let opf_relative = strip_fragment(&mi.href).to_string();

        // Some EPUBs have multiple <itemref>s pointing at the same file via
        // fragment identifiers. Dedupe so each unique file appears once.
        if !spine_opf_hrefs.insert(opf_relative.clone()) {
            continue;
        }
        if !seen_ids.insert(idref.clone()) {
            continue;
        }

        let archive_path = resolve_path(&extras.opf_dir, &mi.href);
        let data = read_file_bytes(archive, &archive_path)?;
        let title = extract_xhtml_title(&data);
        spine.push(Chapter {
            id: idref.clone(),
            file_name: opf_relative,
            title,
            media_type: mi.media_type.clone(),
            data: Bytes(data),
        });
    }

    Ok(spine)
}

fn resolve_cover_asset_id(
    extras: &opf::OpfExtras,
    href_to_id: &HashMap<String, String>,
) -> Option<String> {
    extras.cover_id.as_ref().and_then(|cid| {
        if extras.manifest.contains_key(cid) {
            return Some(cid.clone());
        }

        let stripped = strip_fragment(cid);
        href_to_id
            .get(stripped)
            .or_else(|| href_to_id.get(&resolve_path(&extras.opf_dir, stripped)))
            .cloned()
    })
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

fn extract_xhtml_title(data: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(data);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut in_title = false;
    let mut title = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if local_name_bytes(e.name().into_inner()) == b"title" => {
                in_title = true;
                title.clear();
            }
            Ok(Event::Text(t)) if in_title => {
                if let Ok(s) = t.decode() {
                    title.push_str(&s);
                }
            }
            Ok(Event::GeneralRef(r)) if in_title => {
                if let Some(resolved) = resolve_xml_entity(r.as_ref()) {
                    title.push_str(&resolved);
                }
            }
            Ok(Event::End(ref e))
                if in_title && local_name_bytes(e.name().into_inner()) == b"title" =>
            {
                let title = title.trim();
                if title.is_empty() {
                    return None;
                }
                return Some(title.to_string());
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }

    None
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

fn local_name_bytes(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == b':') {
        Some(idx) => &bytes[idx + 1..],
        None => bytes,
    }
}

fn resolve_xml_entity(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    match s {
        "amp" => Some("&".to_string()),
        "lt" => Some("<".to_string()),
        "gt" => Some(">".to_string()),
        "quot" => Some("\"".to_string()),
        "apos" => Some("'".to_string()),
        hex if hex.starts_with("#x") || hex.starts_with("#X") => u32::from_str_radix(&hex[2..], 16)
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string()),
        num if num.starts_with('#') => num[1..]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string()),
        other => Some(format!("&{};", other)),
    }
}
