---
id: "078"
title: "Prevent mixed-model embedding writes"
status: open
blocked_by: []
env: either
priority: high
---

# Prevent mixed-model embedding writes

## Context

`RECALL_MODEL` supports BGE-base (768 dimensions) and BGE-small (384). The
current mismatch check only prints a warning; `add` and ingest can still append
new-model vectors to an old-model corpus and then overwrite the database model
metadata. Vector search silently zips unequal vector lengths, producing invalid
scores rather than a clear failure.

The documented contract is that changing models requires re-ingestion. Enforce
that contract before a corpus can be contaminated.

## What to build

1. Make model compatibility a typed precondition for every embedding write
   path: add, JSONL ingest, SQLite ingest, markdown import, sync, and migration.
2. Treat stored model name and dimension as a first check, but validate existing
   embedding dimensions as a defensive backstop for old/corrupt metadata.
3. On mismatch, fail before any write with an actionable explanation and a
   supported re-index/rebuild path. Do not mutate model metadata on failure.
4. Define an explicit policy for read-only search on a mismatched corpus:
   either reject it or make any degraded mode opt-in and unmistakable.

## Acceptance criteria

- [ ] A clean/empty database initializes model metadata normally.
- [ ] Attempting to add, import, ingest, or sync with the other model fails
      before inserting chunks or changing metadata.
- [ ] Search never computes cosine similarity over unequal-length embeddings.
- [ ] A documented rebuild/re-index workflow is available for an intentional
      model switch.
- [ ] Tests cover base-to-small and small-to-base transitions.

## Validation criteria

- Tests prove chunk count, FTS count, and metadata are unchanged after each
  rejected write.
- Golden retrieval tests pass for an intentionally compatible corpus.
