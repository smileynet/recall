---
id: "083"
title: "Add a versioned SQLite schema migration framework"
status: open
blocked_by: []
env: either
priority: high
---

# Add a versioned SQLite schema migration framework

## Context

Recall initializes schema with `CREATE ... IF NOT EXISTS`, but it has no
database schema version or ordered migration runner. The review identified
several imminent persisted-data changes: migration provenance (#077), import
root identity (#080), and the preference metadata planned by #044. Ad hoc
`ALTER TABLE` logic in each feature risks divergent upgrade paths and makes a
fresh database behave differently from a long-lived production corpus.

## What to build

1. Choose and document one schema-version source of truth (prefer SQLite's
   transactional `PRAGMA user_version`, or a clearly justified metadata key).
2. Introduce ordered, idempotent migrations executed inside appropriate
   transactions after opening an existing database. New databases must reach the
   same final schema directly.
3. Establish migration conventions: forward-only numbering, preflight checks,
   error reporting, and a test fixture representing an older production schema.
   Do not make every migration re-embed the corpus.
4. Use the framework for the next persisted-data feature, then have #077, #080,
   and #044 use it rather than inventing independent upgrade code.

## Acceptance criteria

- [ ] A database at the previous schema upgrades exactly once to the current
      schema while preserving chunks, FTS rows, metadata, scan cache, and import
      manifests.
- [ ] Reopening an upgraded database performs no schema writes and succeeds.
- [ ] A fresh database and upgraded database expose the same schema/invariants.
- [ ] A failed migration leaves a diagnosable error and no half-applied schema.
- [ ] Migration tests run without FastEmbed or a network dependency.

## Validation criteria

- Integration tests open fixtures at each supported predecessor version and
  assert data counts and searchability before/after upgrade.
- `cargo test` passes on a database created by the prior released binary.
