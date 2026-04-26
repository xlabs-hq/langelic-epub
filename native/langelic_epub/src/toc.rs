//! Table-of-contents walker for EPUB 3 `nav.xhtml` and EPUB 2 `toc.ncx`.
//!
//! Locates the TOC file via the OPF manifest (item with `properties="nav"`
//! for EPUB 3, or `media-type="application/x-dtbncx+xml"` for EPUB 2),
//! reads it from the zip, and produces a `Vec<NavItem>` whose `href` values
//! are OPF-relative — matching the rest of `Document`.

use crate::error::AppError;
use crate::opf::{ManifestItem, OpfExtras};
use crate::types::NavItem;
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use std::io::{Cursor, Read};

pub fn build(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    extras: &OpfExtras,
) -> Result<Vec<NavItem>, AppError> {
    let Some((mi, kind)) = find_toc(extras) else {
        return Ok(Vec::new());
    };

    let archive_path = resolve_archive_path(&extras.opf_dir, &mi.href);
    let bytes = read_file_bytes(archive, &archive_path)?;

    // hrefs inside the TOC file are relative to the TOC file's own location.
    // We resolve them to OPF-relative paths so they match the rest of Document.
    let toc_dir = parent_dir(&mi.href);

    Ok(match kind {
        TocKind::Nav => parse_nav_xhtml(&bytes, &toc_dir),
        TocKind::Ncx => parse_ncx(&bytes, &toc_dir),
    })
}

enum TocKind {
    Nav,
    Ncx,
}

fn find_toc(extras: &OpfExtras) -> Option<(&ManifestItem, TocKind)> {
    for mi in extras.manifest.values() {
        if let Some(props) = &mi.properties {
            if props.split_whitespace().any(|p| p == "nav") {
                return Some((mi, TocKind::Nav));
            }
        }
    }
    for mi in extras.manifest.values() {
        if mi.media_type == "application/x-dtbncx+xml" {
            return Some((mi, TocKind::Ncx));
        }
    }
    None
}

fn parse_nav_xhtml(bytes: &[u8], toc_dir: &str) -> Vec<NavItem> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut in_toc_nav = false;
    let mut nav_open_count: i32 = 0;
    let mut entry_depth: Option<i32> = None;
    let mut li_stack: Vec<NavItem> = Vec::new();
    let mut roots: Vec<NavItem> = Vec::new();
    let mut in_a = false;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().into_inner());
                match local {
                    b"nav" => {
                        nav_open_count += 1;
                        if !in_toc_nav && is_toc_nav(e) {
                            in_toc_nav = true;
                            entry_depth = Some(nav_open_count);
                        }
                    }
                    b"li" if in_toc_nav => {
                        li_stack.push(empty_nav_item());
                    }
                    b"a" if in_toc_nav && !li_stack.is_empty() => {
                        set_top_href(&mut li_stack, e, toc_dir);
                        in_a = true;
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) if in_toc_nav => {
                let local = local_name(e.name().into_inner());
                if local == b"a" && !li_stack.is_empty() {
                    set_top_href(&mut li_stack, e, toc_dir);
                }
            }
            Ok(Event::Text(t)) if in_a => {
                if let Ok(s) = t.decode() {
                    current_text.push_str(&s);
                }
            }
            Ok(Event::GeneralRef(r)) if in_a => {
                if let Some(s) = resolve_xml_entity(r.as_ref()) {
                    current_text.push_str(&s);
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().into_inner());
                if in_toc_nav {
                    match local {
                        b"a" if in_a => {
                            if let Some(top) = li_stack.last_mut() {
                                if top.title.is_empty() {
                                    top.title = current_text.trim().to_string();
                                }
                            }
                            current_text.clear();
                            in_a = false;
                        }
                        b"li" => {
                            if let Some(item) = li_stack.pop() {
                                if let Some(parent) = li_stack.last_mut() {
                                    parent.children.push(item);
                                } else {
                                    roots.push(item);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if local == b"nav" {
                    if Some(nav_open_count) == entry_depth {
                        in_toc_nav = false;
                        entry_depth = None;
                    }
                    nav_open_count -= 1;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    roots
}

fn parse_ncx(bytes: &[u8], toc_dir: &str) -> Vec<NavItem> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut in_nav_map = false;
    let mut stack: Vec<NavItem> = Vec::new();
    let mut roots: Vec<NavItem> = Vec::new();
    let mut in_nav_label = false;
    let mut in_text = false;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = local_name(e.name().into_inner());
                if local == b"navMap" {
                    in_nav_map = true;
                } else if in_nav_map {
                    match local {
                        b"navPoint" => stack.push(empty_nav_item()),
                        b"navLabel" => in_nav_label = true,
                        b"text" if in_nav_label => {
                            in_text = true;
                            current_text.clear();
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Empty(ref e)) if in_nav_map => {
                let local = local_name(e.name().into_inner());
                if local == b"content" {
                    if let Some(src) = attr_value(e, b"src") {
                        if let Some(top) = stack.last_mut() {
                            if top.href.is_empty() {
                                top.href = resolve_within(toc_dir, &src);
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(t)) if in_text => {
                if let Ok(s) = t.decode() {
                    current_text.push_str(&s);
                }
            }
            Ok(Event::GeneralRef(r)) if in_text => {
                if let Some(s) = resolve_xml_entity(r.as_ref()) {
                    current_text.push_str(&s);
                }
            }
            Ok(Event::End(ref e)) => {
                let local = local_name(e.name().into_inner());
                if in_nav_map {
                    match local {
                        b"navMap" => in_nav_map = false,
                        b"text" if in_text => {
                            if let Some(top) = stack.last_mut() {
                                if top.title.is_empty() {
                                    top.title = current_text.trim().to_string();
                                }
                            }
                            current_text.clear();
                            in_text = false;
                        }
                        b"navLabel" => in_nav_label = false,
                        b"navPoint" => {
                            if let Some(item) = stack.pop() {
                                if let Some(parent) = stack.last_mut() {
                                    parent.children.push(item);
                                } else {
                                    roots.push(item);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    roots
}

fn empty_nav_item() -> NavItem {
    NavItem {
        title: String::new(),
        href: String::new(),
        children: Vec::new(),
    }
}

fn set_top_href(li_stack: &mut [NavItem], e: &quick_xml::events::BytesStart<'_>, toc_dir: &str) {
    if let Some(href) = attr_value(e, b"href") {
        if let Some(top) = li_stack.last_mut() {
            if top.href.is_empty() {
                top.href = resolve_within(toc_dir, &href);
            }
        }
    }
}

fn is_toc_nav(e: &quick_xml::events::BytesStart<'_>) -> bool {
    for attr in e.attributes().with_checks(false).flatten() {
        let key = attr.key.as_ref();
        if local_name(key) == b"type" {
            if let Ok(v) = attr.unescape_value() {
                if v.split_whitespace().any(|p| p == "toc") {
                    return true;
                }
            }
        }
    }
    false
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == name {
            if let Ok(v) = attr.unescape_value() {
                return Some(v.into_owned());
            }
        }
    }
    None
}

fn local_name(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == b':') {
        Some(idx) => &bytes[idx + 1..],
        None => bytes,
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

fn parent_dir(opf_relative_href: &str) -> String {
    let stripped = opf_relative_href.split('#').next().unwrap_or("");
    match stripped.rfind('/') {
        Some(idx) => stripped[..=idx].to_string(),
        None => String::new(),
    }
}

fn resolve_within(toc_dir: &str, href: &str) -> String {
    let stripped = href.split('#').next().unwrap_or("");
    let fragment = href.split_once('#').map(|(_, f)| f);

    let combined = if stripped.starts_with('/') || toc_dir.is_empty() {
        stripped.to_string()
    } else {
        format!("{}{}", toc_dir, stripped)
    };

    let normalized = normalize_path(&combined);
    match fragment {
        Some(f) if !f.is_empty() => format!("{}#{}", normalized, f),
        _ => normalized,
    }
}

fn resolve_archive_path(opf_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
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
