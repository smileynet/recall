---
id: "089"
title: "Make Python-database migration lossless or explicitly incomplete"
status: open
blocked_by: ["077"]
env: either
priority: medium
---

# Make Python-database migration lossless or explicitly incomplete

## Context

Direct migration currently skips a legacy drawer whose embedding is not
768-dimensional, but still completes and writes the migration guard. Invalid
legacy timestamps are converted to epoch zero, which fabricates historical data
and can cause a migrated memory to be removed by `forget --older-than`.

A completed migration must account for every source drawer and must never
invent a timestamp. This extends #077's source provenance and retry semantics;
use #083 if durable row-level outcomes or provenance require schema changes.

## What to build

1. Define an explicit outcome for each legacy drawer: migrated, intentionally
   rejected with source context, or requiring an explicit re-embedding path.
2. Do not report migration completion or write its guard until every source row
   has a durable, user-visible outcome.
3. Reject malformed timestamps with source context or represent them through a
   documented unknown/migration-time policy. Never coerce invalid input to the
   Unix epoch.
4. Apply the same accounting contract to direct-copy and `--embed` modes while
   retaining #077's exact source-provenance and force-reset behavior.

## Acceptance criteria

- [ ] Direct-copy migration either handles every valid source row or fails/
      requires explicit re-embedding; no drawer is silently lost.
- [ ] Invalid timestamps are rejected with source-row context or follow an
      explicit unknown-time policy, never epoch zero.
- [ ] The migration guard is written only when every source row has a durable
      explicit outcome.
- [ ] `--force` retry semantics from #077 remain correct for all outcomes.

## Validation criteria

- Migration tests cover wrong-sized/corrupt vectors and malformed timestamps.
- Follow-up `forget --older-than` tests prove malformed legacy timestamps cannot
  cause accidental deletion.
