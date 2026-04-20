# langelic_epub

[![CI](https://github.com/xlabs-hq/langelic-epub/actions/workflows/ci.yml/badge.svg)](https://github.com/xlabs-hq/langelic-epub/actions/workflows/ci.yml)
[![Hex.pm](https://img.shields.io/hexpm/v/langelic_epub.svg)](https://hex.pm/packages/langelic_epub)
[![Docs](https://img.shields.io/badge/docs-hexdocs-blue.svg)](https://hexdocs.pm/langelic_epub)

EPUB read and write for Elixir, backed by a Rustler NIF. Parses EPUB 2 and
EPUB 3 documents into structured Elixir data and generates EPUB 3 documents
(with a backward-compatible `toc.ncx`) from the same structures.

## Installation

Add to `mix.exs`:

```elixir
def deps do
  [
    {:langelic_epub, "~> 0.1"}
  ]
end
```

Precompiled NIFs are published for macOS (aarch64, x86_64) and Linux
(aarch64-gnu, x86_64-gnu, x86_64-musl). Users on those platforms do not need
a Rust toolchain. Users on other platforms can build from source — see
[Building from source](#building-from-source).

## Quick start

```elixir
# Read
{:ok, doc} = LangelicEpub.parse(File.read!("book.epub"))
doc.title       # => "The Hobbit"
doc.language    # => "en"
length(doc.spine)  # => 23

# Modify a chapter
[first | rest] = doc.spine
translated =
  %LangelicEpub.Chapter{first | data: translate(first.data)}
modified = %LangelicEpub.Document{doc | spine: [translated | rest]}

# Write
{:ok, bytes} = LangelicEpub.build(modified)
File.write!("translated.epub", bytes)
```

## Why this library exists

There was a gap on Hex. `bupe` (the only EPUB-focused Elixir library) was last
updated nine years ago and is minimal; other packages are single-purpose or
metadata-only. The Rust ecosystem has mature EPUB tooling, so rather than
reimplement format handling in pure Elixir — where EPUB 2/3 metadata variants,
NCX vs. nav.xhtml, embedded fonts, refines metadata, and OPF schema quirks all
accumulate bugs over time — this package wraps two mature Rust crates through
a [Rustler](https://github.com/rusterlium/rustler) NIF:

- **[iepub](https://github.com/inkroom/iepub)** handles spine order, TOC
  tree, and cover detection on the read side.
- **[epub-builder](https://github.com/crowdagger/epub-builder)** handles
  EPUB 3 generation on the write side.

A small OPF re-parse layer (quick-xml) fills in the fields iepub drops
(`<dc:language>`, `<dc:rights>`, multiple `<dc:creator>` entries). A post-
processing pass rewrites the generated OPF to preserve identifiers verbatim
and inject DC elements epub-builder doesn't emit natively (`<dc:publisher>`,
`<dc:date>`, `<dc:rights>`).

## Supported features

| Feature                | Read | Write                             |
| ---------------------- | ---- | --------------------------------- |
| EPUB 2 input           | yes  | n/a                               |
| EPUB 3 input           | yes  | yes (always emitted)              |
| Multiple creators      | yes  | yes                               |
| NCX TOC                | yes  | yes (emitted for EPUB 2 readers)  |
| nav.xhtml TOC          | yes  | yes                               |
| Embedded fonts         | yes  | yes                               |
| Embedded images        | yes  | yes                               |
| Embedded CSS           | yes  | yes                               |
| Cover image            | yes  | yes                               |
| DRM-encrypted content  | detected, not decrypted | n/a    |
| MOBI                   | no   | no                                |

## Limitations and known issues

- **Identifier round-trip.** `epub-builder` requires the primary
  `<dc:identifier>` to be a UUID and prefixes it with `urn:uuid:`. When the
  source identifier is not a UUID (e.g. an ISBN or URL), the package generates
  a deterministic UUID v5 for the primary slot and re-injects the original
  identifier verbatim via OPF post-processing so readers that look it up still
  find it.
- **TOC parsing is inconsistent for a small number of source EPUBs.** iepub
  occasionally returns an empty nav tree for valid documents; the cause is a
  parser quirk for specific NCX/nav.xhtml structures. Generated output always
  has at least one nav entry per spine chapter (a positional title is used as
  a fallback) so `epubcheck` does not flag an empty nav.
- **Multiple `<itemref>` entries to the same file** (common in Calibre-split
  EPUBs) are deduplicated into a single spine entry on read.
- **`epub-builder` ID collision warnings.** When both `<dc:language>` and
  `<dc:creator>` are present, epub-builder reuses `id="epub-creator-N"` for
  both. The package rewrites the language ID to avoid the collision.
- **No streaming API.** Both `parse/1` and `build/1` accept and return full
  byte buffers in memory. For documents over ~50 MB this may be inappropriate.
- **No `validate/1` function.** External validation should shell out to
  [epubcheck](https://github.com/w3c/epubcheck).

## Error model

Every public function returns `{:ok, term} | {:error, %LangelicEpub.Error{}}`.
The `:kind` field is a well-documented atom (`:invalid_zip`,
`:missing_container`, `:malformed_opf`, `:io`, `:missing_required_field`,
`:invalid_chapter`, `:duplicate_id`, `:panic`). The full list is in the
moduledoc of [`LangelicEpub.Error`](lib/langelic_epub/error.ex). Panics on
the Rust side are caught and converted to `{:error, %Error{kind: :panic}}`
so a malformed EPUB cannot crash the BEAM scheduler thread.

## Architecture

`langelic_epub` is an Elixir wrapper around a Rustler NIF. The native code
lives in `native/langelic_epub/` and is compiled as a `cdylib`. Both NIF
functions run on the `DirtyCpu` scheduler because parsing or building a
5 MB EPUB takes 50–200 ms, well past the 1 ms guideline.

```
lib/langelic_epub/        # Public API, struct modules, error module
lib/langelic_epub/native.ex  # RustlerPrecompiled binding
native/langelic_epub/src/ # Rust NIF (reader, writer, opf, types, error)
```

## Building from source

Required:

- Elixir ≥ 1.15
- Rust ≥ stable (1.85+)

Set the environment variable to force a source build rather than downloading
the precompiled NIF:

```sh
LANGELIC_EPUB_BUILD=true mix deps.get && mix compile
```

## Contributing

Issues and pull requests are welcome. Before submitting a PR:

- `mix format`
- `mix credo --strict`
- `mix dialyzer`
- `mix test --include external` (requires [epubcheck](https://github.com/w3c/epubcheck) on PATH)
- `cargo fmt --check` and `cargo clippy -- -D warnings` for Rust changes

## License

MIT. See [LICENSE](LICENSE).

This package wraps two Rust crates under separate licenses; see [NOTICE](NOTICE)
for attribution.
