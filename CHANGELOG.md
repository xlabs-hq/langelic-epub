# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

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

[Unreleased]: https://github.com/xlabs-hq/langelic-epub/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/xlabs-hq/langelic-epub/releases/tag/v0.1.0
