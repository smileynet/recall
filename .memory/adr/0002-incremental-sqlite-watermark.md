# ADR 0002 — Incremental SQLite ingest by updated_at watermark

- Status: accepted
- Date: 2026-10-02
- Ticket: 074

## Context

The kiro-cli session DB on a working host holds ~11k conversations (3.5 GB).
Re-embedding every conversation on each `recall sync` is prohibitively slow. The
JSONL source uses a file-stat scan cache (`scan_cache(path, mtime, size, hash)`)
for change detection, but that keys on per-file metadata and does not map to rows
in a foreign SQLite table.

`conversations_v2.updated_at` is a unix-millisecond timestamp with an index
(`idx_conversations_v2_updated_at`). It is the natural change signal — but
unix-ms collisions across rows are common, so a naive strict `>` watermark can
silently drop a row that shares the maximum millisecond with an already-seen row
(a permanent, self-hiding gap).

## Decision

Incremental ingest driven by an `updated_at` watermark stored in recall's `meta`
table under `kiro_sqlite_watermark_ms`.

1. **Query with `updated_at >= watermark`** (inclusive), not `>`. The inclusive
   boundary deliberately re-reads the row(s) at the max millisecond rather than
   risk dropping one.

2. **Idempotent sink absorbs the re-read.** Each conversation is stored under
   `source = "kiro-sqlite:<conversation_id>"` via
   `delete_chunks_by_source` + re-insert inside a `BEGIN IMMEDIATE` transaction.
   Re-reading a boundary row therefore replaces its chunks in place — the corpus
   size does not grow, no duplicates appear. The only cost is re-embedding at
   most the boundary row(s), not the whole table.

3. **Advance the watermark only after batches commit**, to the max `updated_at`
   observed in the batch (never from a wall clock). A crash mid-run leaves the
   watermark unadvanced, so the next run reprocesses rather than skips — safe
   because the sink is idempotent.

## Alternatives considered

- **Strict `>` with no overlap** — simplest, but drops equal-ms boundary rows.
  Rejected: the silent-gap failure mode is exactly what ticket 074 exists to
  prevent.
- **Composite cursor `(updated_at, conversation_id)`** with strict `>` — gives
  no-overlap AND no-gap, but adds query/bookkeeping complexity. Deferred: the
  `>=` + idempotent-sink approach is simpler and the re-embed cost of one
  boundary row per sync is negligible. Revisit if many rows routinely share the
  max millisecond.
- **Reuse the file-stat scan cache** — does not model DB rows. Rejected.

## Consequences

- `recall sync` re-embeds only new/boundary conversations, not all ~11k each run.
- Watermark lives in `meta`, separate from the file `scan_cache`; the two change
  detectors (file-stat for JSONL, updated_at for SQLite) coexist.
- Hard deletes in the source are not detected by a timestamp watermark (out of
  scope; would need soft-delete or reconciliation).
