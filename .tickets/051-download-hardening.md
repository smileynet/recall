---
id: "051"
title: "Fix self-update (broken: .zip/.tar.xz not handled) + checksums + timeout"
status: in_progress
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

Research + code review (2026-08-28, `.scratch/research/xz-decoder.md`,
`.scratch/research/zip-minimal.md`, `.scratch/review/embed-zip-extract.md`,
`.scratch/review/update-insertion.md`) refined the plan below. Line numbers
re-verified post-053 (update.rs unchanged by 053; ranges were ~2-15 low in the
original draft — corrected here).

### Dependencies (decided)
- **zip** `= "8.6.0"`, `default-features = false`, `features = ["deflate"]` —
  pure-Rust (miniz_oxide), no C toolchain, all cargo-dist zips need. MSRV 1.88.
  Pin EXACTLY (heavy API churn); don't name the `ZipFile` type (reader-generic).
- **xz decoder** — CORRECTION: the ticket's "liblzma-rs pure-Rust" was wrong;
  that crate is the C-linked `xz2` fork. Real pure-Rust decode-only options:
  `lzma-rs` (152★, `forbid(unsafe)`, decode-first — recommended) or `lzma-rust2`
  (full XZ, streaming `XzReader`). **Spike required:** confirm `lzma-rs`'s .xz
  subset decodes a real cargo-dist `.tar.xz` (multi-stream/check/BCJ) before
  committing; fall back to `lzma-rust2` (streaming, no full-buffer) if it fails.
  Verify `cargo tree -i liblzma-sys` is empty after (no transitive C pull-in).

### Shared archive helper (new `src/archive.rs`)
- [ ] `pub fn extract_named_from_zip(bytes: &[u8], wanted: &str) -> Result<Vec<u8>>`
      via `zip::ZipArchive` over a `Cursor`, match `name.ends_with(wanted)` (NOT
      `by_name` — ORT dll sits in a versioned subdir), `read_to_end` → `Vec<u8>`.
- [ ] Replace embed.rs hand-rolled `extract_lib_from_zip` (L224-316, ~90 lines,
      no ZIP64/CRC, store+deflate only) with a call to the helper + `fs::write`.
      Keep `extract_lib_from_tgz` and the ORT dispatch unchanged.

### M4 (P1 bug) — Extract zip AND tar.xz in self-update (`src/update.rs`)
- [ ] `extract_binary` (L315-343) is `GzDecoder`-only → add `asset_name` param,
      move current body into `extract_tar_gz`, dispatch on extension:
      `.zip` → `archive::extract_named_from_zip`; `.tar.xz` → xz decoder + `tar`;
      `.tar.gz` → existing path. `.tar.*` needs a two-suffix check.

### H2 — Verify update checksum via `assets[].digest` (`src/update.rs`)
- [ ] `find_asset_url` (L217-282) reads the asset object at L258-260 — capture the
      sibling `digest` field there. Change return `String` → `AssetInfo { url,
      digest: Option<String>, name }`.
- [ ] In `cmd_update` (L191-212), insert `verify_digest(&bytes, digest)` between
      download (L207) and extract (L208), using the already-present `sha2`.
- [ ] Digest absent (older releases) → **[OPEN DECISION a]** warn-and-proceed vs
      hard-fail. Live release HAS digest on every asset (verified), so absence
      means a very old release.

### H1 — Verify ORT runtime checksum (`src/embed.rs`)
- [ ] Add per-platform SHA-256 to the `ort_platform()` table (053 created it —
      extend `(slug, ext)` → `(slug, ext, sha256)`).
- [ ] Verify downloaded bytes before extract/persist; mismatch aborts (temp file
      auto-cleans). Replaces the weak `>1MB` heuristic. Hashes vendored per
      `ORT_VERSION` bump (ORT isn't a GitHub release, no `assets[].digest`).

### L2 — Download timeout (`src/update.rs`)
- [ ] `download_asset` (L296-312) uses bare `ureq::get(url).call()` — no timeout
      (ureq 2.x read/write default to INFINITE). Add a shared `http_agent()`
      (`AgentBuilder` with `timeout_connect` + `timeout_read` + overall
      `.timeout(~120s)`), thread through `download_asset` AND the un-timed
      `find_asset_url` (L227-233); `fetch_latest_version` already has 5s.

## Open decisions (need owner input)
- **(a)** digest-absent behavior: warn-and-proceed (my lean — matches "verify when
  available") vs hard-fail.
- **(b)** Land the xz spike + `zip`/xz deps in THIS ticket, or split: implement
  zip (.zip → Windows, unblocks THIS platform) + H2 + L2 + H1 now, and defer
  `.tar.xz` (Linux/macOS) to a follow-up gated on the xz-decoder spike. Splitting
  ships the Windows fix + all checksums/timeout immediately without blocking on
  the xz crate evaluation.

## Acceptance criteria

- [ ] `recall update` extracts the real release formats (.zip on Windows,
      .tar.xz on Linux/macOS — or .tar.xz deferred per decision b) — verified
      against an actual release asset
- [ ] Update verifies `assets[].digest` when present (absent → per decision a)
- [ ] ORT runtime download verifies a pinned SHA-256; mismatch aborts cleanly
- [ ] Asset download + release-API calls have bounded timeouts
- [ ] `cargo test` passes, `cargo clippy` clean; `cargo tree -i liblzma-sys` empty

## Validation criteria

- Unit test: archive-type dispatch picks zip/tar.xz/tar.gz by name
- Unit test: `verify_digest` mismatch → error, nothing persisted; match → ok
- Unit test: `extract_named_from_zip` round-trips a known file from an in-memory
  store+deflate zip
- Manual: fetch THIS platform's live release asset
  (`recall-x86_64-pc-windows-msvc.zip`, digest `sha256:9c12b0d4…`) and confirm
  extraction + digest verification succeed
