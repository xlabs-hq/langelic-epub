# `langelic_epub` — Implementation Plan

**Date:** 2026-04-19
**Status:** Ready to execute
**Target:** A standalone Hex package (`langelic_epub`) providing EPUB read and write via a Rustler NIF, suitable for community publication.

---

## How to use this document

This plan is written for a fresh Claude Code session that has no context from the conversation that produced it. Read top-to-bottom; everything you need is here.

The order matters:

1. **Context** explains what Langelic is and why this library exists.
2. **Spike evidence** summarises the experiments we already did so you don't repeat them.
3. **Design** is the locked-in architecture.
4. **Implementation guidance** is how to actually build each piece.
5. **Testing**, **documentation**, **distribution**, **quality gates** define what "done" means.
6. **Phased execution** is the recommended sequence with checkpoints.

The artifacts of the original spike are at `/tmp/epub-spike/` — you can re-run them for additional evidence if anything in this plan looks suspect, but you don't need to.

---

## 1. Context

### What Langelic is

Langelic is a Phoenix translation product. Existing pipelines: subtitle translation (videos) and comic translation (CBZ archives). We are now adding **document translation** for EPUB and PDF input, with EPUB and PDF output. The full design lives in `notes/2026-04-19-document-translation-design.md`.

### Why this library exists

Langelic's documents pipeline needs to:

1. **Parse uploaded EPUBs** to extract chapters, assets, metadata, and TOC into a structured form we can hand to translation workers.
2. **Generate translated EPUBs** that open correctly across Kindle, Kobo, Apple Books, Calibre, and modern browsers.

We surveyed the Elixir ecosystem and found nothing usable:

- `bupe` (the only EPUB-focused Elixir library) was last updated nine years ago and is minimal.
- `ex_ebook` is metadata-only and abandoned.
- Everything else is single-purpose (cover extraction, ex_doc themes).

We have to build this. The two realistic paths are:

- **Pure Elixir** using `:zip` + `sweet_xml`. ~600 lines per direction; we own all the EPUB-format edge cases.
- **Wrap a mature Rust crate via Rustler.** ~200 lines of NIF glue + tests; Rust crate maintainers handle the format edge cases.

We chose the Rust path. EPUB has enough format complexity (EPUB 2 vs 3 metadata, NCX vs nav.xhtml, embedded fonts, refines metadata, manifest IDs, OPF schema variants) that DIY accumulates bugs over time. A NIF is a one-time setup cost; pure Elixir is an ongoing maintenance tax.

### Why this is a separate library, not internal to Langelic

Three reasons:

1. **Reusability.** EPUB I/O is generally useful in Elixir; the gap on Hex is real. A clean library benefits the community and signals quality.
2. **Build complexity isolation.** The NIF needs CI infrastructure to produce precompiled binaries. Keeping it in its own repo means the main app's build pipeline is unaffected.
3. **Versioning.** EPUB tooling improvements should be releasable independently of Langelic application code.

The library should be designed and documented as if no one at Langelic will ever touch its code again — because if we do this right, that should be true.

---

## 2. Spike evidence

We tested four Rust crates against eight real-world EPUBs spanning publishers, eras, and complexity (novels from 1990s to 2020s, technical books, illustrated books, single-file EPUBs, Calibre-converted books).

### Crate evaluation

| Crate | License | Reader result | Writer result | Verdict |
|---|---|---|---|---|
| **lib-epub 0.3.0** | MIT | **Panics on 7/8 EPUBs** at `src/epub.rs:1149` (`unwrap()` on metadata refinement that assumes EPUB 2 always has `id` attributes — it doesn't). 1 returns parse error. | Not tested | **Rejected.** Cannot be used without upstream patches. |
| **epub 2.1.5** | **GPL-3.0** | Not tested | n/a | **Rejected.** Copyleft would force our library to GPL. |
| **iepub 1.3.4** | MIT | **Parses 8/8 cleanly.** Lazy-loads chapters and assets. Walks spine and TOC. Active (last release April 2026). | Builds valid EPUB structure but with warts (no `<dc:language>`, hardcoded Chinese label `"封面"`, duplicate `id="cover"` in manifest). | **Best reader.** Use for reading. |
| **epub-builder 0.8.3** | MPL-2.0 | n/a | **Clean output.** Includes `<dc:language>`, proper unique IDs, EPUB 3 with backward-compatible nav.xhtml + toc.ncx, landmarks, iBooks display options. Minor ID collision (`epub-creator-0` reused for language and creator) but tolerated by readers. | **Best writer.** Use for writing. |

### iepub gaps the implementation must work around

Discovered during the spike. These are real, reproducible:

1. **`book.language()` returns `None` for every EPUB tested**, even though all eight have `<dc:language>` in their OPF. Cause: iepub's internal `BookInfo` struct has no `language` field. The OPF parser silently drops it.

2. **TOC parsing is inconsistent.** "Car Hackers Handbook" returned 0 nav entries despite having a real TOC. Other EPUBs returned nav trees correctly. Cause unknown — likely a parser quirk for specific NCX/nav.xhtml structures.

3. **Quirky filename handling.** EPUBs whose internal files have multiple dots (e.g. Brandon Sanderson's "Way of Kings" has files like `Chapter1.html_split_000`) are recognized but the asset-extension classification breaks (each file becomes its own ext category). Doesn't affect reading content but affects any logic that branches on file type.

4. **Multiple `<dc:creator>` entries collapse.** The `book.creator()` method returns a single comma-joined string. Multi-author books need separate handling.

5. **`<dc:rights>` is not exposed.** Same root cause as language — not in `BookInfo`.

**Mitigation strategy:** A small `opf.rs` module on the Rust side re-parses the OPF XML (which iepub has already extracted the bytes for during open) using `quick-xml`. We extract the fields iepub drops and merge them into our `Document` struct. About 100 lines of Rust.

### epub-builder gap

One observed: when both `metadata("lang", ...)` and `metadata("author", ...)` are set, the generated XML reuses `id="epub-creator-0"` for both. Real readers (Kindle, Apple Books, Calibre) handle it without complaint, but `epubcheck` may warn. Acceptable for v0.1; report upstream as a separate workstream.

### Spike artifacts

Located at `/tmp/epub-spike/`:

```
/tmp/epub-spike/
├── samples/                   # 8 sample EPUBs from user's library
├── reader_spike/              # Cargo project that tested lib-epub (panics)
├── iepub_spike/               # Cargo project that tested iepub (works)
├── writer_spike/              # Cargo project that tested iepub writer
├── builder_spike/             # Cargo project that tested epub-builder
├── built.epub                 # Output from iepub writer
├── built_builder.epub         # Output from epub-builder
└── tinycover.png              # 1×1 PNG used as cover in spike
```

You can re-run any of these to verify a finding. Source code is intentionally simple; treat as reference, not as code to copy directly into the library.

---

## 3. Design

### Repository layout

```
langelic_epub/
├── README.md
├── CHANGELOG.md
├── LICENSE                                      # MIT
├── NOTICE                                       # third-party attributions (iepub, epub-builder)
├── .formatter.exs
├── .gitignore
├── mix.exs
├── mix.lock
├── checksum-Elixir.LangelicEpub.Native.exs      # generated by rustler_precompiled
├── lib/
│   ├── langelic_epub.ex                         # public API + moduledoc
│   └── langelic_epub/
│       ├── document.ex                          # %Document{} struct + types
│       ├── chapter.ex                           # %Chapter{} struct
│       ├── asset.ex                             # %Asset{} struct
│       ├── nav_item.ex                          # %NavItem{} struct (recursive)
│       ├── error.ex                             # error structs
│       └── native.ex                            # private NIF module
├── native/langelic_epub/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── .cargo/
│   │   └── config.toml                          # cross-compile profile config
│   └── src/
│       ├── lib.rs                               # rustler::init!, NIF entry points, panic guard
│       ├── reader.rs                            # iepub-backed parse path
│       ├── writer.rs                            # epub-builder-backed build path
│       ├── opf.rs                               # OPF re-parse layer for iepub gaps
│       ├── types.rs                             # Rust structs that mirror Elixir structs
│       └── error.rs                             # internal error type
├── test/
│   ├── test_helper.exs
│   ├── langelic_epub_test.exs                   # public API tests
│   ├── round_trip_test.exs                      # parse → modify → build → parse
│   ├── fixtures_test.exs                        # parse each fixture, assert known structure
│   └── support/
│       ├── fixtures/                            # 6-8 small EPUBs, MIT-licensed where possible
│       │   ├── minimal_epub3.epub
│       │   ├── minimal_epub2.epub
│       │   ├── multi_chapter.epub
│       │   ├── with_cover.epub
│       │   ├── with_nested_toc.epub
│       │   ├── with_embedded_fonts.epub
│       │   ├── with_images_and_css.epub
│       │   └── multi_creator.epub
│       └── fixtures.ex                          # path helpers
└── .github/
    ├── workflows/
    │   ├── ci.yml                               # mix test + cargo test on push
    │   └── release.yml                          # build precompiled NIFs + GitHub release
    └── dependabot.yml
```

### Public Elixir API

The entire public surface:

```elixir
defmodule LangelicEpub do
  @moduledoc """
  EPUB read and write for Elixir.

  ## Reading

      {:ok, doc} = LangelicEpub.parse(File.read!("book.epub"))
      doc.title         # => "The Hobbit"
      doc.language      # => "en"
      Enum.count(doc.spine)  # => 23

  ## Writing

      doc = %LangelicEpub.Document{
        title: "Translated Title",
        language: "th",
        creators: ["Original Author"],
        identifier: "urn:uuid:#{Ecto.UUID.generate()}",
        spine: [
          %LangelicEpub.Chapter{
            id: "ch1",
            file_name: "ch1.xhtml",
            title: "บทที่ 1",
            media_type: "application/xhtml+xml",
            data: chapter_xhtml
          }
        ],
        assets: [],
        toc: [],
        version: "3.0"
      }

      {:ok, bytes} = LangelicEpub.build(doc)
      File.write!("translated.epub", bytes)
  """

  alias LangelicEpub.{Document, Error}

  @doc """
  Parse EPUB bytes into a `LangelicEpub.Document`.

  Accepts the raw bytes of an EPUB file. All chapters and assets are loaded
  into memory; do not call this on documents larger than your available memory.

  ## Examples

      iex> {:ok, doc} = LangelicEpub.parse(File.read!("test/support/fixtures/minimal_epub3.epub"))
      iex> doc.title
      "Minimal EPUB 3"

  ## Errors

  Returns `{:error, %LangelicEpub.Error{}}` for:

    * `:invalid_zip` — bytes are not a valid ZIP archive
    * `:missing_container` — no `META-INF/container.xml`
    * `:missing_opf` — OPF file referenced in container.xml not found
    * `:malformed_opf` — OPF could not be parsed
    * `{:io, reason}` — internal I/O failure
    * `:panic` — Rust side panicked (this should never happen; report a bug)
  """
  @spec parse(binary()) :: {:ok, Document.t()} | {:error, Error.t()}
  def parse(epub_bytes) when is_binary(epub_bytes) do
    LangelicEpub.Native.parse(epub_bytes)
  end

  @doc """
  Build a `LangelicEpub.Document` into EPUB bytes.

  The generated EPUB is always EPUB 3 with backward-compatible `toc.ncx`.

  ## Examples

      iex> doc = %LangelicEpub.Document{title: "Hello", language: "en", identifier: "id-1", creators: ["Me"], spine: [], assets: [], toc: [], version: "3.0"}
      iex> {:ok, bytes} = LangelicEpub.build(doc)
      iex> byte_size(bytes) > 0
      true

  ## Errors

  Returns `{:error, %LangelicEpub.Error{}}` for:

    * `:missing_required_field` — title, identifier, or language is missing
    * `:invalid_chapter` — a chapter's data is not valid UTF-8 or XHTML
    * `:duplicate_id` — two chapters or assets share the same `id`
    * `:panic` — Rust side panicked (report a bug)
  """
  @spec build(Document.t()) :: {:ok, binary()} | {:error, Error.t()}
  def build(%Document{} = doc) do
    LangelicEpub.Native.build(doc)
  end
end
```

### Data structures

Each is a separate module with its own moduledoc, typespec, and `@type t :: %__MODULE__{...}`.

```elixir
defmodule LangelicEpub.Document do
  @moduledoc """
  An EPUB document — metadata, spine (reading order), assets, and TOC.

  All chapters and assets are fully loaded into memory.
  """

  @type t :: %__MODULE__{
          title: String.t(),
          creators: [String.t()],
          language: String.t() | nil,
          identifier: String.t(),
          publisher: String.t() | nil,
          date: String.t() | nil,
          description: String.t() | nil,
          rights: String.t() | nil,
          metadata: %{String.t() => [String.t()]},
          spine: [LangelicEpub.Chapter.t()],
          assets: [LangelicEpub.Asset.t()],
          toc: [LangelicEpub.NavItem.t()],
          cover_asset_id: String.t() | nil,
          version: String.t()
        }

  defstruct [
    :title,
    :creators,
    :language,
    :identifier,
    :publisher,
    :date,
    :description,
    :rights,
    metadata: %{},
    spine: [],
    assets: [],
    toc: [],
    cover_asset_id: nil,
    version: "3.0"
  ]
end

defmodule LangelicEpub.Chapter do
  @moduledoc """
  A chapter in the spine. The `data` field contains the raw XHTML bytes.
  """

  @type t :: %__MODULE__{
          id: String.t(),
          file_name: String.t(),
          title: String.t() | nil,
          media_type: String.t(),
          data: binary()
        }

  defstruct [:id, :file_name, :title, :media_type, :data]
end

defmodule LangelicEpub.Asset do
  @moduledoc """
  A non-chapter resource: CSS, font, image, or other.
  """

  @type t :: %__MODULE__{
          id: String.t(),
          file_name: String.t(),
          media_type: String.t(),
          data: binary()
        }

  defstruct [:id, :file_name, :media_type, :data]
end

defmodule LangelicEpub.NavItem do
  @moduledoc """
  A node in the table of contents. May contain nested children.
  """

  @type t :: %__MODULE__{
          title: String.t(),
          href: String.t(),
          children: [t()]
        }

  defstruct [:title, :href, children: []]
end

defmodule LangelicEpub.Error do
  @moduledoc """
  Error returned from `LangelicEpub.parse/1` or `LangelicEpub.build/1`.

  The `kind` field is an atom or atom-tagged tuple identifying the error class.
  The `message` field is a human-readable string suitable for logging.
  """

  @type t :: %__MODULE__{
          kind: atom() | {atom(), term()},
          message: String.t()
        }

  defexception [:kind, :message]

  @impl true
  def message(%__MODULE__{message: m}), do: m
end
```

### Error model

The Rust side returns `{:error, kind, message}` tuples that the NIF wrapper converts into `%LangelicEpub.Error{}` structs. Each error kind is documented in the moduledoc of `LangelicEpub`.

The error struct is also an exception, so callers can use `raise/1` if they prefer:

```elixir
{:ok, doc} = LangelicEpub.parse(bytes) || raise "couldn't parse"
```

### NIF surface

Two functions, both `DirtyCpu` scheduler:

```rust
#[rustler::nif(schedule = "DirtyCpu")]
fn parse<'a>(env: Env<'a>, epub_bytes: Binary<'a>) -> NifResult<Term<'a>> {
    catch_panic(|| reader::parse(epub_bytes.as_slice()))
        .map(|doc| doc.encode(env))
        .or_else(|err| Ok(err.encode(env)))
}

#[rustler::nif(schedule = "DirtyCpu")]
fn build<'a>(env: Env<'a>, doc: types::Document) -> NifResult<Term<'a>> {
    catch_panic(|| writer::build(&doc))
        .map(|bytes| {
            let mut bin = OwnedBinary::new(bytes.len()).unwrap();
            bin.as_mut_slice().copy_from_slice(&bytes);
            (atoms::ok(), Binary::from_owned(bin, env)).encode(env)
        })
        .or_else(|err| Ok(err.encode(env)))
}

rustler::init!("Elixir.LangelicEpub.Native");
```

`DirtyCpu` because:

- A 5MB EPUB takes ~50–200ms to parse, well above the 1ms NIF guideline.
- Both operations are pure CPU on bytes already in memory; no I/O.

`catch_panic` wraps `std::panic::catch_unwind` and converts any panic into a `LangelicEpub.Error{kind: :panic, ...}` so a buggy EPUB cannot crash the BEAM node. This is critical for production safety.

---

## 4. Implementation guidance

### Order of operations

1. Set up the empty Rust crate with `rustler`, `iepub`, `epub-builder`, `quick-xml`, `thiserror`.
2. Define `types.rs` with Rust structs that derive `NifStruct` to map to/from Elixir structs.
3. Build `reader.rs` first. It's simpler and lets you validate the data flow end-to-end.
4. Build `opf.rs` next, called from `reader.rs` to fill iepub's gaps.
5. Build `writer.rs`.
6. Wire up `lib.rs` with NIF entry points and panic guard.
7. Build the Elixir side.
8. Tests, docs, CI, release.

### Reader strategy

```rust
// native/langelic_epub/src/reader.rs

use crate::{opf, types::*, error::ParseError};
use iepub::prelude::*;

pub fn parse(bytes: &[u8]) -> Result<Document, ParseError> {
    // 1. Parse with iepub.
    let mut book = read_from_vec(bytes.to_vec())
        .map_err(ParseError::from_iepub)?;

    // 2. Force-load all chapter and asset bytes (iepub is lazy by default).
    //    This ensures the returned Document is fully self-contained — no
    //    references back into iepub state.
    for ch in book.chapters_mut() {
        let _ = ch.data_mut();
    }
    for a in book.assets_mut() {
        let _ = a.data_mut();
    }

    // 3. Re-parse the OPF for fields iepub drops.
    let opf_bytes = opf::extract_from_zip(bytes)?;
    let extras = opf::parse_extras(&opf_bytes)?;

    // 4. Convert to our Document type, merging extras for fields iepub
    //    didn't populate.
    Ok(build_document(book, extras))
}

fn build_document(book: EpubBook, extras: opf::OpfExtras) -> Document {
    Document {
        title: book.title().to_string(),
        creators: extras.creators.clone(),
        language: extras.language,
        identifier: book.identifier().to_string(),
        publisher: book.publisher().map(String::from),
        date: book.date().map(String::from),
        description: book.description().map(String::from),
        rights: extras.rights,
        metadata: collect_other_metadata(&book, &extras),
        spine: book.chapters().map(convert_chapter).collect(),
        assets: book.assets().map(convert_asset).collect(),
        toc: book.nav().map(convert_nav).collect(),
        cover_asset_id: book.cover().map(|c| c.file_name().to_string()),
        version: book.version().to_string(),
    }
}
```

Notes for the implementer:

- iepub's `EpubBook` exposes some fields via methods (`title()`, `creator()`, `publisher()`) and others only via the lower-level `meta()` or struct-internal access. Read `~/.cargo/registry/src/index.crates.io-*/iepub-1.3.4/src/epub/core.rs` to discover the actual API — the docs.rs page is incomplete.
- `book.creator()` returns a single comma-joined string for multi-author books. Use `extras.creators` from OPF re-parse instead, which gives a proper `Vec<String>`.
- iepub stores chapter content as XHTML bytes. We expose them as-is — the consumer is responsible for any XHTML parsing or modification. Don't try to be clever.

### OPF re-parse layer

```rust
// native/langelic_epub/src/opf.rs

use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Cursor;
use std::io::Read;
use zip::ZipArchive;

pub struct OpfExtras {
    pub language: Option<String>,
    pub creators: Vec<String>,        // multiple authors supported
    pub rights: Option<String>,
    pub other_dc: Vec<(String, String)>,  // dc:subject, dc:contributor, etc.
}

pub fn extract_from_zip(epub_bytes: &[u8]) -> Result<Vec<u8>, ParseError> {
    let mut archive = ZipArchive::new(Cursor::new(epub_bytes))
        .map_err(|e| ParseError::InvalidZip(e.to_string()))?;

    // Read META-INF/container.xml to find the OPF path.
    let opf_path = {
        let mut container = archive.by_name("META-INF/container.xml")
            .map_err(|_| ParseError::MissingContainer)?;
        let mut s = String::new();
        container.read_to_string(&mut s)
            .map_err(|e| ParseError::Io(e.to_string()))?;
        find_opf_path(&s)?
    };

    let mut opf = archive.by_name(&opf_path)
        .map_err(|_| ParseError::MissingOpf(opf_path))?;
    let mut buf = Vec::new();
    opf.read_to_end(&mut buf).map_err(|e| ParseError::Io(e.to_string()))?;
    Ok(buf)
}

pub fn parse_extras(opf_bytes: &[u8]) -> Result<OpfExtras, ParseError> {
    // Walk the metadata section, collecting dc:* elements.
    // Return what we found; missing fields stay None / empty.
    // Implementation: ~50 lines of quick-xml event-stream parsing.
    todo!()
}

fn find_opf_path(container_xml: &str) -> Result<String, ParseError> {
    // Find <rootfile full-path="..." /> in container.xml.
    todo!()
}
```

Why we use `quick-xml` directly rather than relying on iepub's parse:

- iepub already has a parsed OPF internally but doesn't expose it.
- We could fork iepub, but that's a maintenance burden.
- `quick-xml` is in iepub's dependency tree already, so we don't add a new transitive dep.
- The OPF metadata section is a tiny, well-defined XML fragment. Re-parsing it is cheap.

### Writer strategy

```rust
// native/langelic_epub/src/writer.rs

use crate::{types::*, error::BuildError};
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, ZipLibrary, TocElement};

pub fn build(doc: &Document) -> Result<Vec<u8>, BuildError> {
    validate(doc)?;

    let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;
    builder.epub_version(EpubVersion::V30);

    // Required metadata
    builder.metadata("title", &doc.title)?;
    builder.metadata("identifier", &doc.identifier)?;
    if let Some(lang) = &doc.language {
        builder.metadata("lang", lang)?;
    }

    // Optional metadata
    for creator in &doc.creators {
        builder.metadata("author", creator)?;
    }
    if let Some(publisher) = &doc.publisher {
        builder.metadata("publisher", publisher)?;
    }
    if let Some(description) = &doc.description {
        builder.metadata("description", description)?;
    }
    if let Some(rights) = &doc.rights {
        builder.metadata("rights", rights)?;
    }

    // Assets — special-case CSS and the cover image
    for asset in &doc.assets {
        if asset.media_type == "text/css" {
            builder.stylesheet(asset.data.as_slice())?;
        } else if Some(&asset.id) == doc.cover_asset_id.as_ref() {
            builder.add_cover_image(&asset.file_name, asset.data.as_slice(), &asset.media_type)?;
        } else {
            builder.add_resource(&asset.file_name, asset.data.as_slice(), &asset.media_type)?;
        }
    }

    // Spine + TOC
    add_spine_and_toc(&mut builder, &doc.spine, &doc.toc)?;

    builder.inline_toc();
    let mut buf = Vec::new();
    builder.generate(&mut buf)?;
    Ok(buf)
}

fn validate(doc: &Document) -> Result<(), BuildError> {
    if doc.title.is_empty() {
        return Err(BuildError::MissingRequiredField("title"));
    }
    if doc.identifier.is_empty() {
        return Err(BuildError::MissingRequiredField("identifier"));
    }
    if doc.language.as_deref().unwrap_or("").is_empty() {
        return Err(BuildError::MissingRequiredField("language"));
    }
    let mut ids = std::collections::HashSet::new();
    for ch in &doc.spine {
        if !ids.insert(&ch.id) {
            return Err(BuildError::DuplicateId(ch.id.clone()));
        }
    }
    for a in &doc.assets {
        if !ids.insert(&a.id) {
            return Err(BuildError::DuplicateId(a.id.clone()));
        }
    }
    Ok(())
}

fn add_spine_and_toc(
    builder: &mut EpubBuilder<ZipLibrary>,
    spine: &[Chapter],
    toc: &[NavItem],
) -> Result<(), BuildError> {
    // Walk the spine. For each chapter, look up its TOC entry (if any) and
    // attach it via .title() and any nested children via .child().
    // If the toc list is empty or a chapter has no matching entry, add the
    // chapter without TOC metadata — it'll appear in the spine but not nav.
    // Implementation: ~40 lines.
    todo!()
}
```

Why we always emit EPUB 3:

- It is the current standard.
- All modern readers handle it.
- epub-builder includes a backward-compatible `toc.ncx` automatically, so EPUB 2-only readers still navigate.
- Emitting both EPUB 2 and EPUB 3 from a config flag would double the test surface for marginal benefit.

### Panic safety

```rust
// native/langelic_epub/src/lib.rs

fn catch_panic<T>(f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("rust panic: {}", s)
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("rust panic: {}", s)
            } else {
                "rust panic with non-string payload".to_string()
            };
            Err(AppError::Panic(msg))
        }
    }
}
```

This is non-negotiable. Without it, a buggy EPUB that triggers an unwrap somewhere in iepub will crash the BEAM scheduler thread, which can cascade into application-wide failure.

We test this explicitly: include a known-bad fixture (the kind of EPUB that crashed lib-epub) and assert that `parse/1` returns `{:error, _}` rather than crashing the test process.

### Dependencies

```toml
# native/langelic_epub/Cargo.toml

[package]
name = "langelic_epub"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
license = "MIT"
description = "Rustler NIF for EPUB read/write"

[lib]
crate-type = ["cdylib"]

[dependencies]
rustler = "0.37"
iepub = "1.3.4"
epub-builder = "0.8.3"
quick-xml = "0.39"
zip = "2.2"          # for the OPF re-parse zip read
thiserror = "2"

[profile.release]
strip = true
lto = "thin"
opt-level = 3
```

```elixir
# mix.exs

defmodule LangelicEpub.MixProject do
  use Mix.Project

  @version "0.1.0"
  @source_url "https://github.com/<your-org>/langelic_epub"

  def project do
    [
      app: :langelic_epub,
      version: @version,
      elixir: "~> 1.15",
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      package: package(),
      docs: docs(),
      description: "EPUB read and write for Elixir, backed by a Rustler NIF.",
      source_url: @source_url
    ]
  end

  def application, do: [extra_applications: [:logger]]

  defp deps do
    [
      {:rustler_precompiled, "~> 0.8"},
      {:rustler, "~> 0.37", optional: true},
      {:ex_doc, "~> 0.34", only: :dev, runtime: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false}
    ]
  end

  defp package do
    [
      files: ~w(
        lib
        native/langelic_epub/src
        native/langelic_epub/Cargo.toml
        native/langelic_epub/.cargo
        checksum-*.exs
        mix.exs
        README.md
        CHANGELOG.md
        LICENSE
        NOTICE
      ),
      licenses: ["MIT"],
      links: %{
        "GitHub" => @source_url,
        "Changelog" => "#{@source_url}/blob/main/CHANGELOG.md"
      }
    ]
  end

  defp docs do
    [
      main: "LangelicEpub",
      extras: ["README.md", "CHANGELOG.md"],
      source_ref: "v#{@version}"
    ]
  end
end
```

`{:rustler, optional: true}` is the standard rustler_precompiled pattern: precompiled binaries are used by default, and a local Rust toolchain compile is the fallback only when explicitly requested.

---

## 5. Testing strategy

### Fixture management

Curate 6–8 small EPUBs (under 200KB each) that exercise specific features. Do **not** include the user's personal Dropbox EPUBs — those have unclear licenses. Create or commission CC0/MIT-licensed test EPUBs:

- `minimal_epub3.epub` — single chapter, EPUB 3, minimum required metadata
- `minimal_epub2.epub` — single chapter, EPUB 2 with NCX
- `multi_chapter.epub` — 5 chapters, flat TOC
- `with_cover.epub` — has a cover image
- `with_nested_toc.epub` — TOC with at least 2 levels of nesting
- `with_embedded_fonts.epub` — embeds a font (use a freely licensed one like Inter)
- `with_images_and_css.epub` — chapter references CSS and inline images
- `multi_creator.epub` — two `<dc:creator>` entries

Generate them with `pandoc` or by hand-crafting the XML. Document the source of each in `test/support/fixtures/README.md`.

**One additional fixture:** `crashes_libepub.epub`. This is one of the EPUBs that panicked lib-epub (use Confederacy of Dunces as the model — the issue was a `<dc:contributor opf:role="bkp">` without an `id` attribute, which iepub handles fine but lib-epub doesn't). Use this to verify our panic guard works even though we don't expect iepub to panic on it.

### Test categories

**Unit tests (Elixir, `mix test`):**

Per fixture, parse and assert:

- `doc.title` matches expected
- `doc.language` matches expected (this catches the OPF re-parse working)
- `doc.creators` has expected count and values (multi-creator fixture)
- `doc.spine` has expected count
- `doc.toc` has expected structure (depth, child counts)
- Each chapter's `data` is non-empty and is parseable XHTML
- `doc.cover_asset_id` is set when expected

**Round-trip tests:**

```elixir
test "parse → modify → build → parse preserves structure" do
  original = LangelicEpub.parse(File.read!(@multi_chapter)) |> elem(1)

  # Modify a chapter's text
  modified = update_in(original.spine, fn spine ->
    Enum.map(spine, fn ch ->
      %{ch | data: String.replace(ch.data, "old", "new")}
    end)
  end)

  {:ok, bytes} = LangelicEpub.build(modified)
  {:ok, reparsed} = LangelicEpub.parse(bytes)

  assert reparsed.title == original.title
  assert reparsed.language == original.language
  assert length(reparsed.spine) == length(original.spine)
  assert reparsed.spine |> hd() |> Map.get(:data) =~ "new"
  assert reparsed.spine |> hd() |> Map.get(:data) =~ "new"
end
```

**Error path tests:**

- `LangelicEpub.parse(<<>>)` returns `{:error, %{kind: :invalid_zip}}`
- `LangelicEpub.parse(<<"PK", random_garbage::binary>>)` returns `{:error, _}` without crashing
- `LangelicEpub.build(%Document{title: ""})` returns `{:error, %{kind: :missing_required_field}}`
- `LangelicEpub.build(%Document{spine: [%Chapter{id: "x", ...}, %Chapter{id: "x", ...}]})` returns `{:error, %{kind: :duplicate_id}}`

**Panic safety test:**

- A fixture known to be malformed in a way that could cause panics (manipulate one of the fixtures: corrupt the OPF mid-element). Assert `LangelicEpub.parse/1` returns `{:error, %{kind: :panic}}` instead of crashing the test process.

**External validation (CI only):**

- Run [epubcheck](https://github.com/w3c/epubcheck) against every EPUB built by the round-trip tests. Fail CI on any reported errors.
- epubcheck is a Java tool; install via `apt-get install epubcheck` on the GitHub Actions runner.

### Property-based tests (optional but recommended)

Use `stream_data` to generate random `%Document{}` values within constraints (valid identifiers, non-empty title, valid language code, sensible chapter counts) and assert that `parse(build(doc))` is equivalent to `doc` for the fields the EPUB format preserves.

---

## 6. Documentation requirements

The library is for community use. Documentation matters as much as code.

### README.md

Sections, in order:

1. **One-paragraph description** — what it does, what it doesn't.
2. **Installation** — `{:langelic_epub, "~> 0.1"}` snippet, note about precompiled binaries.
3. **Quick start** — three-block example: parse, modify, build.
4. **Why this library exists** — short paragraph mentioning the gap on Hex, the choice to wrap Rust crates, the credits to iepub and epub-builder.
5. **Supported EPUB features** — a table:

   | Feature | Read | Write |
   |---|---|---|
   | EPUB 2 | ✓ | ✗ (always emit EPUB 3) |
   | EPUB 3 | ✓ | ✓ |
   | Multiple creators | ✓ | ✓ |
   | NCX TOC | ✓ | ✓ |
   | nav.xhtml TOC | ✓ | ✓ |
   | Embedded fonts | ✓ | ✓ |
   | Embedded images | ✓ | ✓ |
   | Embedded CSS | ✓ | ✓ |
   | Cover image | ✓ | ✓ |
   | DRM | detected, not decrypted | n/a |
   | MOBI | ✗ | ✗ |

6. **Limitations and known issues** — be honest about TOC parsing edge cases, the iepub multi-creator workaround, etc.
7. **Architecture** — one-paragraph "this is a Rustler NIF wrapping iepub for reading and epub-builder for writing."
8. **Building from source** — Rust toolchain requirements for users without precompiled binaries.
9. **Contributing** — link to CONTRIBUTING.md.
10. **License + acknowledgements** — MIT + credit upstream Rust crates.

### Module documentation

Every module has a `@moduledoc` that:

- States its purpose in one sentence.
- Shows a usage example.
- Lists the most important types and functions.

Every public function has a `@doc` that:

- States what it does in one sentence.
- Documents parameters and return type (also covered by `@spec`).
- Shows at least one example (use doctests where they don't require I/O).
- Lists possible error conditions for any function returning `{:ok, _} | {:error, _}`.

Every public type has a `@type` and `@typedoc`.

### CHANGELOG.md

Follow [Keep a Changelog](https://keepachangelog.com/) format. Initial entry:

```markdown
## [0.1.0] - 2026-04-XX

### Added
- Initial release.
- `LangelicEpub.parse/1` — parse EPUB 2 and EPUB 3 documents.
- `LangelicEpub.build/1` — generate EPUB 3 documents (with backward-compatible NCX).
- Precompiled NIFs for macOS (aarch64, x86_64) and Linux (aarch64-gnu, x86_64-gnu, x86_64-musl).
```

### Inline comments

Sparingly, only for non-obvious decisions. The major ones:

- In `reader.rs`, the comment block explaining why we re-parse the OPF.
- In `writer.rs`, the comment explaining why we always emit EPUB 3.
- In `lib.rs`, the comment explaining why we use `DirtyCpu` and `catch_unwind`.

---

## 7. Distribution & CI

### `rustler_precompiled` setup

```elixir
# lib/langelic_epub/native.ex

defmodule LangelicEpub.Native do
  @moduledoc false

  version = Mix.Project.config()[:version]

  use RustlerPrecompiled,
    otp_app: :langelic_epub,
    crate: "langelic_epub",
    base_url: "https://github.com/<your-org>/langelic_epub/releases/download/v#{version}",
    force_build: System.get_env("LANGELIC_EPUB_BUILD") in ["1", "true"],
    targets: ~w(
      aarch64-apple-darwin
      x86_64-apple-darwin
      aarch64-unknown-linux-gnu
      x86_64-unknown-linux-gnu
      x86_64-unknown-linux-musl
    ),
    nif_versions: ["2.16"],
    version: version

  def parse(_bytes), do: :erlang.nif_error(:nif_not_loaded)
  def build(_doc), do: :erlang.nif_error(:nif_not_loaded)
end
```

The user-facing `LangelicEpub` module wraps these — `Native` is `@moduledoc false`.

### CI workflow

```yaml
# .github/workflows/ci.yml

name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    name: ${{ matrix.os }} / OTP ${{ matrix.otp }} / Elixir ${{ matrix.elixir }}
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
        otp: ["26.2", "27.0"]
        elixir: ["1.16.3", "1.17.3"]

    steps:
      - uses: actions/checkout@v4

      - uses: erlef/setup-beam@v1
        with:
          otp-version: ${{ matrix.otp }}
          elixir-version: ${{ matrix.elixir }}

      - uses: actions-rs/toolchain@v1
        with:
          toolchain: 1.85.0
          override: true

      - name: Install epubcheck
        if: runner.os == 'Linux'
        run: sudo apt-get install -y epubcheck

      - name: Cache deps
        uses: actions/cache@v4
        with:
          path: |
            deps
            _build
            native/langelic_epub/target
          key: ${{ runner.os }}-deps-${{ hashFiles('mix.lock', 'native/langelic_epub/Cargo.lock') }}

      - run: mix deps.get
      - run: mix format --check-formatted
      - run: mix credo --strict
      - run: mix dialyzer
      - run: mix compile --warnings-as-errors
        env:
          LANGELIC_EPUB_BUILD: "true"
      - run: mix test
        env:
          LANGELIC_EPUB_BUILD: "true"
      - run: cargo fmt --manifest-path native/langelic_epub/Cargo.toml --check
      - run: cargo clippy --manifest-path native/langelic_epub/Cargo.toml -- -D warnings
      - run: cargo test --manifest-path native/langelic_epub/Cargo.toml
```

`LANGELIC_EPUB_BUILD=true` forces `rustler_precompiled` to compile from source (no GitHub release exists yet for the current commit during CI).

### Release workflow

Use `philss/rustler-precompiled-action` — its README is the canonical guide. The workflow runs on any tag matching `v*`:

```yaml
# .github/workflows/release.yml

name: Release

on:
  push:
    tags:
      - "v*"

jobs:
  build_release:
    name: NIF ${{ matrix.nif }} - ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        nif: ["2.16"]
        job:
          - { target: aarch64-apple-darwin,     os: macos-14 }
          - { target: x86_64-apple-darwin,      os: macos-13 }
          - { target: aarch64-unknown-linux-gnu, os: ubuntu-22.04, use-cross: true }
          - { target: x86_64-unknown-linux-gnu,  os: ubuntu-22.04 }
          - { target: x86_64-unknown-linux-musl, os: ubuntu-22.04, use-cross: true }

    steps:
      - uses: actions/checkout@v4

      - uses: philss/rustler-precompiled-action@v1.1.4
        with:
          project-name: langelic_epub
          project-version: ${{ github.ref_name }}
          target: ${{ matrix.job.target }}
          nif-version: ${{ matrix.nif }}
          use-cross: ${{ matrix.job.use-cross || false }}
          project-dir: native/langelic_epub
```

After the matrix completes, the action automatically attaches each `.so`/`.dylib` archive to the GitHub release named after the tag. `mix hex.publish` then publishes the package, and `rustler_precompiled` resolves the binary URLs at install time.

### Versioning

Strict semver:

- **Major** — public API breaking changes (`%Document{}` field added/removed/renamed, function signature changes, error kind enum changes).
- **Minor** — new features, new fields with sensible defaults, new optional metadata.
- **Patch** — bug fixes, documentation, dependency updates that don't affect behaviour.

The `version` in `mix.exs`, `Cargo.toml`, and the `@version` module attribute must always match. Add a script `bin/check_versions` and run it in CI.

### Hex publishing

```bash
# Manual release process:
# 1. Update CHANGELOG.md with the new version.
# 2. Bump version in mix.exs and native/langelic_epub/Cargo.toml.
# 3. Commit, tag, push:
git commit -am "Release v0.1.0"
git tag v0.1.0
git push origin main --tags
# 4. Wait for the release workflow to complete (check Actions tab).
# 5. Publish to Hex once binaries are attached:
mix rustler_precompiled.download LangelicEpub.Native --all --print
mix hex.publish
```

The first step of `mix rustler_precompiled.download` regenerates the `checksum-Elixir.LangelicEpub.Native.exs` file. Commit that, then `mix hex.publish`.

---

## 8. Quality gates

### Definition of done per phase

Each phase has explicit deliverables. Don't move forward until each item is checked.

### Linting and formatting

- `mix format --check-formatted` must pass.
- `mix credo --strict` with **no warnings** (configure `.credo.exs` if any rule needs to be disabled with explicit reason).
- `mix dialyzer` clean. Annotate any genuine intentional differences explicitly.
- `cargo fmt --check` must pass.
- `cargo clippy -- -D warnings` clean.

### Coverage

`mix test --cover` should report > 85% line coverage. Use `mix coveralls.html` to inspect uncovered lines and ensure they are either trivial getters or well-justified.

### Documentation

`mix docs` should produce a doc set with:

- Every public module documented.
- Every public function documented with at least one example.
- No "no documentation" warnings.

Run `mix docs && open doc/index.html` and read it as a first-time user. If anything is unclear, fix the docs.

### Performance

Sanity benchmark (not a test gate, but record results in `bench/`):

- Parse a 1MB EPUB in < 100ms.
- Build a 1MB EPUB in < 200ms.
- Round-trip a 5MB EPUB in < 1 second.

Numbers are upper bounds, not targets. If you blow past them, investigate.

---

## 9. Phased execution

### Phase A — reader MVP (~1.5 days)

**Goal:** `LangelicEpub.parse/1` works for all 8 spike fixtures.

Tasks:

1. Initialise repo: `mix new langelic_epub --module LangelicEpub`.
2. Set up `mix.exs` per the template above.
3. Initialise the Rust crate: `mkdir -p native/langelic_epub && cd native/langelic_epub && cargo init --lib`.
4. Configure `Cargo.toml` per the template.
5. Create `types.rs` with `Document`, `Chapter`, `Asset`, `NavItem` Rust structs deriving `NifStruct`.
6. Create `error.rs` with `ParseError` enum + `thiserror` derives.
7. Implement `opf.rs` with `extract_from_zip` and `parse_extras`.
8. Implement `reader.rs` calling iepub then merging OPF extras.
9. Wire NIF entry point in `lib.rs` with `catch_panic` guard.
10. Create Elixir-side struct modules and `LangelicEpub.parse/1`.
11. Create `LangelicEpub.Native` module with `RustlerPrecompiled` config (set `force_build: true` for now).
12. Curate fixtures (or use spike samples temporarily).
13. Write fixture-driven parse tests asserting metadata, spine, TOC.
14. `mix test` green.

**Done when:** all 8 fixtures parse, `language` is correctly populated for all, panic guard test passes.

### Phase B — writer MVP (~1.5 days)

**Goal:** `LangelicEpub.build/1` produces valid EPUBs that round-trip cleanly.

Tasks:

1. Implement `writer.rs` using epub-builder.
2. Add `validate` function for required fields and duplicate IDs.
3. Implement TOC nesting walk (the `add_spine_and_toc` helper).
4. Wire NIF entry point.
5. Add `LangelicEpub.build/1` Elixir side.
6. Write round-trip tests for all 8 fixtures.
7. Write error-path tests (missing title, duplicate IDs, etc.).
8. Install epubcheck locally and validate generated output.

**Done when:** every round-trip preserves title, language, creators, identifier, spine length, asset count. Built EPUBs pass epubcheck with no errors.

### Phase C — distribution (~1 day)

**Goal:** Precompiled binaries available for all target platforms.

Tasks:

1. Set up GitHub repo (if not already done).
2. Add `.github/workflows/ci.yml` and verify it passes on a PR.
3. Add `.github/workflows/release.yml`.
4. Tag a `v0.1.0-rc.1` release to test the workflow.
5. Verify all 5 target binaries are attached to the GitHub release.
6. On a clean machine without Rust installed, add the package as a path dep to a test app and verify it loads the precompiled binary.

**Done when:** rc.1 release exists with all binaries, fresh dev environment can use the package without Rust toolchain.

### Phase D — documentation + publish (~1 day)

**Goal:** Library is on Hex, discoverable, and documented well enough that a stranger can use it correctly.

Tasks:

1. Write README.md per the template above.
2. Write CHANGELOG.md.
3. Polish all moduledocs and function docs.
4. Run `mix docs`, read every page, fix anything unclear.
5. Run `mix hex.publish --dry-run` and review.
6. Tag `v0.1.0`, wait for release workflow.
7. `mix rustler_precompiled.download LangelicEpub.Native --all --print`.
8. Commit the regenerated checksum file.
9. `mix hex.publish` for real.
10. Add the dependency to `langelic`'s `mix.exs` and verify the documents pipeline works.

**Done when:** package is on Hex, docs render correctly on hexdocs.pm, langelic uses it.

### Total estimate: ~5 days

This is for focused, uninterrupted work. Build in slack for the inevitable surprises (epubcheck warning you didn't expect, a fixture EPUB with a quirk, a CI matrix failure on Windows you didn't plan for).

---

## 10. Open questions and explicit non-goals

### Out of scope for v0.1

- **MOBI support.** iepub has it, but Langelic doesn't need it.
- **DRM decryption.** We detect encrypted assets and pass them through; we never decrypt.
- **Streaming API.** Both `parse/1` and `build/1` work on full byte buffers in memory. EPUBs over ~50MB are rare; the documents pipeline targets <50MB. A streaming API can come later if needed.
- **EPUB 2 generation.** We always emit EPUB 3 with backward-compatible NCX. EPUB 2 generation is dead-end work.
- **Fine-grained TOC manipulation API.** v0.1 builds the TOC from the spine + nested `NavItem` tree. Higher-level helpers ("add a chapter at position N", "renumber chapters") can come later.
- **Validation against epubcheck.** We test against epubcheck in CI but don't expose a `validate/1` function. Users who want validation can shell out themselves.

### Open questions for the v0.1 implementer

These can be decided during implementation; no need to resolve before starting:

1. **What to name the GitHub org / repo.** Probably `langelic/langelic_epub` if there's an org account, otherwise `<user>/langelic_epub`. Update the URLs in mix.exs accordingly.
2. **Whether to expose iepub-specific concepts.** I recommend not — keep the API library-agnostic so we can swap iepub later.
3. **Whether the `metadata` field should be `%{String => [String]}` or something more structured.** Map of strings is the simple, defensible choice. If a real use case demands more structure (e.g. typed accessors for `dc:subject`), add it in v0.2.
4. **Whether to include doctests.** Recommend yes for the simple ones (struct construction). For parse/build that need fixtures, prefer integration tests.
5. **Whether to validate UTF-8 on chapter `data` during build.** EPUB requires XHTML which requires UTF-8. We could enforce; we could let downstream readers complain. I lean toward enforcement (it's a small check, and silent corruption is the worst outcome).

### Future work (post-v0.1)

Capture these as GitHub issues against the v0.1 release, not as scope creep:

- Multi-language metadata (e.g. parallel titles in source + target).
- Page-list navigation (EPUB 3 `<nav epub:type="page-list">`).
- Media overlays / SMIL audio.
- Dictionaries embedded as collections.
- Streaming parse for very large EPUBs.
- Idiomatic Elixir TOC builder helpers.
- Optional `validate/1` function shelling out to epubcheck.
- `Inspect` protocol implementations that elide large `data` binaries for readable IEx output.

---

## 11. Critical references

- **iepub source code:** `~/.cargo/registry/src/index.crates.io-*/iepub-1.3.4/` — read this for the actual API. docs.rs is incomplete.
- **iepub on crates.io:** https://crates.io/crates/iepub
- **iepub on GitHub:** https://github.com/inkroom/iepub
- **epub-builder on docs.rs:** https://docs.rs/epub-builder
- **epub-builder on GitHub:** https://github.com/crowdagger/epub-builder
- **rustler_precompiled guide:** https://hexdocs.pm/rustler_precompiled/precompilation_guide.html
- **rustler_precompiled action:** https://github.com/philss/rustler-precompiled-action
- **EPUB 3 spec:** https://www.w3.org/TR/epub-33/
- **epubcheck:** https://github.com/w3c/epubcheck

The Langelic-specific context lives in:

- `notes/2026-04-19-document-translation-design.md` — full document translation design
- `lib/langelic/comics/ocr_adapter.ex` and `lib/langelic/workers/ocr_comic_page_worker.ex` — the pattern this library will plug into
- `AGENTS.md` — project conventions (always relevant)

---

## 12. What "high quality" means here

If we're going to publish this and ask the community to use it, the bar is:

- **It just works.** No silent failures, no surprising errors. If something can fail, the failure is well-typed and well-documented.
- **You can read the code.** Idiomatic Elixir on the Elixir side, idiomatic Rust on the Rust side. No clever tricks. Comments where decisions are non-obvious.
- **You can find the docs.** `LangelicEpub.parse/1` shows up in IEx help, on hexdocs.pm, in the README. Examples actually run.
- **You can trust the tests.** Round-trip tests verify the property that matters. Error paths are covered. Panic safety is asserted.
- **You can install it.** Five seconds in `mix.exs`, no Rust toolchain required for the common platforms. Building from source works for the others.
- **You can get help.** Public repo, issues enabled, CHANGELOG honest, README has a "Limitations" section.

If at any point during implementation you find yourself thinking "this is a bit rough but it's good enough for Langelic," stop and fix it. We are not building this for Langelic. We are building this for the next person in the Elixir community who needs to read or write an EPUB and is staring at the same empty Hex page we did.
