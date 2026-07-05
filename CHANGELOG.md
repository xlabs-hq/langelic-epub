# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-07-05

### Fixed

- The generated `nav.xhtml` no longer contains an empty
  `<nav epub:type="landmarks">` wrapper. epub-builder emits the wrapper even
  when there are no landmark entries, which fails epubcheck RSC-005
  ("element \"nav\" incomplete; missing required element \"ol\""). The
  writer's post-processing pass now strips a landmarks nav that has no
  entries; a landmarks nav with real entries is kept verbatim. Built EPUBs
  that previously carried this epubcheck error are now clean, and the test
  suite no longer allowlists it.
- The generated `toc.ncx` no longer trips epubcheck RSC-005 "different
  playOrder values for navPoint/navTarget/pageTarget that refer to same
  target". epub-builder gives every navPoint a fresh sequential `playOrder`,
  so a file appearing in the TOC more than once (e.g. as both a nested child
  and a top-level entry — common in real books) got distinct values where the
  NCX spec requires one shared value. The writer now renumbers `playOrder` in
  document order, 1-based, with repeat targets reusing the first occurrence's
  value. The test suite no longer allowlists this error either.

## [0.2.1] - 2026-07-05

## [0.2.0] - 2026-07-05

### Added

- `LangelicEpub.Document` gains a `page_progression_direction` field
  (`"rtl"`, `"ltr"`, or `nil`). When set, `build/1` writes the OPF
  `<spine page-progression-direction>` attribute so right-to-left target
  languages (Arabic, Hebrew, …) paginate correctly in real readers. For
  `"rtl"`, the generated `nav.xhtml` root `<html>` also gets `dir="rtl"` and
  the document language so table-of-contents labels render in the correct
  direction.
- `LangelicEpub.Error` may now have `kind: :invalid_page_direction` when
  `page_progression_direction` is anything other than `"rtl"`, `"ltr"`, or
  `nil`. The value is rejected at build time rather than silently dropped.

### Changed

- When `page_progression_direction` is `nil`, the built OPF now **omits** the
  `<spine>` `page-progression-direction` attribute entirely. Previously the
  underlying `epub-builder` unconditionally emitted
  `page-progression-direction="ltr"`. An omitted direction is semantically
  identical (readers default to `ltr`) but no longer hard-codes a direction
  the caller never asked for. Existing `nil` builds change bytes but not
  rendering.

### Notes

- The reader continues to **not** surface a source EPUB's spine direction:
  `parse/1` always returns `page_progression_direction: nil`. Direction is a
  build-time decision derived from the target language, never round-tripped
  from the source (a `rtl` Japanese source rebuilt into English must shed it).

## [0.1.1] - 2026-06-27

### Changed

- Dependency updates: `rustler` 0.37 → 0.38 (Elixir + crate), `zip` 7 → 8,
  `quick-xml` 0.39 → 0.40, plus `uuid`, `credo`, and `ex_doc` bumps. The
  quick-xml `Attribute::unescape_value` API was deprecated; migrated to
  `normalized_value(XmlVersion::Implicit1_0)`, which is behavior-identical
  (same XML 1.0 implicit version, depth, and predefined-entity resolver). No
  user-facing API change.
- Reader no longer depends on the `iepub` crate. All structural data
  (manifest, spine, metadata, cover, TOC) now comes from a pure
  `zip` + `quick_xml` stack. This removes a large transitive dependency
  surface and lets us tolerate non-OCF-conformant `mimetype` files
  (trailing `\n`, `\r\n`, space, or a leading UTF-8 BOM) — common in
  Calibre exports and other real-world EPUBs that other readers accept
  silently. Genuine non-EPUB content is still rejected with a new
  `:invalid_mimetype` error kind.

### Added

- `LangelicEpub.Error` may now have `kind: :invalid_mimetype` when the
  `mimetype` zip entry is missing or its content (after trimming a
  UTF-8 BOM and whitespace) is not `application/epub+zip`.
- OTP 29 support: `rustler` 0.38 builds against OTP 29's NIF interface, and CI
  now tests on OTP 29 / Elixir 1.20 in addition to OTP 26/27. The precompiled
  NIF 2.16 artifact forward-loads on OTP 29's newer NIF ABI, so no new artifact
  is shipped (a 2.17 artifact would needlessly drop OTP 26 compatibility).

### Fixed

- Dublin Core metadata is now matched by its namespace URI rather than a
  hard-coded `dc:` prefix (the OPF is parsed with quick-xml's `NsReader`), so
  `dc:`-equivalent elements bound to a non-standard prefix or a default
  namespace are recovered instead of silently dropped. Parsing the OPF also
  now rejects elements that declare more than 256 namespace bindings, bounding
  a malformed or hostile package.

## [0.1.0] - 2026-04-20

### Added

- `LangelicEpub.parse/1` — parse EPUB 2 and EPUB 3 bytes into a
  `%LangelicEpub.Document{}` with spine, assets, table of contents, and
  metadata (including fields like `<dc:language>`, `<dc:rights>`, and
  multiple `<dc:creator>` entries that the underlying iepub crate does not
  expose natively).
- `LangelicEpub.build/1` — emit EPUB 3 bytes from a
  `%LangelicEpub.Document{}`, with a backward-compatible `toc.ncx`
  alongside the EPUB 3 `nav.xhtml` so EPUB 2-only readers still navigate
  correctly.
- Validation of required fields (`title`, `identifier`, `language`) and
  spine/asset ID uniqueness at build time. UTF-8 is enforced on chapter
  `data` to prevent silent corruption in downstream readers.
- Rust panics inside the NIF are caught (`std::panic::catch_unwind`) and
  returned as `{:error, %LangelicEpub.Error{kind: :panic, ...}}` so a
  malformed input cannot crash the BEAM scheduler.
- Precompiled NIFs published via GitHub Releases for
  `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, and
  `x86_64-unknown-linux-musl`.

[Unreleased]: https://github.com/xlabs-hq/langelic-epub/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/xlabs-hq/langelic-epub/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/xlabs-hq/langelic-epub/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/xlabs-hq/langelic-epub/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/xlabs-hq/langelic-epub/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/xlabs-hq/langelic-epub/releases/tag/v0.1.0
