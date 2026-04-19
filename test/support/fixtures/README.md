# Test fixtures

Two kinds of fixture EPUBs live here.

## Committed fixtures

Hand-crafted, CC0-licensed, minimal EPUBs that exercise specific features.
These are safe to ship with the repo and run in CI.

*(To be added — see plan §5 "Fixture management".)*

## Spike samples (not committed)

Real-world EPUBs live under `samples/` and are **gitignored**. These come from
the author's personal library and have unclear licenses; they are used only
for local smoke-testing Phase A and Phase B. Populate the directory by
copying EPUBs into `test/support/fixtures/samples/` before running tests that
depend on them.

If `samples/` is empty, the corresponding tests are skipped with a clear
message.
