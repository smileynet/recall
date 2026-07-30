---
id: 19
title: "Spike: ingest performance — why 20 min for 1600 files?"
status: open
priority: normal
type: spike
blocked_by: []
estimate: 30min
---

# Spike: Ingest Performance

## Question

The E2E validation showed 1,608 files → 8,844 chunks taking 20 minutes (1192s).
That's ~7.4 chunks/sec for embedding. The model benchmark showed 135 chunks/sec for batch.
Why is ingest 18× slower than raw embedding throughput?

## Hypotheses

1. **File I/O overhead** — reading 1,608 files sequentially
2. **Small batch sizes** — each file's chunks are embedded as a batch, but small files may have 1-5 chunks
3. **SQLite write contention** — per-file transactions (BEGIN/COMMIT per file)
4. **Model reloading** — is the model being loaded per-file? (shouldn't be, but verify)
5. **Memory pressure** — at 11GB RAM, GC/paging may be throttling

## What to do

1. Add timing instrumentation to `run_ingest`: time per-file vs embed vs DB write
2. Run a profiled ingest on ~100 files
3. Identify the bottleneck (is it embed, I/O, or DB?)
4. Propose fix (batch across files? larger embed batches? async I/O?)

## Success criteria

- [ ] Bottleneck identified with data
- [ ] Proposed optimization with expected improvement
- [ ] Decision: fix before deployment or defer?

## Impact

If ingest runs every 30 min via scheduled task, it needs to handle incremental changes
(typically 5-20 new files). Even at current speed, 20 files × ~5 chunks × 7.4/sec = ~14s.
That's fine for incremental. The 20-min problem only hits initial full ingest.

Verdict: probably defer — incremental is fast enough.
