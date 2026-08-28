---
id: "051"
title: "Fix self-update (broken: .zip/.tar.xz not handled) + checksums + timeout"
status: open
blocked_by: ["053"]
priority: high
validation_criteria: 
  - "recall update extracts real release assets (.zip win, .tar.xz nix)"
  - "checksum mismatch aborts; download has bounded timeout"
---

# Harden self-update and ORT download: checksums, archive formats, timeout

## Context

From the 2026-08-23 review, deepened by research + code review + a LIVE release
inspection 2026-08-28 (`.scratch/research/checksum-verify.md`,
`.scratch/research/archive-extraction.md`, `.scratch/research/ureq-timeouts.md`,
`.scratch/review/update-flow-current.md`, `.scratch/review/release-config.md`).

Blocked by 053 (its M1 refactor creates the version-keyed platform table that
H1's ORT checksum slots into).

## KEY FINDING — self-update is currently broken on every platform

The live `smileynet/recall` release (verified via GitHub API 2026-08-28) ships:
- Windows: `recall-x86_64-pc-windows-msvc.zip`
- Linux/macOS: `recall-*.tar.xz` (cargo-dist default — **xz, not gzip**)

But `update.rs::extract_binary` (L313-341) is **`GzDecoder`-only**. So:
- Windows `.zip` → fails (not gzip)
- Linux/macOS `.tar.xz` → fails (xz, not gzip)

`recall update` cannot extract ANY current release asset. M4 is therefore a
**bug fix, not an enhancement** — and the format is `.tar.xz`, not the `.tar.gz`
the original ticket assumed.

## KEY FINDING — checksums already exist (H1/H2 de-risked)

The release (cargo-dist 0.32, verified live) already provides, per archive:
- a `<archive>.sha256` sidecar asset
- GitHub-populated `assets[].digest` (`sha256:<hex>`) on every asset
- an aggregate `sha256.sum` asset

So verification needs NO release-pipeline change. `find_asset_url` already parses
the release JSON with serde_json — reading `assets[].digest` is free.

## What to build

### M4 (now P1 bug) — Extract zip AND tar.xz in self-update (`src/update.rs`)
- [ ] Detect archive type by asset extension (`.zip` / `.tar.xz` / `.tar.gz`);
      `.tar.*` needs a two-suffix check (`Path::extension()` only returns the last)
- [ ] `.zip` → `zip` crate `by_name`; `.tar.xz` → `xz2`/`liblzma` + `tar`;
      keep `.tar.gz` via existing `flate2`+`tar`
- [ ] New deps required: `zip` (pin exactly, trim to `deflate` feature) and an xz
      decoder (`xz2`). Confirm licenses/build (xz2 links liblzma — consider
      `liblzma`/`ruzstd`-style pure-Rust alt if C toolchain is unwanted)
- [ ] Factor the ORT zip parse from `embed.rs::extract_lib_from_zip` into a shared
      extract-to-bytes helper (the review notes it already builds `Vec<u8>` before
      writing — trivial split)

### H2 — Verify update checksum via `assets[].digest` (`src/update.rs`)
- [ ] In `find_asset_url`, capture `assets[].digest` alongside the download URL
- [ ] After `download_asset`, compute `sha2::Sha256` and compare (constant-time)
      before `replace_self`; mismatch aborts
- [ ] If `digest` is absent (older releases), warn that integrity is unverified
- sha2 + serde_json already present — zero new deps for this item

### H1 — Verify ORT runtime checksum (`src/embed.rs`)
- [ ] Add per-platform SHA-256 into the `ort_platform()` table 053 introduces
- [ ] Verify downloaded bytes before extract/persist; mismatch aborts (temp file
      auto-cleans). Replaces the weak `>1MB` heuristic.
- Note: ORT is NOT a GitHub-authored release, so `assets[].digest` doesn't apply —
      hashes are vendored per `ORT_VERSION` bump. Document in the table.

### L2 — Download timeout (`src/update.rs`)
- [ ] `download_asset` uses bare `ureq::get(url).call()` with NO timeout — a stall
      hangs forever. ureq 2.x read/write timeouts default to INFINITE.
- [ ] Build a shared agent with `timeout_connect`, `timeout_read`, and an overall
      `.timeout(~120s)` watchdog; apply to both `download_asset` AND `find_asset_url`
      (the latter also lacks a timeout; only `fetch_latest_version` has 5s)

## Acceptance criteria

- [ ] `recall update` extracts the real release formats (.zip on Windows,
      .tar.xz on Linux/macOS) — verified against an actual release asset
- [ ] Update verifies `assets[].digest` when present, warns when absent
- [ ] ORT runtime download verifies a pinned SHA-256; mismatch aborts cleanly
- [ ] Asset download + release-API calls have bounded timeouts
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit test: archive-type dispatch picks zip/tar.xz/tar.gz by name
- Unit test: checksum mismatch → error, nothing persisted
- Manual/integration: download the live release asset for this platform and
  confirm extraction succeeds (guards against another format regression)
