---
id: "053"
title: "Modernize embed/update: OnceLock for ORT init, computed URLs, home-dir fallback"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "cargo test + clippy clean"
  - "no unsafe static mut in embed.rs"
---

# Modernize embed/update: OnceLock for ORT init, computed URLs, home-dir fallback

## Context

From the 2026-08-23 review (`.scratch/review/embed-update.md`, items M1/M3/L1),
deepened by research + code review 2026-08-28 (`.scratch/research/oncelock-migration.md`,
`.scratch/review/embed-ort-current.md`).

Sequenced BEFORE 051 (051 blocked_by 053): the M1 URL refactor establishes a
single version-keyed platform table that 051's ORT checksum then slots into,
avoiding a double refactor of the same `#[cfg]` cascade.

## Verified current state (2026-08-28)

- Two `unsafe` blocks in `ensure_ort_runtime` — write at `embed.rs:74-76`
  (inside `call_once`), read at `embed.rs:79-83`. `ensure_ort_runtime_inner` has none.
- `ort_download_url()` (`embed.rs:14-38`): 5 per-`cfg` arms, each a fully-literal
  URL with `1.20.0` baked in (~10 textual instances). `ORT_VERSION` const is used
  ONLY in a log line — the URLs do NOT reference it. Bumping version = edit const
  + all 5 URLs (the const is a false source-of-truth today).
- Home cascade `USERPROFILE → HOME → "."` duplicated in `ort_lib_dir` (L56-62)
  and `model_cache_dir` (L379-390). The `"."` fallback silently writes to CWD.

## What to build

### M3 — Replace `unsafe static mut` with `OnceLock` (`src/embed.rs`)
Research confirms the correct stable pattern (NOT nightly `get_or_try_init`, and
never reset the cell — it hands out `&'static`):
```rust
static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn ensure_ort_runtime() -> Result<()> {
    ORT_INIT
        .get_or_init(|| ensure_ort_runtime_inner().map_err(|e| format!("{:#}", e)))
        .clone()
        .map_err(|e| anyhow::anyhow!("ONNX Runtime initialization failed: {}", e))
}
```
- [ ] Replace `Once` + `static mut ORT_INIT_ERROR` with the above
- [ ] Remove both `unsafe` blocks (also clears the `static_mut_refs` 2024 deny-lint risk)

### M1 — Compute ORT URL from `ORT_VERSION` (`src/embed.rs`)
- [ ] Introduce one `ort_platform() -> (slug, ext)` `#[cfg]` cascade (e.g.
      `("win-x64", "zip")`, `("linux-x64", "tgz")`), then
      `format!("https://github.com/microsoft/onnxruntime/releases/download/v{v}/onnxruntime-{slug}-{v}.{ext}", v = ORT_VERSION)`
- [ ] Return `String` (callers unaffected). Collapses 5 literals → 1 template.
- [ ] This same `(slug, ext)` table is where 051 adds the per-platform SHA-256.

### L1 — Home-directory fallback (`src/embed.rs`)
- [ ] De-dup the cascade into a `recall_home()` helper used by both `ort_lib_dir`
      and `model_cache_dir`
- [ ] Decision: fall back to `std::env::temp_dir()` (not `"."`), and log a warning —
      keeps recall functional in headless/service contexts without silently
      writing to a possibly read-only CWD. (Document the choice.)

## Acceptance criteria

- [ ] No `unsafe` in `embed.rs` ORT init path
- [ ] ORT URL derived from `ORT_VERSION` (change version in one place); grep shows
      no bare `1.20.0` in URL literals
- [ ] Single `recall_home()` helper; no `"."` silent-CWD fallback
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- `cargo clippy` reports no `static_mut_refs`/unsafe warnings in embed.rs
- Grep: no `static mut` in `src/embed.rs`; no duplicated home cascade
- Bumping `ORT_VERSION` in one place changes the resolved download URL (unit test
  on the URL builder)
