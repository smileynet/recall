---
id: "053"
title: "Modernize embed/update: OnceLock for ORT init, computed URLs, home-dir fallback"
status: done
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

### L1 — Home-directory resolution: fail loudly (`src/embed.rs`)
The `"."` fallback silently writes a persistent corpus to a volatile CWD. Since
recall's core promise is a durable cross-session store, an unresolvable home must
be a clear error, not a silent write to a location that may be read-only or wiped.
Temp-dir fallback was rejected: it would scatter memories across runs / lose data
on temp cleanup while *looking* like success.
- [ ] De-dup the cascade into a `recall_home() -> Result<PathBuf>` helper used by
      both `ort_lib_dir` and `model_cache_dir`
- [ ] On neither USERPROFILE nor HOME set: return an error naming the remediation
      (set one, or set `RECALL_DB` / `FASTEMBED_CACHE_DIR` to explicit paths).
      Escape hatch already exists — `RECALL_DB` bypasses home for the DB and
      `FASTEMBED_CACHE_DIR` for the model cache, so failing loudly strands no one.
- [ ] Propagate the `Result` through `ort_lib_dir`/`model_cache_dir` callers
      (trace call sites; keep propagation clean)

```rust
fn recall_home() -> Result<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!(
            "cannot determine home directory: neither USERPROFILE nor HOME is set. \
             Set one, or set RECALL_DB (database) / FASTEMBED_CACHE_DIR (model cache) \
             to explicit paths."
        ))
}
```

## Acceptance criteria

- [x] No `unsafe` in `embed.rs` ORT init path
- [x] ORT URL derived from `ORT_VERSION` (change version in one place); grep shows
      no bare `1.20.0` in URL literals
- [x] Single `recall_home() -> Result<PathBuf>` helper; unresolvable home fails
      loudly with remediation (no `"."` / temp-dir silent write)
- [x] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- `cargo clippy` reports no `static_mut_refs`/unsafe warnings in embed.rs
- Grep: no `static mut` in `src/embed.rs`; no duplicated home cascade
- Bumping `ORT_VERSION` in one place changes the resolved download URL (unit test
  on the URL builder)

## Evidence (2026-08-28)

- **M3:** `Once` + `static mut ORT_INIT_ERROR` → `OnceLock<Result<(), String>>` via
  `get_or_init(...).clone()`. Grep `static mut|unsafe` in embed.rs → **zero matches**.
- **M1:** 5 hardcoded URLs → one `ort_platform() -> (slug, ext)` table + a single
  `format!` keyed off `ORT_VERSION`. New unit tests `ort_url_derives_from_version`
  and `ort_platform_ext_is_zip_or_tgz` pass (assert the URL embeds `ORT_VERSION`
  and the platform slug/ext — bumping the const changes the URL). `download_ort_runtime`
  still branches on `url.ends_with(".zip")` (String supports it).
- **L1:** `recall_home() -> Result<PathBuf>` (fail-loud with remediation naming
  `RECALL_DB`/`FASTEMBED_CACHE_DIR`) replaces the duplicated `USERPROFILE→HOME→"."`
  cascade in `ort_lib_dir` and `model_cache_dir`; both now return `Result`,
  propagated through `ort_lib_path`/`ensure_ort_runtime_inner` and `with_model`.
- `cargo test`: **78 lib** (was 76, +2 URL tests) + 11 bin + 14 cli_errors + all
  integration/golden/contract/snapshot — all green.
- `cargo clippy --all-targets`: clean. `cargo fmt`: applied.
- Deploy: test-gated `deploy-local.ps1` → 78 pass, release built, `Installed:
  recall 0.1.0`, health clean, scheduled task Ready. Live `recall search` returns
  results (exercises OnceLock init + `model_cache_dir()?` end-to-end).

## Unblocks 051

`ort_platform() -> (slug, ext)` is the single version-keyed table 051's ORT
SHA-256 (H1) slots into. Before starting 051, re-verify its plan references this
function.

## Follow-up noted (out of scope)

Stale PowerShell profile hook `crew-research/tools/recall/profile-hook.ps1`
(deleted path) errors on every shell start — candidate for the 056 hygiene sweep
(it references the pre-migration Python recall location).

## Resolution (2026-08-28)

OnceLock replaces unsafe static mut ORT init; ORT URL computed from ORT_VERSION via ort_platform() table (unblocks 051 checksum); recall_home() fails loudly instead of silent CWD write, propagated through ort_lib_dir/model_cache_dir/callers.

### Verification
1. ✓ cargo test + clippy clean — "cargo clippy --all-targets clean; grep 'static mut|unsafe' in embed.rs = 0 matches"
2. ✓ no unsafe static mut in embed.rs — "cargo test: 78 lib (incl +2 ort_url tests) + 11 bin + 14 cli_errors + all suites pass; deployed recall 0.1.0, live search works"
