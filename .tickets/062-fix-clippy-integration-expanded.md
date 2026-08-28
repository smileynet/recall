---
id: "062"
title: "Fix unused import/var in integration_expanded.rs (clippy clean)"
status: in_progress
blocked_by: []
priority: low
validation_criteria:
  - "cargo clippy --all-targets clean"
  - "cargo test passes"
---

# Fix unused import/var in integration_expanded.rs (clippy clean)

## Context

Pre-existing clippy warnings surfaced during 061 (documented there as out of
scope). They are the only warnings keeping `cargo clippy --all-targets` from
clean, so future tickets get a noisy "clippy clean" signal. Verified 2026-08-28.

## Problems (verified)

- `tests/integration_expanded.rs:7` — `use recall::{embed, scan, search, store};`
  `embed` is never referenced → unused-import warning.
- `tests/integration_expanded.rs:139` — in `test_scan_detects_v3_session_dir`,
  `let conn = setup_db(&tmp);` is never used. The test only reads fixture files
  off disk (`fixtures_dir()` + `std::fs`); it touches neither `conn` nor the DB.
  `setup_db` also sets process-global `RECALL_DB` as a side effect that this test
  doesn't need.

## What to build

- [ ] Remove `embed` from the `use recall::{...}` list (genuinely unused)
- [ ] In `test_scan_detects_v3_session_dir`, drop the unused `let tmp`/`let conn =
      setup_db(&tmp)` lines (test doesn't use the DB; also removes an unneeded
      `RECALL_DB` set_var)

## Acceptance criteria

- [x] `cargo clippy --all-targets` clean (no warnings)
- [x] `cargo test` passes (test behavior unchanged)

## Validation criteria

- `cargo clippy --all-targets 2>&1` → no `warning:` lines
- `cargo test --test integration_expanded` → all pass

## Evidence (2026-08-28)

- Removed `embed` from the import; dropped the unused `tmp`/`conn` in
  `test_scan_detects_v3_session_dir` (it only reads fixtures off disk).
- `cargo clippy --all-targets`: no warnings (previously 2).
- `cargo test --test integration_expanded`: 5 passed. Full `cargo test`: all
  binaries green (76 lib + 11 bin + 14 cli_errors + others).
- `cargo fmt --check`: clean.
