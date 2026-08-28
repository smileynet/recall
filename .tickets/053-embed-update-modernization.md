---
id: "053"
title: "Modernize embed/update: OnceLock for ORT init, computed URLs, home-dir fallback"
status: open
blocked_by: []
priority: medium
validation_criteria:
  - "cargo test + clippy clean"
  - "no unsafe static mut in embed.rs"
---

# Modernize embed/update: OnceLock for ORT init, computed URLs, home-dir fallback

## Context

From the 2026-08-23 review (`.scratch/review/embed-update.md`, items M1/M3/L1).
Non-bug quality improvements deferred from 048. Verified `static mut
ORT_INIT_ERROR` still present at `src/embed.rs:70`.

## What to build

### M3 — Replace `unsafe static mut` with `OnceLock` (`src/embed.rs`)
`ORT_INIT` (`Once`) + `static mut ORT_INIT_ERROR: Option<String>` uses `unsafe`
blocks for read/write. Modern equivalent is safe:
- [ ] Replace with `static ORT_INIT: OnceLock<Result<(), String>>` and
      `get_or_init(|| ensure_ort_runtime_inner().map_err(|e| format!("{:#}", e)))`
- [ ] Remove all `unsafe` blocks in `ensure_ort_runtime`

### M1 — Compute ORT URLs from constants (`src/embed.rs`)
`ORT_VERSION` exists but download URLs are hardcoded strings; a version bump
means editing multiple URLs.
- [ ] Build the download URL from `ORT_VERSION` + platform triple in one function
- [ ] Single source of truth for the version

### L1 — Home-directory fallback (`src/embed.rs`, and any peers)
`unwrap_or_else(|_| ".".to_string())` writes to CWD if neither USERPROFILE nor
HOME is set — CWD may be read-only.
- [ ] Fail with a clear error instead of silently falling back to `.`, OR
- [ ] Fall back to the system temp dir. Decide and apply consistently.

## Acceptance criteria

- [ ] No `unsafe` in `embed.rs` ORT init path
- [ ] ORT URL derived from `ORT_VERSION` (change version in one place)
- [ ] Home-dir resolution failure is explicit, not a silent CWD write
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- `cargo clippy` reports no `static_mut_refs` / unsafe warnings in embed.rs
- Grep: no `static mut` in `src/embed.rs`
