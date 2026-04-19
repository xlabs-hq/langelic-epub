//! OPF re-parse layer for fields iepub drops.
//!
//! iepub's reader silently drops `<dc:language>` and `<dc:rights>`, and
//! collapses multiple `<dc:creator>` entries into a single comma-joined
//! string. We re-parse the OPF XML with quick-xml to recover the
//! structured values.

use crate::error::AppError;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};

#[derive(Debug, Default)]
pub struct OpfExtras {
    pub language: Option<String>,
    pub creators: Vec<String>,
    pub rights: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub date: Option<String>,
    pub title: Option<String>,
    pub identifier: Option<String>,
    /// Other `<dc:*>` entries (subject, contributor, type, format, source, relation, coverage).
    pub other_dc: HashMap<String, Vec<String>>,
    /// Manifest items: id -> (href, media-type, properties)
    pub manifest: HashMap<String, ManifestItem>,
    /// The cover asset id, derived from `<meta name="cover" content="..."/>` (EPUB 2)
    /// or `<item properties="cover-image" ...>` (EPUB 3).
    pub cover_id: Option<String>,
    /// Spine: ordered list of idrefs.
    pub spine: Vec<String>,
    /// EPUB package version ("2.0" or "3.0").
    pub version: Option<String>,
    /// Base directory of the OPF file within the archive (for href resolution).
    pub opf_dir: String,
}

#[derive(Debug, Clone)]
pub struct ManifestItem {
    pub href: String,
    pub media_type: String,
    pub properties: Option<String>,
}

pub fn extract_from_zip(epub_bytes: &[u8]) -> Result<(Vec<u8>, String), AppError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(epub_bytes))
        .map_err(|e| AppError::InvalidZip(e.to_string()))?;

    let container_xml = {
        let mut container = archive
            .by_name("META-INF/container.xml")
            .map_err(|_| AppError::MissingContainer)?;
        let mut s = String::new();
        container
            .read_to_string(&mut s)
            .map_err(|e| AppError::Io(e.to_string()))?;
        s
    };

    let opf_path = find_opf_path(&container_xml)?;

    let mut opf = archive
        .by_name(&opf_path)
        .map_err(|_| AppError::MissingOpf(opf_path.clone()))?;
    let mut buf = Vec::new();
    opf.read_to_end(&mut buf)
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok((buf, opf_path))
}

fn find_opf_path(container_xml: &str) -> Result<String, AppError> {
    let mut reader = Reader::from_str(container_xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                if local_name(&e) == b"rootfile" {
                    for attr in e.attributes().with_checks(false).flatten() {
                        if attr.key.as_ref() == b"full-path" {
                            let v = attr
                                .unescape_value()
                                .map_err(|err| AppError::MalformedOpf(err.to_string()))?;
                            return Ok(v.into_owned());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(AppError::MalformedOpf(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
    Err(AppError::MissingContainer)
}

pub fn parse_extras(opf_bytes: &[u8], opf_path: &str) -> Result<OpfExtras, AppError> {
    let mut extras = OpfExtras {
        opf_dir: opf_dir(opf_path),
        ..Default::default()
    };

    let mut reader = Reader::from_reader(opf_bytes);
    reader.config_mut().trim_text(true);

    #[derive(PartialEq)]
    enum Section {
        None,
        Metadata,
        Manifest,
        Spine,
    }

    let mut section = Section::None;
    let mut current_dc: Option<String> = None;
    let mut current_text = String::new();
    let mut buf = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| AppError::MalformedOpf(e.to_string()))?;

        match event {
            Event::Start(ref e) => {
                let raw_name = e.name().into_inner().to_vec();
                let local = local_name_bytes(&raw_name).to_vec();
                match local.as_slice() {
                    b"package" => {
                        for attr in e.attributes().with_checks(false).flatten() {
                            if attr.key.as_ref() == b"version" {
                                if let Ok(v) = attr.unescape_value() {
                                    extras.version = Some(v.into_owned());
                                }
                            }
                        }
                    }
                    b"metadata" => section = Section::Metadata,
                    b"manifest" => section = Section::Manifest,
                    b"spine" => section = Section::Spine,
                    _ => {
                        if section == Section::Metadata && is_dc_element(&raw_name) {
                            if let Ok(s) = std::str::from_utf8(&local) {
                                current_dc = Some(s.to_string());
                                current_text.clear();
                            }
                        }
                    }
                }
            }
            Event::Empty(ref e) => {
                let name = local_name(e).to_vec();
                match section {
                    Section::Metadata => {
                        // <meta name="cover" content="..."/> — EPUB 2 cover hint.
                        if name == b"meta" {
                            let mut meta_name = None;
                            let mut meta_content = None;
                            for attr in e.attributes().with_checks(false).flatten() {
                                match attr.key.as_ref() {
                                    b"name" => {
                                        meta_name =
                                            attr.unescape_value().ok().map(|c| c.into_owned())
                                    }
                                    b"content" => {
                                        meta_content =
                                            attr.unescape_value().ok().map(|c| c.into_owned())
                                    }
                                    _ => {}
                                }
                            }
                            if meta_name.as_deref() == Some("cover") {
                                extras.cover_id = meta_content;
                            }
                        }
                    }
                    Section::Manifest => {
                        if name == b"item" {
                            let item = parse_manifest_item(e)?;
                            if let Some((id, mi)) = item {
                                if mi
                                    .properties
                                    .as_deref()
                                    .unwrap_or("")
                                    .contains("cover-image")
                                {
                                    extras.cover_id.get_or_insert_with(|| id.clone());
                                }
                                extras.manifest.insert(id, mi);
                            }
                        }
                    }
                    Section::Spine => {
                        if name == b"itemref" {
                            for attr in e.attributes().with_checks(false).flatten() {
                                if attr.key.as_ref() == b"idref" {
                                    if let Ok(v) = attr.unescape_value() {
                                        extras.spine.push(v.into_owned());
                                    }
                                }
                            }
                        }
                    }
                    Section::None => {}
                }
            }
            Event::Text(t) => {
                if section == Section::Metadata && current_dc.is_some() {
                    if let Ok(s) = t.decode() {
                        current_text.push_str(&s);
                    }
                }
            }
            Event::GeneralRef(r) => {
                if section == Section::Metadata && current_dc.is_some() {
                    if let Some(resolved) = resolve_entity(r.as_ref()) {
                        current_text.push_str(&resolved);
                    }
                }
            }
            Event::End(ref e) => {
                let raw_name = e.name().into_inner().to_vec();
                let local = local_name_bytes(&raw_name).to_vec();
                match local.as_slice() {
                    b"metadata" | b"manifest" | b"spine" => section = Section::None,
                    _ => {
                        if section == Section::Metadata && is_dc_element(&raw_name) {
                            if let Ok(dc_name) = std::str::from_utf8(&local) {
                                if current_dc.as_deref() == Some(dc_name) {
                                    let value = std::mem::take(&mut current_text);
                                    assign_dc(&mut extras, dc_name, value);
                                    current_dc = None;
                                }
                            }
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(extras)
}

fn parse_manifest_item(
    e: &quick_xml::events::BytesStart<'_>,
) -> Result<Option<(String, ManifestItem)>, AppError> {
    let mut id = None;
    let mut href = None;
    let mut media_type = None;
    let mut properties = None;
    for attr in e.attributes().with_checks(false).flatten() {
        let value = attr
            .unescape_value()
            .map_err(|err| AppError::MalformedOpf(err.to_string()))?
            .into_owned();
        match attr.key.as_ref() {
            b"id" => id = Some(value),
            b"href" => href = Some(value),
            b"media-type" => media_type = Some(value),
            b"properties" => properties = Some(value),
            _ => {}
        }
    }
    match (id, href, media_type) {
        (Some(id), Some(href), Some(media_type)) => Ok(Some((
            id,
            ManifestItem {
                href,
                media_type,
                properties,
            },
        ))),
        _ => Ok(None),
    }
}

fn assign_dc(extras: &mut OpfExtras, name: &str, value: String) {
    let value = value.trim().to_string();
    if value.is_empty() {
        return;
    }
    match name {
        "title" => {
            extras.title.get_or_insert(value);
        }
        "language" => {
            extras.language.get_or_insert(value);
        }
        "creator" => extras.creators.push(value),
        "rights" => {
            extras.rights.get_or_insert(value);
        }
        "publisher" => {
            extras.publisher.get_or_insert(value);
        }
        "description" => {
            extras.description.get_or_insert(value);
        }
        "date" => {
            extras.date.get_or_insert(value);
        }
        "identifier" => {
            extras.identifier.get_or_insert(value);
        }
        other => {
            extras
                .other_dc
                .entry(other.to_string())
                .or_default()
                .push(value);
        }
    }
}

/// Returns true when `raw_name` is an element belonging to the Dublin Core
/// Metadata Element Set — i.e. one with the `dc:` prefix when OPFs use the
/// conventional `xmlns:dc="http://purl.org/dc/elements/1.1/"` namespace.
/// Bare element names without a prefix (like `<meta>`) are excluded so we
/// don't round-trip them back out as `<dc:meta>`.
fn is_dc_element(raw_name: &[u8]) -> bool {
    raw_name.starts_with(b"dc:") || raw_name.starts_with(b"DC:")
}

fn local_name<'a>(e: &'a quick_xml::events::BytesStart<'_>) -> &'a [u8] {
    local_name_bytes(e.name().into_inner())
}

fn local_name_bytes(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == b':') {
        Some(idx) => &bytes[idx + 1..],
        None => bytes,
    }
}

fn resolve_entity(raw: &[u8]) -> Option<String> {
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
        // Unknown entity — emit literally so we don't silently drop data.
        other => Some(format!("&{};", other)),
    }
}

fn opf_dir(opf_path: &str) -> String {
    match opf_path.rfind('/') {
        Some(idx) => opf_path[..=idx].to_string(),
        None => String::new(),
    }
}
