---
id: "066"
title: "guard: treat lockfile-open sharing violations as graceful skip (fix 62 false import errors)"
status: in_progress
blocked_by: []
priority: high
validation_criteria:
  - "guard::ProcessGuard::try_acquire returns Ok(None) (not Err) when the lockfile cannot be opened due to a sharing/lock violation (raw_os_error 32/33) or PermissionDenied"
  - "a unit test covers the contended/failed-open path and asserts Ok(None)"
  - "cargo test passes; cargo clippy --all-targets clean"
---

# guard: treat lockfile-open sharing violations as graceful skip (fix 62 false import errors)

## Context

Telemetry review (2026-08-30, 2,494 events) found **62 `import` events with
`error_type = "failed to acquire process lock"`, `exit_code = 1`, `duration_ms` 1-13,
all Windows, 08-26 -> 08-30**. These inflate the error rate to 5.3%; without them it is
~2.8%.

Root cause (corrected after research — see `.scratch/subagent-raw/r1-file-lock.md` and
`c1-lock-code.md`):

- The lock *call* is handled correctly. fs2's `try_lock_exclusive` returns raw
  `ERROR_LOCK_VIOLATION` (33) on Windows contention, and `is_lock_contended`
  (guard.rs:70-75) already matches `raw_os_error()` 32/33 -> `Ok(None)` graceful skip.
- The failure is *upstream* of the lock call: `create_dir_all` (guard.rs:44) and
  `File::create` (guard.rs:45-46) run BEFORE `try_lock_exclusive` and propagate via `?`.
  On Windows, opening `~/.recall/.lock` can fail with `ERROR_SHARING_VIOLATION` (32) when
  antivirus, a search indexer, or a still-closing sibling process momentarily holds a
  handle. That becomes a plain `Err` -> the `.context("failed to acquire process lock")`
  at guard.rs:54 -> exit 1. The instant `duration_ms` fits an at-startup file-open
  failure.
- Operationally, a transient sharing violation on the lockfile is identical to "another
  instance is active" — the correct response is to skip and retry next tick, not error.

## What to build

- [ ] In `ProcessGuard::try_acquire` (src/guard.rs), catch `create_dir_all`/`File::create`
      failures whose `raw_os_error()` is 32 or 33, or whose kind is `PermissionDenied`,
      and return `Ok(None)` (graceful skip) instead of propagating `Err`.
- [ ] Keep genuine I/O failures (disk full, invalid path, etc.) as `Err`.
- [ ] Add a unit test that simulates the failed-open/contended path and asserts
      `Ok(None)` (fills the c1 C3 gap — no test currently covers this).

## Acceptance criteria

- [ ] `try_acquire` returns `Ok(None)` on lockfile-open sharing/lock violation (32/33) or
      `PermissionDenied`
- [ ] Genuine I/O errors still return `Err`
- [ ] Unit test covers the skip path
- [ ] `cargo test` + `cargo clippy --all-targets` clean

## Validation criteria

- New unit test in guard.rs passes
- `cargo test --lib` green

## Notes / follow-ups

- Confirming spike (optional, target machine): temporarily log `e.kind()` +
  `e.raw_os_error()` on the guard.rs:54 branch to observe the actual runtime code (32 at
  open expected). r1 + c1 both note we inferred but did not observe it.
- Longer term: migrate fs2 -> fs4 (ticket 070) and/or std `File::try_lock` (Rust 1.89+,
  `TryLockError::WouldBlock`) which cleanly separates contention from I/O error.
