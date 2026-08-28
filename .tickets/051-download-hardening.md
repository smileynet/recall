---
id: "051"
title: "Harden self-update and ORT download: checksums, zip support, timeout"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "cargo test passes"
  - "checksum mismatch aborts download without persisting"
---

# Harden self-update and ORT download: checksums, zip support, timeout

## Context

From the 2026-08-23 full code review (`.scratch/review/embed-update.md`). Ticket
048 handled atomic writes, corrupt-cache detection, zero-size tar skip, and
Windows update rollback. These download-integrity items were deferred.

## What to build

### H1 — Checksum verification for the ORT runtime download (`src/embed.rs`)
The ORT archive is fetched over HTTPS but never validated. A truncated or
tampered download currently only fails the >1MB size sanity check.
- [ ] Pin per-platform SHA-256 for the ORT archive as constants (keyed to `ORT_VERSION`)
- [ ] Verify the downloaded bytes before extract/persist; abort on mismatch
- Note: upstream ORT GitHub releases don't ship per-asset `.sha256` sidecars, so
  the hashes must be vendored per version bump. Document that in the const block.

### H2 — Checksum verification for the self-update binary (`src/update.rs`)
The update archive is trusted implicitly.
- [ ] If the GitHub release includes a checksums asset (e.g. `SHA256SUMS`), fetch
      and verify against it before `replace_self`
- [ ] If no checksums asset exists, log a warning that integrity can't be verified

### M4 — Support zip archives in self-update extraction (`src/update.rs`)
`extract_binary` only handles tar.gz. If a release ships Windows as `.zip`
(common), update fails.
- [ ] Detect archive type by asset extension; extract from zip or tar.gz accordingly
- [ ] Reuse the zip-extraction logic already in `embed.rs` (factor into a shared helper)

### L2 — Download timeout (`src/update.rs`)
Version check has a 5s timeout but `download_asset()` has none — a stalled
download hangs indefinitely.
- [ ] Add a read/overall timeout to the asset download (e.g. 120s), surfacing a
      clear error on timeout

## Acceptance criteria

- [ ] SHA-256 verified before persisting the ORT runtime; mismatch aborts cleanly
- [ ] Self-update verifies checksums when available, warns when not
- [ ] Self-update extracts both zip and tar.gz
- [ ] Asset download has a bounded timeout
- [ ] `cargo test` passes, `cargo clippy` clean

## Validation criteria

- Unit test: checksum mismatch → error, no file persisted
- Unit test: archive-type detection picks zip vs tar.gz correctly
