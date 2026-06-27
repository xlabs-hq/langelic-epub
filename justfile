# Project commands. Run `just --list` to see them all.

cargo_manifest := "native/langelic_epub/Cargo.toml"

# Interactive release: pick patch/minor/major, roll the CHANGELOG, tag & push.
release:
    elixir scripts/release.exs

# Run the full test suite (builds the NIF from source; includes epubcheck tests).
test:
    LANGELIC_EPUB_BUILD=true mix test --include external

# Format Elixir + Rust.
fmt:
    mix format
    cargo fmt --manifest-path {{cargo_manifest}}

# Run the CI gates locally before pushing.
check:
    bin/check_versions
    mix format --check-formatted
    LANGELIC_EPUB_BUILD=true mix compile --warnings-as-errors
    mix credo --strict
    cargo fmt --manifest-path {{cargo_manifest}} -- --check
    cargo clippy --manifest-path {{cargo_manifest}} --all-targets -- -D warnings
