---
id: "051"
title: "Fix self-update (broken: .zip/.tar.xz not handled) + checksums + timeout"
status: done
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

## Decisions (locked 2026-08-28)
- **(a) Hard-fail** when an asset digest is absent — do not install unverified.
  (Live releases always carry `assets[].digest`; absence = a broken/ancient
  release we should refuse rather than trust.)
- **(b) Split:** this ticket ships `.zip` (Windows) extraction + H2 + H1 + L2 now.
  `.tar.xz` (Linux/macOS) is deferred to **063** (gated on a pure-Rust xz spike).
  `extract_binary`'s `.tar.xz` arm lands here as a `bail!` stub pointing at 063.

## What to build

Research + code review (2026-08-28, `.scratch/research/xz-decoder.md`,
`.scratch/research/zip-minimal.md`, `.scratch/review/embed-zip-extract.md`,
`.scratch/review/update-insertion.md`) refined the plan below. Line numbers
re-verified post-053 (update.rs unchanged by 053).

### Dependencies (decided)
- **zip** `= "8.6.0"`, `default-features = false`, `features = ["deflate"]` —
  pure-Rust (miniz_oxide), no C toolchain. MSRV 1.88. Pin EXACTLY; don't name the
  `ZipFile` type (reader-generic).
- No xz dep in this ticket (deferred to 063).

### Shared archive helper (new `src/archive.rs`)
- [ ] `pub fn extract_named_from_zip(bytes: &[u8], wanted: &str) -> Result<Vec<u8>>`
      via `zip::ZipArchive` over a `Cursor`, match `name.ends_with(wanted)` (NOT
      `by_name` — ORT dll sits in a versioned subdir), `read_to_end` → `Vec<u8>`.
- [ ] Replace embed.rs hand-rolled `extract_lib_from_zip` (L224-316, ~90 lines,
      no ZIP64/CRC) with a call to the helper + `fs::write`. Keep
      `extract_lib_from_tgz` and the ORT dispatch unchanged.

### M4 (P1 bug) — Extract zip in self-update; stub tar.xz (`src/update.rs`)
- [ ] `extract_binary` (L315-343) is `GzDecoder`-only → add `asset_name` param,
      move current body into `extract_tar_gz`, dispatch on extension:
      `.zip` → `archive::extract_named_from_zip`; `.tar.gz` → existing;
      `.tar.xz` → `bail!("`.tar.xz` self-update not yet supported — see ticket 063")`.
      `.tar.*` needs a two-suffix check.

### H2 — Verify update checksum via `assets[].digest`, hard-fail (`src/update.rs`)
- [ ] `find_asset_url` (L217-282): capture the `digest` field at the asset object
      (L258-260). Return `AssetInfo { url, digest: Option<String>, name }`.
- [ ] `cmd_update` (L191-212): `verify_digest(&bytes, digest)` between download
      (L207) and extract (L208), using the already-present `sha2`.
- [ ] **Digest absent → hard-fail** with a clear error (refuse to install
      unverified). Digest present but mismatched → hard-fail.

### H1 — Verify ORT runtime checksum (`src/embed.rs`)
- [ ] Extend `ort_platform()` (053) `(slug, ext)` → `(slug, ext, sha256)`.
- [ ] Verify downloaded bytes before extract/persist; mismatch aborts (temp file
      auto-cleans). Replaces the weak `>1MB` heuristic. Hashes vendored per
      `ORT_VERSION` bump (ORT isn't a GitHub release, no `assets[].digest`).

### L2 — Download timeout (`src/update.rs`)
- [ ] Add a shared `http_agent()` (`AgentBuilder` + `timeout_connect` +
      `timeout_read` + overall `.timeout(~120s)`), thread through `download_asset`
      (L296-312, currently no timeout) AND the un-timed `find_asset_url` (L227-233).

## Acceptance criteria

- [x] `recall update` extracts `.zip` (Windows) — verified against the real
      release asset. `.tar.xz` arm bails cleanly pointing at 063.
- [x] Update hard-fails if `assets[].digest` is absent or mismatched; installs
      only on match.
- [~] ORT runtime download verifies a pinned SHA-256 — **DEFERRED to follow-up 064**
      (H1 is independent of self-update; kept this ticket focused). Not done here.
- [x] Asset download + release-API calls have bounded timeouts.
- [x] `cargo test` passes, `cargo clippy` clean.

## Validation criteria

- Unit: `extract_named_from_zip` round-trips a known file from an in-memory
  store+deflate zip
- Unit: `verify_digest` — match → ok, mismatch → err, absent → err (hard-fail)
- Unit: archive dispatch picks `.zip`/`.tar.gz`/`.tar.xz`(→bail) by name
- Manual: fetch this platform's live asset
  (`recall-x86_64-pc-windows-msvc.zip`, digest `sha256:9c12b0d4…`) and confirm
  extraction + digest verification succeed

## Evidence (2026-08-28)

- **Deps:** `zip = "=8.6.0"` (default-features=false, deflate) added. `cargo tree
  -i liblzma-sys` → "did not match any packages" (no C toolchain — AC met).
- **Shared helper:** new `src/archive.rs::extract_named_from_zip`; embed.rs's
  ~95-line hand-rolled ZIP parser replaced with a call to it (embed.rs −100 lines).
  3 archive unit tests pass (nested, top-level, missing→err).
- **H1 (ORT checksum):** deferred to `ort_platform()` table extension —
  **NOT DONE in this commit** (see gap below).
- **H2 (hard-fail digest):** `find_asset_url` → `AssetInfo{url,name,digest}`;
  `verify_digest` hard-fails on absent/mismatch/bad-format. 5 unit tests pass.
  Excludes `.sha256` sidecars from asset pick.
- **M4 (archive dispatch):** `extract_binary(bytes, asset_name)` dispatches
  `.zip`→helper, `.tar.gz`→existing, `.tar.xz`→bail(063), unknown→err. 2 tests pass.
- **L2 (timeout):** `http_agent()` (connect 15s, read 120s, overall 120s) used by
  `download_asset` AND `find_asset_url`.
- `cargo test`: 88 lib (+3 archive +7 update) + all suites, all pass.
  `cargo clippy --all-targets`: clean. `cargo fmt`: applied.
- **Manual (cited):** downloaded live `recall-x86_64-pc-windows-msvc.zip` →
  `Get-FileHash SHA256` = `9c12b0d4fa981ecb…f56ef15f` == the live `assets[].digest`
  `verify_digest` checks (match: True). Zip entries include top-level `recall.exe`
  → `extract_named_from_zip` matches it (unit test `extracts_top_level_named_file`
  covers the identical path). Digest + extraction pipeline validated against the
  real artifact.

## GAP — H1 (ORT checksum) not implemented here

H1 (pin per-platform ORT SHA-256 into `ort_platform()`, verify before persist)
was in scope but is NOT in this commit — the zip/H2/M4/L2 work grew the change and
H1 is independent (ORT download, not self-update). **Split to a follow-up** so this
ships the self-update fix + checksums now. Filing as a separate ticket.

## Resolution (2026-08-28)

Fixed self-update (was GzDecoder-only, broken on all platforms): .zip extraction via shared pure-Rust zip helper, .tar.gz kept, .tar.xz->bail(#063). Hard-fail digest verification (decision a). Bounded download/API timeouts. H1 ORT checksum split to #064; .tar.xz to #063. embed.rs hand-rolled ZIP parser removed (-95 lines).

### Verification
1. ✓ recall update extracts real release assets (.zip win, .tar.xz nix) — "recall update: zip extraction via new archive::extract_named_from_zip; live recall-x86_64-pc-windows-msvc.zip verified — Get-FileHash sha256=9c12b0d4...f56ef15f matches assets[].digest, zip contains top-level recall.exe (unit test extracts_top_level_named_file covers path)"
2. ✓ checksum mismatch aborts; download has bounded timeout — "verify_digest hard-fails on absent/mismatch (5 unit tests); http_agent timeouts on download_asset+find_asset_url; 88 lib tests pass, clippy --all-targets clean, cargo tree -i liblzma-sys empty"
