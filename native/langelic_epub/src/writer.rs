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

    let mut ids: HashSet<&str> = HashSet::new();
    for ch in &doc.spine {
        if !ids.insert(ch.id.as_str()) {
            return Err(AppError::DuplicateId(ch.id.clone()));
        }
        if std::str::from_utf8(&ch.data.0).is_err() {
            return Err(AppError::InvalidChapter(
                ch.id.clone(),
                "data is not valid UTF-8".to_string(),
            ));
        }
    }
    for a in &doc.assets {
        if !ids.insert(a.id.as_str()) {
            return Err(AppError::DuplicateId(a.id.clone()));
        }
    }
    Ok(())
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
///     DC elements from the document's `metadata` map.
fn patch_opf(epub_bytes: Vec<u8>, doc: &Document) -> Result<Vec<u8>, AppError> {
    let cursor = Cursor::new(&epub_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| AppError::Io(format!("reopen: {}", e)))?;

    let opf_path = find_opf_path_in_archive(&mut archive)?;

    let mut out = Vec::with_capacity(epub_bytes.len());
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut out));

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

                // mimetype is stored; other files use epub-builder's defaults.
                // We use deflated for the OPF.
                let options: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);
                writer
                    .start_file(&name, options)
                    .map_err(|e| AppError::Io(format!("start_file opf: {}", e)))?;
                writer
                    .write_all(patched.as_bytes())
                    .map_err(|e| AppError::Io(format!("write opf: {}", e)))?;
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
///     from `doc.metadata` just before `</metadata>`.
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

    // 2. Inject missing DC elements before </metadata>.
    let extra = build_extra_dc_xml(doc);
    if !extra.is_empty() {
        if let Some(idx) = result.find("</metadata>") {
            result.insert_str(idx, &extra);
        }
    }

    // 3. Fix epub-builder's ID collision: both `<dc:language>` and
    //    `<dc:creator>` use `epub-creator-N`. epubcheck treats duplicate
    //    IDs as errors. Rename the language id to `epub-lang-N`.
    result = fix_language_id_collision(&result);

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
