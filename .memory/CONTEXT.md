---
type: glossary
title: "Context"
---

# Context

**Drawer**:
A single stored memory record — one chunk of content with its embedding, wing, room, and type. The unit of storage and retrieval.
_Avoid_: document (too big), row (implementation term)

**Wing**:
A project-scoped namespace in the database. Auto-derived from session cwd or explicit `--wing` flag. Enables scoped search.
_Avoid_: project (overloaded), namespace (too generic)

**Room**:
A category within a wing (defaults to "general"). Groups related memories.
_Avoid_: folder, tag

**Scan cache**:
Stored (mtime, size, content_hash) per file in the database. Enables <100ms change detection without re-reading file content.
_Avoid_: index (overloaded), manifest (implies a separate file)

**RRF (Reciprocal Rank Fusion)**:
The algorithm for combining BM25 (keyword) and vector (semantic) search results into a single ranked list. `score = 1/(k + bm25_rank) + 1/(k + vector_rank)`.
_Avoid_: hybrid search (too vague about the fusion method)

**Prime**:
The `recall prime` output — a self-contained payload of recent facts + top retrieval results, injected at session start.
_Avoid_: wake-up, context dump

## Embedding Model

The Python recall used bge-base-en-v1.5 (768-dim, stored as float32). The Rust binary matches this exactly. bge-small-en-v1.5 (384-dim) is supported via `RECALL_MODEL=bge-small` but embeddings are incompatible between models — switching requires full re-ingest. Quality difference between models is negligible (~3%) on our corpus; the choice is driven by migration compatibility.

## Environment

- All project repos live at `D:/code/` (migrated from `~/code` — discovery scan handles this via `is_dir()` guard)
- crew-research checkout is sparse — only `.memory/` is reliably present; don't expect tools/, AGENTS.md, or scripts
- Binary installed at `~/.cargo/bin/recall.exe`, model cache at `~/.recall/models/`

## Gotchas

- fastembed-rs defaults model cache to `.fastembed_cache` **relative to CWD**. We override to `~/.recall/models/` via `InitOptions::with_cache_dir()`. If this is ever removed, models download into random directories.
- Windows Task Scheduler `RepetitionDuration` max is ~999 days (use `New-TimeSpan -Days 999`), not `[TimeSpan]::MaxValue`.
- Search latency is ~1.5s warm, ~5s cold (model load). The bottleneck is loading all embeddings from disk for brute-force cosine. sqlite-vec would fix this.
