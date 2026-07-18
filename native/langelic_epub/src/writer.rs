//! EPUB writer backed by epub-builder.
//!
//! We always emit EPUB 3. epub-builder generates a backward-compatible
//! `toc.ncx` alongside the EPUB 3 `nav.xhtml`, so EPUB 2-only readers still
//! navigate correctly — see plan §4 "Why we always emit EPUB 3".
//!
//! File paths on the input `Document` are OPF-relative (see `reader.rs`).
//! epub-builder prepends `OEBPS/` itself, so we pass paths through unchanged.
//!
//! ## Gaps in epub-builder we work around
//!
//! epub-builder's `metadata()` only accepts: title, lang, author, description,
//! subject, license, generator, direction, toc_name. And `<dc:identifier>` is
//! always rendered as `urn:uuid:...`. To preserve `publisher`, `date`,
//! `rights`, and the original `identifier` string, we post-process the
//! generated EPUB: open the zip, rewrite the OPF, re-zip. The post-process
//! step is a pure string edit — no re-parsing of the OPF XML tree.
//!
//! The same post-process pass owns rendition layout and
//! page-progression-direction: it injects explicit `rendition:layout`
//! metadata, rewrites epub-builder's unconditional spine direction to match
//! the document (or strips it when unspecified), and orients nav.xhtml for
//! RTL. See `rewrite_opf_metadata`, `set_spine_direction`, and
//! `add_dir_and_lang_to_html`.

use crate::error::AppError;
use crate::types::{Chapter, Document, NavItem};
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, TocElement, ZipLibrary};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use uuid::Uuid;

/// Fixed namespace UUID so non-UUID identifiers round-trip to the same UUID
/// every time. (We still preserve the original identifier in the OPF via
/// post-processing; this UUID is what epub-builder stuffs into the
/// uuid-locked slot.)
const LANGELIC_NS: Uuid = Uuid::from_u128(0x4c6f_6e67_656c_6963_4550_5542_4e61_6d65);

pub fn build(doc: &Document) -> Result<Vec<u8>, AppError> {
    validate(doc)?;
    let raw = build_with_epub_builder(doc)?;
    patch_opf(raw, doc)
}

fn build_with_epub_builder(doc: &Document) -> Result<Vec<u8>, AppError> {
    let mut builder =
        EpubBuilder::new(ZipLibrary::new().map_err(|e| AppError::Io(format!("zip init: {}", e)))?)
            .map_err(|e| AppError::Io(format!("epub-builder init: {}", e)))?;

    builder.epub_version(EpubVersion::V30);
    builder.set_uuid(identifier_to_uuid(&doc.identifier));

    builder
        .metadata("title", &doc.title)
        .map_err(builder_err("title"))?;
    if let Some(lang) = &doc.language {
        builder
            .metadata("lang", lang)
            .map_err(builder_err("lang"))?;
    }
    for creator in &doc.creators {
        builder
            .metadata("author", creator)
            .map_err(builder_err("author"))?;
    }
    if let Some(description) = &doc.description {
        builder
            .metadata("description", description)
            .map_err(builder_err("description"))?;
    }

    // epub-builder reserves specific filenames under OEBPS/ for its own
    // generated artifacts. Assets colliding with those names would cause a
    // "Duplicate filename" error — skip the asset and let epub-builder's
    // generated version stand.
    let reserved: HashSet<&'static str> = ["nav.xhtml", "toc.ncx", "content.opf"]
        .iter()
        .copied()
        .collect();

    for asset in &doc.assets {
        if reserved.contains(asset.file_name.as_str()) {
            continue;
        }

        let is_cover = doc
            .cover_asset_id
            .as_deref()
            .map(|cid| cid == asset.id)
            .unwrap_or(false);

        if is_cover {
            builder
                .add_cover_image(&asset.file_name, asset.data.0.as_slice(), &asset.media_type)
                .map_err(builder_err("add_cover_image"))?;
        } else if asset.file_name == "stylesheet.css" {
            // epub-builder always writes OEBPS/stylesheet.css — use its
            // dedicated stylesheet() method so we don't double-write.
            builder
                .stylesheet(asset.data.0.as_slice())
                .map_err(builder_err("stylesheet"))?;
        } else {
            builder
                .add_resource(&asset.file_name, asset.data.0.as_slice(), &asset.media_type)
                .map_err(builder_err("add_resource"))?;
        }
    }

    add_spine_and_toc(&mut builder, &doc.spine, &doc.toc)?;

    // Intentionally skip `inline_toc()`. With it enabled, epub-builder adds
    // a generated `toc.xhtml` to the spine; since the EPUB already has a
    // linked `nav.xhtml`, that extra spine entry would bloat round-trip
    // spine counts without adding navigation value.

    let mut buf = Vec::new();
    builder
        .generate(&mut buf)
        .map_err(|e| AppError::Io(format!("generate: {}", e)))?;
    Ok(buf)
}

fn validate(doc: &Document) -> Result<(), AppError> {
    if doc.title.is_empty() {
        return Err(AppError::MissingRequiredField("title"));
    }
    if doc.identifier.is_empty() {
        return Err(AppError::MissingRequiredField("identifier"));
    }
    match doc.language.as_deref() {
        None | Some("") => return Err(AppError::MissingRequiredField("language")),
        _ => {}
    }
    // Fail loud on an unrecognised direction rather than silently dropping it —
    // a bad direction is the exact bug this feature exists to prevent.
    match doc.page_progression_direction.as_deref() {
        None | Some("rtl") | Some("ltr") => {}
        Some(other) => return Err(AppError::InvalidPageDirection(other.to_string())),
    }
    match doc.rendition_layout.as_deref() {
        None | Some("pre-paginated") | Some("reflowable") => {}
        Some(other) => return Err(AppError::InvalidRenditionLayout(other.to_string())),
    }

    let mut ids: HashSet<&str> = HashSet::new();
    for ch in &doc.spine {
        if !ids.insert(ch.id.as_str()) {
            return Err(AppError::DuplicateId(ch.id.clone()));
        }
        let chapter_data = std::str::from_utf8(&ch.data.0).map_err(|_| {
            AppError::InvalidChapter(ch.id.clone(), "data is not valid UTF-8".to_string())
        })?;
        if doc.rendition_layout.as_deref() == Some("pre-paginated")
            && ch.media_type == "application/xhtml+xml"
            && !has_viewport_meta(chapter_data)
        {
            return Err(AppError::MissingViewport(ch.id.clone()));
        }
    }
    for a in &doc.assets {
        if !ids.insert(a.id.as_str()) {
            return Err(AppError::DuplicateId(a.id.clone()));
        }
    }
    Ok(())
}

/// Check an XHTML string for a viewport `<meta>` declaration without parsing
/// XML. Attribute order is irrelevant, and both XML quote styles are accepted.
fn has_viewport_meta(xhtml: &str) -> bool {
    let mut search_from = 0;
    while let Some(rel_start) = xhtml[search_from..].find("<meta") {
        let start = search_from + rel_start;
        let Some(rel_end) = xhtml[start..].find('>') else {
            return false;
        };
        let end = start + rel_end + 1;
        let tag = &xhtml[start..end];
        if tag.contains("name=\"viewport\"") || tag.contains("name='viewport'") {
            return true;
        }
        search_from = end;
    }
    false
}

fn add_spine_and_toc(
    builder: &mut EpubBuilder<ZipLibrary>,
    spine: &[Chapter],
    toc: &[NavItem],
) -> Result<(), AppError> {
    let toc_by_href: HashMap<&str, &NavItem> = toc
        .iter()
        .filter(|n| !n.href.is_empty())
        .map(|n| (n.href.as_str(), n))
        .collect();

    for (index, chapter) in spine.iter().enumerate() {
        let mut content = EpubContent::new(&chapter.file_name, chapter.data.0.as_slice());

        let nav_match = toc_by_href
            .get(chapter.file_name.as_str())
            .copied()
            .or_else(|| {
                toc.iter().find(|n| {
                    !n.href.is_empty()
                        && n.href
                            .split('#')
                            .next()
                            .map(|h| h == chapter.file_name.as_str())
                            .unwrap_or(false)
                })
            });

        // Always set a title. If neither the Chapter struct nor the TOC gives
        // us one, fall back to a positional title so epub-builder's generated
        // nav.xhtml / toc.ncx have at least one entry each (empty nav
        // structures fail epubcheck RSC-005).
        let title = chapter
            .title
            .clone()
            .or_else(|| nav_match.map(|n| n.title.clone()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Chapter {}", index + 1));

        content = content.title(title);

        if let Some(nav) = nav_match {
            for child in &nav.children {
                if let Some(el) = nav_item_to_toc(child) {
                    content = content.child(el);
                }
            }
        }

        builder
            .add_content(content)
            .map_err(|e| AppError::Io(format!("add_content({}): {}", chapter.file_name, e)))?;
    }
    Ok(())
}

fn nav_item_to_toc(nav: &NavItem) -> Option<TocElement> {
    if nav.href.is_empty() {
        return None;
    }
    let mut el = TocElement::new(&nav.href, &nav.title);
    for child in &nav.children {
        if let Some(c) = nav_item_to_toc(child) {
            el = el.child(c);
        }
    }
    Some(el)
}

fn builder_err(context: &'static str) -> impl Fn(epub_builder::Error) -> AppError {
    move |e| AppError::Io(format!("{}: {}", context, e))
}

fn identifier_to_uuid(identifier: &str) -> Uuid {
    let trimmed = identifier
        .strip_prefix("urn:uuid:")
        .unwrap_or(identifier)
        .trim();
    if let Ok(u) = Uuid::parse_str(trimmed) {
        return u;
    }
    Uuid::new_v5(&LANGELIC_NS, identifier.as_bytes())
}

/// Open the generated EPUB, rewrite OPF to:
///   * replace epub-builder's urn:uuid identifier with the original identifier
///     (so round-trips preserve the identifier exactly);
///   * inject `<dc:publisher>`, `<dc:date>`, `<dc:rights>`, and any custom
///     DC elements from the document's `metadata` map;
///   * inject `<meta property="rendition:layout">` when requested;
///   * set (or strip) the `<spine>` `page-progression-direction` attribute to
///     match `doc.page_progression_direction`.
///
/// The generated `nav.xhtml` is also rewritten:
///   * an empty `<nav epub:type="landmarks">` wrapper is stripped (epubcheck
///     RSC-005 otherwise);
///   * when the direction is `"rtl"`, the root `<html>` gains `dir="rtl"` and
///     the document language — epub-builder's nav template emits neither, so
///     RTL TOC labels would otherwise render LTR.
///
/// And the generated `toc.ncx` has its `playOrder` values renumbered — see
/// `renumber_ncx_play_order`.
fn patch_opf(epub_bytes: Vec<u8>, doc: &Document) -> Result<Vec<u8>, AppError> {
    let cursor = Cursor::new(&epub_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| AppError::Io(format!("reopen: {}", e)))?;

    let opf_path = find_opf_path_in_archive(&mut archive)?;
    // epub-builder writes nav.xhtml and toc.ncx next to content.opf.
    let nav_rtl = doc.page_progression_direction.as_deref() == Some("rtl");
    let nav_path = format!("{}nav.xhtml", dir_of(&opf_path));
    let ncx_path = format!("{}toc.ncx", dir_of(&opf_path));

    let mut out = Vec::with_capacity(epub_bytes.len());
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));

        // mimetype is stored; other files use epub-builder's defaults. We
        // deflate any entry we rewrite (OPF, nav.xhtml, and toc.ncx).
        let deflated: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for i in 0..archive.len() {
            let file = archive
                .by_index_raw(i)
                .map_err(|e| AppError::Io(format!("by_index_raw({}): {}", i, e)))?;
            let name = file.name().to_string();
            drop(file);

            if name == opf_path {
                let original = read_entry(&mut archive, &name)?;
                let original_str = String::from_utf8(original)
                    .map_err(|_| AppError::Io("opf not utf-8".to_string()))?;
                let patched = rewrite_opf_metadata(&original_str, doc);

                writer
                    .start_file(&name, deflated)
                    .map_err(|e| AppError::Io(format!("start_file opf: {}", e)))?;
                writer
                    .write_all(patched.as_bytes())
                    .map_err(|e| AppError::Io(format!("write opf: {}", e)))?;
            } else if name == nav_path {
                let original = read_entry(&mut archive, &name)?;
                let original_str = String::from_utf8(original)
                    .map_err(|_| AppError::Io("nav not utf-8".to_string()))?;
                // epubcheck 5.3.0 accepts a pre-paginated publication's
                // generated nav without viewport metadata when nav is not in
                // the spine, so fixed-layout builds leave its <head> alone.
                let mut patched = strip_empty_landmarks_nav(&original_str);
                if nav_rtl {
                    patched = add_dir_and_lang_to_html(&patched, "rtl", doc.language.as_deref());
                }

                writer
                    .start_file(&name, deflated)
                    .map_err(|e| AppError::Io(format!("start_file nav: {}", e)))?;
                writer
                    .write_all(patched.as_bytes())
                    .map_err(|e| AppError::Io(format!("write nav: {}", e)))?;
            } else if name == ncx_path {
                let original = read_entry(&mut archive, &name)?;
                let original_str = String::from_utf8(original)
                    .map_err(|_| AppError::Io("ncx not utf-8".to_string()))?;
                let patched = renumber_ncx_play_order(&original_str);

                writer
                    .start_file(&name, deflated)
                    .map_err(|e| AppError::Io(format!("start_file ncx: {}", e)))?;
                writer
                    .write_all(patched.as_bytes())
                    .map_err(|e| AppError::Io(format!("write ncx: {}", e)))?;
            } else {
                let file = archive
                    .by_name(&name)
                    .map_err(|e| AppError::Io(format!("by_name {}: {}", name, e)))?;
                writer
                    .raw_copy_file(file)
                    .map_err(|e| AppError::Io(format!("raw_copy_file {}: {}", name, e)))?;
            }
        }

        writer
            .finish()
            .map_err(|e| AppError::Io(format!("zip finish: {}", e)))?;
    }

    Ok(out)
}

/// Directory portion of an archive path (e.g. `"OEBPS/content.opf"` → `"OEBPS/"`).
fn dir_of(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..=idx].to_string(),
        None => String::new(),
    }
}

fn find_opf_path_in_archive(
    archive: &mut zip::ZipArchive<Cursor<&Vec<u8>>>,
) -> Result<String, AppError> {
    let container = read_entry(archive, "META-INF/container.xml")?;
    let s = std::str::from_utf8(&container)
        .map_err(|_| AppError::MalformedOpf("container.xml not utf-8".to_string()))?;
    // Very small parser: look for full-path="...".
    let needle = "full-path=\"";
    let start = s.find(needle).ok_or(AppError::MissingContainer)? + needle.len();
    let end = s[start..]
        .find('"')
        .ok_or_else(|| AppError::MalformedOpf("unterminated full-path".to_string()))?;
    Ok(s[start..start + end].to_string())
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, AppError> {
    let mut f = archive
        .by_name(name)
        .map_err(|e| AppError::Io(format!("by_name {}: {}", name, e)))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| AppError::Io(format!("read_to_end {}: {}", name, e)))?;
    Ok(buf)
}

/// Rewrite the OPF `<metadata>` section:
///   * replace the content of `<dc:identifier id="epub-id-1">...</dc:identifier>`
///     with the original identifier verbatim;
///   * inject `<dc:publisher>`, `<dc:date>`, `<dc:rights>`, and any DC elements
///     from `doc.metadata`, plus `rendition:layout` when requested, just before
///     `</metadata>`.
fn rewrite_opf_metadata(opf: &str, doc: &Document) -> String {
    let mut result = opf.to_string();

    // 1. Replace the primary identifier value.
    let id_open = "<dc:identifier id=\"epub-id-1\">";
    if let Some(start) = result.find(id_open) {
        let content_start = start + id_open.len();
        if let Some(rel_end) = result[content_start..].find("</dc:identifier>") {
            let end = content_start + rel_end;
            let escaped = xml_escape(&doc.identifier);
            result.replace_range(content_start..end, &escaped);
        }
    }

    // 2. Inject missing DC elements and explicit rendition layout metadata
    //    before </metadata>. `rendition` is a predefined EPUB 3 vocabulary, so
    //    the package needs no prefix declaration.
    let mut extra = build_extra_dc_xml(doc);
    if let Some(layout) = doc.rendition_layout.as_deref() {
        extra.push_str(&format!(
            "    <meta property=\"rendition:layout\">{}</meta>\n",
            xml_escape(layout)
        ));
    }
    if !extra.is_empty() {
        if let Some(idx) = result.find("</metadata>") {
            result.insert_str(idx, &extra);
        }
    }

    // 3. Fix epub-builder's ID collision: both `<dc:language>` and
    //    `<dc:creator>` use `epub-creator-N`. epubcheck treats duplicate
    //    IDs as errors. Rename the language id to `epub-lang-N`.
    result = fix_language_id_collision(&result);

    // 4. Set (or strip) the spine's page-progression-direction. epub-builder
    //    unconditionally emits `page-progression-direction="ltr"`; we replace
    //    it with the requested value, or drop the attribute entirely when the
    //    document leaves direction unspecified (nil), so an omitted direction
    //    reads as "reader default" rather than a hard-coded ltr.
    result = set_spine_direction(&result, doc);

    result
}

/// Rewrite the `<spine>` opening tag's `page-progression-direction`:
///   * strip whatever epub-builder emitted (always `ltr`);
///   * re-add the attribute iff `doc.page_progression_direction` is set.
///
/// Robust to any other attributes epub-builder puts on `<spine>` (e.g.
/// `toc="ncx"`), which are preserved untouched.
fn set_spine_direction(opf: &str, doc: &Document) -> String {
    let Some(spine_pos) = opf.find("<spine") else {
        return opf.to_string();
    };
    let attrs_start = spine_pos + "<spine".len();
    let Some(rel_end) = opf[attrs_start..].find('>') else {
        return opf.to_string();
    };
    let tag_end = attrs_start + rel_end; // index of the closing '>'
    let attrs = &opf[attrs_start..tag_end];

    let mut new_attrs = remove_attr(attrs, "page-progression-direction");
    // Normalise trailing whitespace left by the removal (or by epub-builder).
    while new_attrs.ends_with(char::is_whitespace) {
        new_attrs.pop();
    }
    if let Some(dir) = doc.page_progression_direction.as_deref() {
        new_attrs.push_str(&format!(" page-progression-direction=\"{}\"", dir));
    }

    let mut result = String::with_capacity(opf.len());
    result.push_str(&opf[..attrs_start]);
    result.push_str(&new_attrs);
    result.push_str(&opf[tag_end..]);
    result
}

/// Remove a `name="..."` attribute (and one run of leading whitespace) from an
/// element's attribute string. Returns the input unchanged if the attribute is
/// absent or unterminated.
fn remove_attr(attrs: &str, name: &str) -> String {
    let needle = format!("{}=\"", name);
    let Some(pos) = attrs.find(&needle) else {
        return attrs.to_string();
    };
    let val_start = pos + needle.len();
    let Some(rel_q) = attrs[val_start..].find('"') else {
        return attrs.to_string();
    };
    let end = val_start + rel_q + 1; // just past the closing quote

    // Drop leading whitespace immediately before the attribute so we don't
    // leave a double space.
    let keep_end = attrs[..pos].trim_end().len();

    let mut out = String::with_capacity(attrs.len());
    out.push_str(&attrs[..keep_end]);
    out.push_str(&attrs[end..]);
    out
}

/// Remove an **empty** `<nav epub:type="landmarks">` element from the
/// generated nav.xhtml.
///
/// epub-builder's v3 nav template unconditionally emits the landmarks wrapper,
/// even when there are no landmark entries; an empty landmarks nav fails
/// epubcheck RSC-005 ("element \"nav\" incomplete; missing required element
/// \"ol\""). We only remove a wrapper with no `<li>` entries inside — if
/// epub-builder ever emits real landmarks, they are kept verbatim.
fn strip_empty_landmarks_nav(xhtml: &str) -> String {
    let mut result = xhtml.to_string();
    let mut search_from = 0;

    while let Some(rel) = result[search_from..].find("<nav") {
        let start = search_from + rel;
        let Some(tag_rel_end) = result[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_rel_end + 1;

        if !result[start..tag_end].contains("landmarks") {
            search_from = tag_end;
            continue;
        }

        // Landmarks navs contain only an <ol> of <li> links, never a nested
        // <nav>, so the next closing tag ends this element.
        let Some(close_rel) = result[tag_end..].find("</nav>") else {
            break;
        };
        let close_end = tag_end + close_rel + "</nav>".len();

        if result[tag_end..tag_end + close_rel].contains("<li") {
            // Real landmark entries — keep the element.
            search_from = close_end;
            continue;
        }

        // Remove the element, plus its own-line indentation and trailing
        // newline, so no blank line is left behind.
        let line_start = result[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let removal_start = if result[line_start..start]
            .chars()
            .all(|c| c == ' ' || c == '\t')
        {
            line_start
        } else {
            start
        };
        let removal_end = if result[close_end..].starts_with('\n') {
            close_end + 1
        } else {
            close_end
        };

        result.replace_range(removal_start..removal_end, "");
        search_from = removal_start;
    }

    result
}

/// Renumber every `navPoint`'s `playOrder` in the generated toc.ncx.
///
/// epub-builder assigns a fresh sequential playOrder to every navPoint, so
/// when the same target file appears in the TOC more than once (common in
/// real books: a chapter listed both as a nested child and as a top-level
/// entry), the duplicates get *different* playOrder values — which epubcheck
/// rejects (RSC-005 "different playOrder values for navPoint/navTarget/
/// pageTarget that refer to same target"). The NCX rule is the opposite:
/// navPoints referring to the same content `src` must share one playOrder.
///
/// We renumber in document order, 1-based, advancing the counter only for the
/// first appearance of each `src`; repeat references reuse the first value.
/// String-level like the other rewrites here, robust to attribute order.
fn renumber_ncx_play_order(ncx: &str) -> String {
    // Opening <navPoint ...> tags, in document order.
    let mut nav_tags: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;
    while let Some(rel) = ncx[pos..].find("<navPoint") {
        let start = pos + rel;
        let Some(tag_rel_end) = ncx[start..].find('>') else {
            break;
        };
        let end = start + tag_rel_end + 1;
        nav_tags.push((start, end));
        pos = end;
    }

    // <content src="..."/> values, in document order.
    let mut srcs: Vec<&str> = Vec::new();
    pos = 0;
    while let Some(rel) = ncx[pos..].find("<content") {
        let start = pos + rel;
        let Some(tag_rel_end) = ncx[start..].find('>') else {
            break;
        };
        let end = start + tag_rel_end + 1;
        let tag = &ncx[start..end];
        if let Some(a) = tag.find("src=\"") {
            let vstart = a + "src=\"".len();
            if let Some(vlen) = tag[vstart..].find('"') {
                srcs.push(&tag[vstart..vstart + vlen]);
            }
        }
        pos = end;
    }

    // Valid NCX nests as navPoint = navLabel, content, navPoint* — each
    // navPoint's own <content> precedes any nested navPoints, so the i-th
    // opening tag pairs with the i-th <content>. If the document doesn't have
    // that shape, leave it untouched rather than scrambling playOrder.
    if nav_tags.len() != srcs.len() || nav_tags.is_empty() {
        return ncx.to_string();
    }

    let mut assigned: HashMap<&str, usize> = HashMap::new();
    let mut next = 1usize;
    let numbers: Vec<usize> = srcs
        .iter()
        .map(|s| {
            *assigned.entry(s).or_insert_with(|| {
                let n = next;
                next += 1;
                n
            })
        })
        .collect();

    let mut out = String::with_capacity(ncx.len());
    let mut cursor = 0;
    for ((start, end), n) in nav_tags.iter().zip(numbers.iter()) {
        out.push_str(&ncx[cursor..*start]);
        out.push_str(&set_play_order(&ncx[*start..*end], *n));
        cursor = *end;
    }
    out.push_str(&ncx[cursor..]);
    out
}

/// Replace (or, defensively, insert) the `playOrder` attribute value inside a
/// `<navPoint ...>` opening tag.
fn set_play_order(tag: &str, n: usize) -> String {
    let needle = "playOrder=\"";
    if let Some(a) = tag.find(needle) {
        let vstart = a + needle.len();
        if let Some(vlen) = tag[vstart..].find('"') {
            let mut out = String::with_capacity(tag.len() + 4);
            out.push_str(&tag[..vstart]);
            out.push_str(&n.to_string());
            out.push_str(&tag[vstart + vlen..]);
            return out;
        }
    }
    // epub-builder always emits playOrder; insert it if a source ever doesn't.
    format!("<navPoint playOrder=\"{}\"{}", n, &tag["<navPoint".len()..])
}

/// Inject `dir` and the document language onto the root `<html>` element of an
/// XHTML string (used to orient the generated nav.xhtml for RTL books).
/// Existing `dir`/`lang` attributes are left untouched.
fn add_dir_and_lang_to_html(xhtml: &str, dir: &str, lang: Option<&str>) -> String {
    let Some(html_pos) = xhtml.find("<html") else {
        return xhtml.to_string();
    };
    let attrs_start = html_pos + "<html".len();
    let Some(rel_end) = xhtml[attrs_start..].find('>') else {
        return xhtml.to_string();
    };
    let tag_end = attrs_start + rel_end;
    let existing = &xhtml[attrs_start..tag_end];

    let mut inject = String::new();
    if !existing.contains(" dir=") {
        inject.push_str(&format!(" dir=\"{}\"", xml_escape(dir)));
    }
    if let Some(l) = lang {
        if !existing.contains("xml:lang=") {
            inject.push_str(&format!(" xml:lang=\"{}\"", xml_escape(l)));
        }
        if !existing.contains(" lang=") {
            inject.push_str(&format!(" lang=\"{}\"", xml_escape(l)));
        }
    }
    if inject.is_empty() {
        return xhtml.to_string();
    }

    let mut result = String::with_capacity(xhtml.len() + inject.len());
    result.push_str(&xhtml[..attrs_start]);
    result.push_str(&inject);
    result.push_str(&xhtml[attrs_start..]);
    result
}

fn fix_language_id_collision(opf: &str) -> String {
    let mut out = String::with_capacity(opf.len());
    let mut cursor = 0;
    while let Some(rel) = opf[cursor..].find("<dc:language ") {
        let abs = cursor + rel;
        // Copy everything before this element.
        out.push_str(&opf[cursor..abs]);
        // Find the end of the opening tag.
        let tag_end = match opf[abs..].find('>') {
            Some(i) => abs + i + 1,
            None => {
                out.push_str(&opf[abs..]);
                return out;
            }
        };
        let tag = &opf[abs..tag_end];
        // Swap id="epub-creator-N" → id="epub-lang-N" within this tag only.
        let patched = tag.replace("id=\"epub-creator-", "id=\"epub-lang-");
        out.push_str(&patched);
        cursor = tag_end;
    }
    out.push_str(&opf[cursor..]);
    out
}

fn build_extra_dc_xml(doc: &Document) -> String {
    let mut out = String::new();
    let mut push = |tag: &str, value: &str| {
        if !value.is_empty() {
            out.push_str(&format!("    <dc:{tag}>{}</dc:{tag}>\n", xml_escape(value)));
        }
    };
    if let Some(v) = doc.publisher.as_deref() {
        push("publisher", v);
    }
    if let Some(v) = doc.date.as_deref() {
        push("date", v);
    }
    if let Some(v) = doc.rights.as_deref() {
        push("rights", v);
    }
    // Custom DC elements users kept in the metadata map (e.g. subject).
    for (key, values) in &doc.metadata {
        for value in values {
            push(key, value);
        }
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
