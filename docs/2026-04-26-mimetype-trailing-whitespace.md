# Bug: `LangelicEpub.parse/1` rejected EPUBs whose `mimetype` had trailing whitespace

**Status:** Fixed in `6fd5e01` (lenient `validate_mimetype`, iepub dropped).
**Reporter:** discovered while building the Langelic EPUB import test corpus;
1 of 50 real-world fixtures tripped this.
**Severity (at the time):** medium — affected a long tail of older /
fan-formatted / Calibre-converted EPUBs that other readers (Apple Books, Calibre,
ADE, iBooks) accept silently. Each affected fixture was one document the user
couldn't translate.

## Symptom

Calling `LangelicEpub.parse/1` on certain EPUBs returned:

```elixir
{:error,
 %LangelicEpub.Error{
   kind: :malformed_opf,
   message: ~s(malformed OPF: InvalidArchive("not a epub file"))
 }}
```

The bytes were a valid EPUB by every other measure: the ZIP unzips, `mimetype`
is the first entry containing `application/epub+zip`, `META-INF/container.xml` is
present and well-formed, the OPF parses, the spine resolves, and other readers
open it without complaint.

## Root cause

The `mimetype` file in the failing EPUBs is **21 bytes**: the canonical
`application/epub+zip` (20 bytes) plus a trailing `0x0a` (LF). The OCF spec
requires the file to contain *exactly* `application/epub+zip`, but trailing
whitespace is widespread in the wild — Calibre, fan exporters, and several
conversion tools all emit it (and occasionally a leading UTF-8 BOM, or CRLF).

At the time, structure came from the upstream `iepub` crate, whose
`read_from_vec` did a byte-exact check on the mimetype field and returned
`InvalidArchive("not a epub file")` for any non-20-byte length. Our wrapper
bubbled that up as `AppError::MalformedOpf` → `kind: :malformed_opf` — misleading,
since the OPF was fine; only the mimetype envelope was non-conformant. The check
fired *before* any of our own code ran, so the data we'd otherwise recover never
got a chance.

## Resolution

iepub was dropped entirely (`6fd5e01`) and the reader became pure `zip` +
`quick_xml`, so iepub's strict check no longer runs. The `mimetype` envelope is
now validated leniently by `opf::validate_mimetype`, which reads the `mimetype`
zip entry, strips a leading UTF-8 BOM, trims surrounding whitespace, and compares
against `application/epub+zip`:

```rust
let stripped: &[u8] = buf.strip_prefix(UTF8_BOM).unwrap_or(&buf);
let text = std::str::from_utf8(stripped)?.trim();
if text != EPUB_MIMETYPE {
    return Err(AppError::InvalidMimetype(format!(
        "expected {:?}, got {:?}", EPUB_MIMETYPE, text
    )));
}
```

Tolerance is deliberately scoped to *cosmetic* deviation (BOM + whitespace).
Genuine non-EPUB content still fails — but now with the accurate
`kind: :invalid_mimetype` instead of the misleading `:malformed_opf`. A missing
`mimetype` entry is also `:invalid_mimetype`.

## Tests

`test/lenient_mimetype_test.exs`:

- **acceptance:** trailing LF, trailing CRLF, trailing space, leading UTF-8 BOM —
  each builds a 1-chapter EPUB with the deviant mimetype and asserts `parse/1`
  succeeds.
- **regression:** a wrong mimetype string and random bytes both return
  `{:error, %LangelicEpub.Error{kind: :invalid_mimetype}}` — the guard that we
  didn't accidentally start accepting arbitrary zips.

Fixtures are built programmatically via `EpubFixtureBuilder` (no checked-in
binaries).

## Downstream impact (context)

The Langelic corpus had marked the affected fixture (`calibre_watts_blindsight`)
as `:skip`. With this fix shipped and the `langelic_epub` dependency bumped, that
annotation can be removed and the corpus suite goes from 49/50 to 50/50.
