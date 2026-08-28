---
id: "064"
title: "Verify ORT runtime download against pinned SHA-256 (H1 from 051)"
status: open
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

- [ ] ORT download verifies a pinned SHA-256; mismatch aborts cleanly
- [ ] Per-platform hashes vendored in the `ort_platform()` table
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit: ORT bytes vs pinned hash — match → ok, mismatch → err
- Manual: obtain the real ORT v1.20.0 archive SHA-256 for this platform and
  confirm it matches the vendored constant
