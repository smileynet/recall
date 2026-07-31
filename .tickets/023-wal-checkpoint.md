---
id: 23
title: "Add WAL checkpoint after large batch operations"
status: open
priority: normal
blocked_by: []
estimate: 10min
---

# WAL Checkpoint After Large Operations

## Problem

After migrate (180K rows) or full ingest (8K+ chunks), the WAL file can grow large
(100s of MB). Without explicit checkpointing, the main DB file doesn't get the data
until SQLite's auto-checkpoint threshold (1000 pages / ~4MB) triggers gradually.

## Fix

Add `PRAGMA wal_checkpoint(TRUNCATE)` after:
- `migrate` completes
- `ingest` completes (when total_chunks > 100)
- `import` completes (when total_chunks > 100)

TRUNCATE mode moves WAL data into the main DB file AND truncates the WAL to zero bytes
(releases disk space).

## Acceptance criteria

- [ ] WAL file is small after large operations complete
- [ ] No performance impact on normal small operations (skip checkpoint when < 100 chunks)
