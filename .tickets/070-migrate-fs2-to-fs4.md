---
id: "070"
title: "guard: migrate unmaintained fs2 to fs4 (RUSTSEC informational; drop-in successor)"
status: open
blocked_by: []
priority: low
---

# guard: migrate unmaintained fs2 to fs4 (RUSTSEC informational; drop-in successor)

## Context

`src/guard.rs:11` uses `use fs2::FileExt` for the single-instance process lock. `fs2`
is unmaintained (RUSTSEC informational advisory — no known vulnerability, flagged
because the crate is abandoned). `fs4` is the maintained drop-in successor with the
same `FileExt` API surface.

Current state (verified 2026-10-09 review): `Cargo.toml`/`Cargo.lock` pin `fs2 = "0.4"`
(resolved 0.4.3). No `fs4` dependency anywhere. The lock is acquired in
`guard.rs::try_acquire()` via `try_lock_exclusive()`.

Low risk, low effort — the APIs are intentionally compatible. This is hygiene, not a
functional change.

## What to build

1. Replace `fs2 = "0.4"` with `fs4 = "0.x"` (latest) in `Cargo.toml`.
2. Update `use fs2::FileExt` -> `use fs4::fs_std::FileExt` (or the fs4-equivalent import
   path — confirm against fs4's current docs; the trait moved namespaces across fs4
   versions) in `src/guard.rs`.
3. Rebuild and run the guard tests; confirm `try_lock_exclusive`/`unlock` still behave
   (lock held for process lifetime, auto-release on drop/crash).
4. `cargo build --release` + `cargo test` must pass unchanged.

## Acceptance criteria

- [ ] `fs2` removed from `Cargo.toml` and `Cargo.lock`; `fs4` present
- [ ] `src/guard.rs` compiles against fs4's `FileExt` with no behavior change
- [ ] `cargo test` passes (guard/lock tests included); single-instance lock still enforced
- [ ] `cargo audit` (if available) no longer flags fs2

## Notes

- Pure dependency swap; no coordination with other tickets.
- Verify the fs4 trait import path against its current release — it is NOT always
  `fs4::FileExt` (varies by fs4 major version).
