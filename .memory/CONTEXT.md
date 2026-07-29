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
