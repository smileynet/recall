---
id: "079"
title: "Make JSONL scan cache content-correct"
status: open
blocked_by: []
env: either
priority: high
---

# Make JSONL scan cache content-correct

## Context

The review found that `scan_cache.content_hash` is written but never consulted.
`scan_for_changes` stores modification time only to whole seconds and size:
metadata changes always trigger re-embedding, while a same-size rewrite within
one timestamp second can be missed. This contradicts the intended stat-gate
design: stat first, hash only when metadata differs, then re-ingest only if
content differs.

## What to build

1. Preserve sufficiently precise modification metadata (or deliberately
   invalidate existing coarse cache rows during migration).
2. When metadata is unchanged, skip hashing. When metadata differs, hash the
   file and compare it to the stored digest before classifying it as changed.
3. If content is identical, refresh the metadata/cache entry without parsing,
   embedding, or replacing chunks.
4. If content differs, retain the current transactional replacement behavior.
   Document the chosen digest length and collision rationale.

## Acceptance criteria

- [ ] A metadata-only touch does not re-embed or replace its session chunks.
- [ ] A content change is detected even when its file size is unchanged.
- [ ] Cache entries are refreshed after a metadata-only change.
- [ ] Existing scan-cache rows remain safe after upgrading.
- [ ] The documented stat/hash behavior matches the implementation.

## Validation criteria

- Unit/integration tests exercise cache miss, unchanged hit, touch-only change,
  same-size content change, and cache migration/invalidating legacy precision.
- Benchmark confirms unchanged scans remain inexpensive.
