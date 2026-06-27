# LangelicEpub Porting & Maintenance Playbook

How to grow and maintain this **Rustler NIF** wrapper around EPUB read/write —
shipped as a **precompiled binary** via `rustler_precompiled` so end users need
**no Rust toolchain**.

It carries forward the discipline from the sibling project **ExPdfium**
(`/Users/james/Desktop/elixir/ex_pdfium`), which wraps a Rust crate the same way
and shipped cleanly end-to-end. The release machinery, doc discipline, and
working loop transfer *directly*. What's genuinely different here isn't the
Elixir/Rust bridge — it's that EPUB is a **container format** (a zip of XML/XHTML
under an OCF envelope) full of real-world malformation we must tolerate without
silently corrupting data.

The full design rationale lives in
[`2026-04-19-langelic-epub-implementation-plan.md`](2026-04-19-langelic-epub-implementation-plan.md).
This file is the **operational** companion: the rules to honor and the loop to
work when extending the surface. Read it once top-to-bottom, then keep it open
while you work.

---

## 0. The shape of the thing

LangelicEpub is a thin **Rustler NIF** over a pure-Rust stack (`zip` +
`quick-xml` for reading, `epub-builder` for writing), distributed as a
**precompiled binary**. The Elixir side owns the ergonomics (structs, the public
API, validation messages); the Rust side is a faithful, minimal bridge.

```
lib/langelic_epub.ex            # public API: parse/1, build/1
lib/langelic_epub/native.ex     # RustlerPrecompiled config + NIF stubs
lib/langelic_epub/document.ex   # %LangelicEpub.Document{} — the parsed/buildable tree
lib/langelic_epub/chapter.ex    # %LangelicEpub.Chapter{}  (spine items)
lib/langelic_epub/asset.ex      # %LangelicEpub.Asset{}    (manifest resources)
lib/langelic_epub/nav_item.ex   # %LangelicEpub.NavItem{}  (TOC entries)
lib/langelic_epub/error.ex      # %LangelicEpub.Error{}    (kind + message)
native/langelic_epub/src/
  lib.rs      # #[rustler::nif] fns: parse, build (+ panic catch_unwind)
  reader.rs   # OCF/zip → Document
  writer.rs   # Document → EPUB 3 bytes
  opf.rs      # OPF (manifest/spine/metadata) parsing
  toc.rs      # NCX + nav.xhtml parsing
  types.rs    # Rust ↔ Elixir term encoding
  error.rs    # error kinds
```

**Golden rule (carried from ExPdfium/ExBashkit):** vendor *no* parsing or
emission logic on the Elixir side. Every semantic — how a spine is ordered, how a
`mimetype` is validated, how nav vs NCX is reconciled — lives in Rust. Elixir
only marshals data and shapes the public ergonomics. When you add a capability,
the new behavior goes in Rust; Elixir gains a struct field or a function head,
not logic.

---

## 1. Lessons to honor

These were paid for once already (here and in ExPdfium). Don't relearn them.

### The precompiled-NIF release dance (the part everyone gets wrong)
- `lib/langelic_epub/native.ex` downloads a prebuilt NIF whose checksum must be
  in `checksum-Elixir.LangelicEpub.Native.exs`. **That file is regenerated
  *after* a release exists**, from the artifacts the tag build attached. Full
  ordering in [`UPDATE_PROCEDURE.md`](UPDATE_PROCEDURE.md). The trap:
  1. tag `vX.Y.Z` → `release.yml` builds the NIFs and creates the GitHub release,
  2. **then** `mix rustler_precompiled.download LangelicEpub.Native --all --print`
     downloads them and writes the checksum file,
  3. publish to Hex (the workflow does it from CI, gated by the `hex`
     environment) — the package tarball includes the freshly-generated checksums.
- The download/compile step has a **chicken-and-egg**: compiling `native.ex`
  tries to fetch a NIF that isn't published yet. Anything that compiles the app
  during a release must set `LANGELIC_EPUB_BUILD=true` so the local build
  satisfies compilation.
- Keep the **NIF ABI version** consistent across three places: `nif_versions` in
  `native.ex`, the `nif` matrix in `release.yml`, and the
  `default = ["nif_version_2_XX"]` feature in `Cargo.toml`. The artifact built
  against the lowest NIF version forward-loads on newer OTPs — **this is why we
  ship one artifact per target, not per OTP.**

### Keep mix.exs and Cargo.toml versions in lockstep
- `bin/check_versions` fails CI if `@version` (mix.exs) ≠ `version`
  (Cargo.toml). `just release` bumps both together — never hand-edit one.

### Pin crates to released versions, bump deliberately
- All Rust deps are on crates.io and pinned in `Cargo.toml`. Dependabot proposes
  weekly bumps; review each against [`UPDATE_PROCEDURE.md` Part A](UPDATE_PROCEDURE.md)
  before merging — `zip`/`quick-xml` API drift is the usual surprise.

### CI gates that catch the common breakage
- `bin/check_versions`, `mix format --check-formatted`,
  `mix compile --warnings-as-errors`, `mix credo --strict`, `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `mix dialyzer`, and `mix test --include external`
  (the `:external` tests shell out to `epubcheck`). CI builds the NIF from source
  (`LANGELIC_EPUB_BUILD=true`) rather than downloading. `just check` runs the
  fast subset locally.

### Docs are part of "done"
- Every new public function/field gets a moduledoc + `@spec` + a doctest or test.
  Every new capability gets a README section and a CHANGELOG `[Unreleased]`
  entry. Doc drift is the easiest thing to forget.

---

## 2. What's genuinely different about EPUB (the crux)

ExPdfium's hard part was a non-thread-safe C++ library and lifetimes. **None of
that applies here** — the stack is pure Rust, synchronous, and owns no global
state. Our hard part is entirely different.

### a) Tolerating real-world malformation without lying
EPUBs in the wild violate the spec constantly: `mimetype` files with trailing
whitespace or a UTF-8 BOM, namespaced OPF where readers expect bare elements,
cover images referenced by id where the manifest uses href, NCX and nav that
disagree. Calibre and other tools accept these silently; so should we — **but
only where it's safe**. The rule:
- **Tolerate cosmetic deviation** (whitespace, BOM, namespace prefixes) — match
  what real readers accept. The `mimetype` trailing-whitespace and namespaced-OPF
  fixes in `docs/` are the template.
- **Reject genuine corruption** with a specific `%LangelicEpub.Error{kind: …}` —
  never guess past missing structural data. A wrong `mimetype` is
  `:invalid_mimetype`, not a silent pass.
- When you add tolerance, add a `docs/NNNN-<slug>.md` note (cause → fix → test)
  like the existing three, and a regression test with a real-world fixture.

### b) Round-trip fidelity is the contract
`build/1` emits EPUB 3 but writes a backward-compatible `toc.ncx` alongside
`nav.xhtml` so EPUB-2-only readers still navigate. The bar is that
`parse |> build |> parse` preserves the structural data we expose, and the output
passes `epubcheck`. Any new write capability must keep `epubcheck` green (the
`:external` test) and not regress the NCX/nav dual-emit.

### c) UTF-8 and validation live at the boundary
Chapter `data` is enforced UTF-8 to prevent silent downstream corruption;
required fields (`title`, `identifier`, `language`) and id uniqueness are
validated at build time. New fields that cross the NIF boundary get the same
treatment — validate in Rust, surface a typed error, don't trust input.

### d) Panics must not crash the BEAM
NIF entry points wrap work in `std::panic::catch_unwind` and return
`{:error, %LangelicEpub.Error{kind: :panic}}`. Any new `#[rustler::nif]` must
keep that guard — a malformed input must never take down a scheduler.

---

## 3. The working loop (this is what produces clean phases)

Per capability you add:

1. **TDD** — write the failing test first (`LANGELIC_EPUB_BUILD=true mix test`,
   with a real EPUB fixture where structure matters).
2. **Implement** — Rust first (reader/writer/opf/toc + the NIF), then the Elixir
   struct field or function head. Marshal-only; semantics stay in Rust.
3. **Full gate** — `just check` plus `mix test --include external` (keep
   `epubcheck` green). `mix dialyzer` for type drift.
4. **Review** — dispatch the **`superpowers:code-reviewer`** subagent against the
   diff. On the sibling projects it caught a real soundness bug nearly every
   phase — take it seriously, fold the fixes.
5. **Document** — README capability section + CHANGELOG `[Unreleased]` entry; a
   `docs/` note if you added malformation tolerance.
6. **Commit, push, watch CI green.** Tag a release (`just release`) when a
   capability is a meaningful user-facing increment — see
   [`UPDATE_PROCEDURE.md`](UPDATE_PROCEDURE.md).

---

## 4. Definition of done (per capability and overall)

- [ ] NIF stubs in `native.ex` match the `#[rustler::nif]` fns exactly.
- [ ] Public functions have moduledocs, `@spec`s, and doctests/tests.
- [ ] `just check` clean; `mix test --include external` green (epubcheck passes).
- [ ] `mix dialyzer` clean.
- [ ] README capability section + CHANGELOG `[Unreleased]` entry (+ a `docs/` note
      for any new malformation tolerance).
- [ ] No vendored logic — EPUB semantics come from the Rust side.
- [ ] New NIF guarded by `catch_unwind`; new boundary input validated + typed
      errors; round-trip + epubcheck preserved.

When in doubt, open ExPdfium (`/Users/james/Desktop/elixir/ex_pdfium`) and copy
the proven shape — `native.ex`, `release.yml`, `scripts/release.exs`, the doc
trio, and the per-phase loop are all directly transferable.
