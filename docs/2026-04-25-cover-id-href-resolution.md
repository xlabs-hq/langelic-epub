# Bug: `cover_asset_id` was sometimes a file path instead of a manifest item id

**Status:** Fixed in `1ef3a98` (`resolve_cover_asset_id`).
**Reporter:** discovered while wiring up the Langelic translated-cover renderer
**Severity (at the time):** medium — every consumer of
`LangelicEpub.Document.cover_asset_id` had to defensively check whether it was an
id or a path, and any consumer that didn't got a silent miss (the `:cover` asset
in Langelic's DB ended up classified as `:image`, the rendered EPUB had no cover
entry).

## Symptom

For an EPUB whose OPF declares the cover via the EPUB-2 idiom

```xml
<opf:meta name="cover" content="images/cover.jpg" />
```

(note: `content="images/cover.jpg"` is a *file path*, not a manifest item id),
`LangelicEpub.parse/1` returned

```elixir
%LangelicEpub.Document{
  cover_asset_id: "images/cover.jpg",
  assets: [
    %LangelicEpub.Asset{id: "coverpng", file_name: "images/cover.jpg", ...},
    ...
  ]
}
```

`cover_asset_id` referenced no actual asset by id — it matched only by
`file_name`.

The OPF 2.0.1 spec says `content` for `<meta name="cover">` should be a manifest
item id, but real-world EPUBs (libgen rips, Calibre exports, older Adobe tooling)
frequently put the href there instead. Both shapes are common enough that
consumers can't assume id-only.

Reproduction file: `qntm - There Is No Antimemetics Division - libgen.li.epub`
(see `2026-04-24-namespaced-opf-spine-bug.md` for the source).

## Root cause

`opf::parse_extras` stores the meta `content` value verbatim into
`extras.cover_id`. The reader then took that value as-is whenever it was
`Some(_)`, so a path-valued `content` propagated straight through to
`cover_asset_id` without ever being resolved against the manifest — even though
the reader already had a reverse `href → id` map on hand.

## Resolution

`reader::resolve_cover_asset_id` (`1ef3a98`) normalises `extras.cover_id` to a
manifest item id, always:

```rust
fn resolve_cover_asset_id(extras, href_to_id) -> Option<String> {
    extras.cover_id.as_ref().and_then(|cid| {
        if extras.manifest.contains_key(cid) {
            return Some(cid.clone());      // already an id → pass through
        }
        let stripped = strip_fragment(cid); // else treat as an href
        href_to_id
            .get(stripped)
            .or_else(|| href_to_id.get(&resolve_path(&extras.opf_dir, stripped)))
            .cloned()
    })
}
```

`href_to_id` is built earlier in `parse` with both the OPF-relative href and the
archive-absolute path inserted for each manifest item, so the lookup catches
either shape. A `content` that matches neither an id nor a known href resolves to
`None` rather than propagating junk. The EPUB-3 `properties="cover-image"` path
is handled upstream in `opf::parse_extras` (it sets `extras.cover_id` to the
item's id directly), so it flows through the `contains_key` branch unchanged.

End result: `LangelicEpub.Document.cover_asset_id` is *always* a manifest item id
or `nil`, never a path. Consumers can compare it directly against `Asset.id`.

## Tests

`test/langelic_epub_test.exs` → `describe "parse/1 cover_asset_id"` covers all
four cases:

1. path-form meta (`content="images/cover.png"`) → `cover_asset_id == "coverpng"`
2. id-form meta (`content="coverpng"`) → same (regression)
3. EPUB-3 `properties="cover-image"` → `cover_asset_id == "cover-image"`
4. malformed ref (points at neither id nor real href) → `cover_asset_id == nil`

Fixtures: `EpubFixtureBuilder.cover_meta_epub2/1` and
`cover_image_property_epub3/0`.

## Downstream impact (context)

Langelic's extractor had a defensive `file_name`-match workaround in
`EpubStructureExtractor.classify_asset/2`. With parsing fixed, the id-based
clause is the one that matches and the file-name fallback is dead code that can
be removed in a follow-up Langelic change.
