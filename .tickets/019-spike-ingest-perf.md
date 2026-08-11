---
id: 19
title: "Spike: ingest performance — why 20 min for 1600 files?"
status: done
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

- [x] Bottleneck identified with data
- [x] Proposed optimization with expected improvement
- [x] Decision: fix before deployment or defer?

## Impact

If ingest runs every 30 min via scheduled task, it needs to handle incremental changes
(typically 5-20 new files). Even at current speed, 20 files × ~5 chunks × 7.4/sec = ~14s.
That's fine for incremental. The 20-min problem only hits initial full ingest.

Verdict: probably defer — incremental is fast enough.

## Research Findings (2026-08-01)

Performance analysis from production deployment (2,765 files, 26K chunks):

- **Full ingest:** ~68 min (production), consistent with 135 chunks/sec embedding + per-file overhead
- **Incremental (no changes):** 42ms (stat-scan only, no embedding)
- **Incremental (typical):** seconds (5-20 files)
- **Bottleneck confirmed:** per-file transaction overhead + small batch sizes (hypothesis 2+3)
- **Model reloading:** NOT occurring — embedder is shared across the full run

### Why the gap exists (135 chunks/sec bench vs ~7.4 chunks/sec real)

1. **Small batch sizes:** Files with 1-5 chunks don't benefit from batch parallelism in the ONNX model
2. **Per-file DB transactions:** Each file does BEGIN/COMMIT (safe but adds latency per file)
3. **File I/O interleaved with embedding:** Read → chunk → embed → write → repeat (no pipelining)

### Optimization path (if ever needed)

- Batch chunks across files (embed in groups of 64 regardless of file boundaries)
- Pipeline: file read + chunking on one thread, embedding on another
- Expected improvement: 3-5× on full ingest (would bring 68 min → ~15-20 min)

### Decision: DEFER

The scheduled task runs every 6 hours on incremental changes. Typical incremental takes
seconds. Full ingest is a one-time event (migration from Python or fresh setup). The
optimization is not worth the complexity for the current deployment.

If a public release makes full ingest common (new users), revisit with ticket for
cross-file batching.

## Resolution (2026-08-11)

TBD
