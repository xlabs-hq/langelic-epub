# LangelicEpub Update & Release Procedure

Two things drift over time and need a deliberate procedure:

1. **The Rust crates** that back the NIF — pinned in
   `native/langelic_epub/Cargo.toml` (`zip`, `quick-xml`, `epub-builder`,
   `rustler`, …). Dependabot opens weekly PRs for these; review them with the
   notes below.
2. **The supported Elixir/OTP and NIF ABI** — the CI matrix, the
   `nif_versions`/`targets` in `lib/langelic_epub/native.ex`, and the release
   matrix in `.github/workflows/release.yml`.

And one thing must happen in a **specific order** every release: regenerating the
precompiled-NIF checksum file. That's the part everyone gets wrong; it's last,
and CI now does it for you.

---

## Part A — Bumping the Rust crates

All deps are on crates.io, so track **released versions**, not git refs.

1. See what Dependabot proposes (or check manually):
   ```bash
   grep -E 'zip|quick-xml|epub-builder|rustler' native/langelic_epub/Cargo.toml
   cargo update --manifest-path native/langelic_epub/Cargo.toml --dry-run
   ```
2. Read the crate's CHANGELOG for breaking changes to the APIs we touch:
   - `zip` — archive reading/writing (the OCF container is a zip).
   - `quick-xml` — OPF/NCX/nav parsing (namespaces, entity handling).
   - `epub-builder` — EPUB 3 emission on the `build/1` path.
   - `rustler` — NIF ABI. A rustler bump can change the **NIF version**; if it
     does, update `nif_versions` in `native.ex`, the `nif` matrix in
     `release.yml`, and the `default = ["nif_version_2_XX"]` feature in
     `Cargo.toml` together. The release artifact built against the lowest NIF
     version loads on all newer OTPs — this is why we ship one artifact per
     target, not per OTP.
3. Bump, then prove it locally with a real build (not the precompiled download):
   ```bash
   cargo update -p <crate> --manifest-path native/langelic_epub/Cargo.toml
   LANGELIC_EPUB_BUILD=true mix test --include external
   ```
4. Fix any signature/type drift. **Map** changes across the NIF boundary — never
   re-implement EPUB semantics on the Elixir side (the golden rule from
   [PORTING.md](PORTING.md)).

> **Security:** if `zip`/`quick-xml` fix a parsing/zip-bomb/entity-expansion
> advisory, bump promptly and cut a patch release.

---

## Part B — Bumping the toolchain / supported versions

LangelicEpub ships **no external binary** (it's a pure Rust NIF), so there is no
runtime library pin to track. What can drift:

- **CI matrix** (`.github/workflows/ci.yml`) — the OTP/Elixir combos we test and
  the `epubcheck` install. Add a newer OTP/Elixir row when one ships; keep the
  oldest supported row matching `elixir: "~> 1.15"` in `mix.exs`.
- **Release targets** (`.github/workflows/release.yml` + `targets` in
  `native.ex`) — the precompiled platforms. Add a target in **both** places or a
  consumer on that platform falls back to a from-source build.
- **`epubcheck`** — the external validator the `:external` tests shell out to.
  It's installed in CI, not pinned; a major epubcheck bump can change which
  EPUBs it flags. Re-run `mix test --include external` after a CI bump.

---

## Part C — Cutting a release (order matters)

This is the precompiled-NIF dance. Use `just release` (`scripts/release.exs`) for
steps 1–3; CI does the rest, including the checksum regen.

1. **Bump the version.** `just release` shows current vs published, asks
   patch/minor/major, rolls the CHANGELOG `[Unreleased]` section and its compare
   links, bumps **both** `mix.exs` and `Cargo.toml` (kept in sync —
   `bin/check_versions` gates this in CI), then (on confirm) commits, tags
   `vX.Y.Z`, and pushes — which triggers `release.yml`.
   - Semver against **our** API (`parse/1`, `build/1`, the structs/errors), not
     the crates'. Big additive features are minor `0.x` bumps.
2. **Wait for `release.yml`'s build matrix to finish.** Confirm the GitHub
   release has **one artifact per target** (5: the two darwin + three linux
   targets in the matrix).
3. **CI regenerates the checksum file and publishes — gated by the `hex`
   environment.** The `publish` job runs:
   ```bash
   LANGELIC_EPUB_BUILD=true mix rustler_precompiled.download LangelicEpub.Native --all --print
   mix hex.publish package --yes
   ```
   GitHub **pauses** before this job for a required reviewer to approve, so you
   eyeball the GitHub release before anything ships to Hex. `LANGELIC_EPUB_BUILD`
   forces a local NIF build so compiling `native.ex` doesn't try to download a
   NIF whose checksum doesn't exist yet (the chicken-and-egg). The package
   tarball carries the freshly-generated checksums; nothing is committed back to
   the repo and there's no re-tag.
   - One-time setup for this to work: a `hex` **environment** in the repo
     (Settings → Environments) with a required reviewer, and a `HEX_API_KEY`
     secret on that environment.
   - **Doing it manually instead?** After step 2, run the two commands above
     locally (the `--print` one writes `checksum-Elixir.LangelicEpub.Native.exs`;
     commit it), then `mix hex.publish`. This is the irreversible outward step —
     needs your Hex auth and a fresh go-ahead.

### Verify the whole point afterwards
On a clean machine (or a fresh `_build`/`deps`) with **no Rust toolchain
installed**:
```elixir
# mix.exs
{:langelic_epub, "~> 0.1"}
```
```bash
mix deps.get && mix compile     # downloads the precompiled NIF, no build
```
If that loads and `LangelicEpub.parse/1` round-trips a real EPUB, the release is
good.

---

## Release ordering, at a glance

```
just release (bump mix.exs + Cargo.toml + CHANGELOG)  →  tag vX.Y.Z
   →  release.yml builds N artifacts  →  GitHub release created
   →  `hex` environment approval  →  CI regens checksums + hex.publish
   →  verify clean precompiled install
```

The checksum file is **always** generated *after* the artifacts exist. Never
hand-edit it; never publish before it's regenerated.
