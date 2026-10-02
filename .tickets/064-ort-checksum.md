---
id: "064"
title: "Verify ORT runtime download against pinned SHA-256 (H1 from 051)"
status: done
blocked_by: []
priority: medium
validation_criteria:
  - "ORT download mismatch aborts; pinned per-platform sha256"
---

# Verify ORT runtime download against pinned SHA-256 (H1 from 051)

## Context

Split from 051 (2026-08-28). 051 shipped self-update zip extraction + hard-fail
digest verification + timeouts. H1 (ORT runtime checksum) was in 051's scope but
deferred: it's independent of self-update (ORT download path in embed.rs, not the
GitHub self-update in update.rs), and 051 had grown large. Kept honest — 051's
ORT-checksum AC is marked deferred here, not falsely checked.

## Problem

`embed.rs::download_ort_runtime` verifies only a weak `>1MB` size heuristic before
persisting the ONNX Runtime library. A truncated/tampered archive that clears 1MB
poisons every later command. ORT is NOT a GitHub release, so `assets[].digest`
doesn't apply — the hash must be vendored per `ORT_VERSION`.

## What to build

- [ ] Extend `ort_platform()` (from 053) `(slug, ext)` → `(slug, ext, sha256)`;
      vendor the per-platform SHA-256 for ORT v1.20.0 (from Microsoft's release).
- [ ] In `download_ort_runtime`, verify `sha2::Sha256(bytes)` against the pinned
      hash BEFORE extract/persist; mismatch aborts (temp file auto-cleans via the
      053 atomic-persist path). Replaces the `>1MB` heuristic.
- [ ] Reuse the `verify_digest`-style compare (or a shared helper) if clean.
- [ ] Document that hashes are vendored per ORT version bump.

## Acceptance criteria

- [x] ORT download verifies a pinned SHA-256; mismatch aborts cleanly
- [x] Per-platform hashes vendored in the `ort_platform()` table
- [x] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit: ORT bytes vs pinned hash — match → ok, mismatch → err
- Manual: obtain the real ORT v1.20.0 archive SHA-256 for this platform and
  confirm it matches the vendored constant

## Resolution (2026-10-02)

Replaced the >1MB heuristic with pinned per-platform SHA-256 verification of the archive before extraction (ort_platform -> (slug,ext,sha256); verify_archive_sha256 aborts on mismatch; v1.20.0 hashes vendored for all 5 platforms from Microsoft's release). Root-caused + fixed a latent extraction bug the size check had masked: ort_lib_entry_matches matched libonnxruntime_providers_shared.so (14KB sidecar, first in tar order) via the macOS infix fallback -> stub with no OrtGetApiBase poisoned the cache. Now sidecars (libonnxruntime_*) are rejected; a post-extraction size assertion stays as defense-in-depth. Likely the root cause of the ~50 Load-model telemetry failures 068 cites.

### Verification
1. ✓ ORT download mismatch aborts; pinned per-platform sha256 — "Unit tests (verify_archive_sha256 match/mismatch, 64-hex pinned-hash, providers-sidecar rejection) pass; 113 lib tests green, clippy clean. Manual: real ORT v1.20.0 linux-x64 archive SHA-256 from Microsoft (aa70d48b...5930) equals the vendored constant, stable on re-fetch. End-to-end: deleted cached lib -> forced download -> SHA verify passed -> extracted correct 16.5MB libonnxruntime.so.1.20.0 (not the 14KB sidecar) -> recall add stored OK."
