---
id: "077"
title: "Fix migrate --force data duplication"
status: open
blocked_by: ["083"]
env: either
priority: high
---

# Fix migrate --force data duplication

## Context

The 2026-10-09 architecture review found that `recall migrate --force` does
not fulfil its CLI promise to clear the previous import. `run_migrate` deletes
only chunks whose source is exactly `"migrated"`, but both direct-copy and
re-embedding modes preserve each drawer's `source_file`/`source`. The delete is
therefore normally a no-op and the retry inserts duplicate chunks and FTS rows.

Do **not** use `migrate --force` against a populated destination until this is
fixed. This is a data-integrity issue, not merely a misleading progress message.
Implement any durable provenance schema through #083's migration framework.

## What to build

1. Define durable migration provenance for every destination chunk imported
   from a particular source database. Do not overload the user-visible source
   string: it can legitimately overlap with regular ingest sources.
2. Make a repeated migration without `--force` remain a no-op; with `--force`,
   atomically remove exactly the chunks and FTS rows associated with that source
   database before re-importing them.
3. Make the direct-copy and `--embed` paths obey the same provenance and reset
   semantics. Preserve unrelated chunks, including chunks with coincidentally
   equal source strings.
4. Reconcile the CLI help text, implementation comments, and user-facing
   progress output with the selected behavior. Decide whether an additional
   confirmation is warranted once `--force` becomes genuinely destructive.

## Acceptance criteria

- [ ] A first migration imports each source drawer once and records provenance.
- [ ] Repeating the same migration without `--force` makes no writes.
- [ ] Repeating with `--force` leaves the same chunk/FTS counts as a clean
      one-time migration, for both direct-copy and `--embed` modes.
- [ ] `--force` preserves unrelated destination chunks and source-colliding
      non-migrated chunks.
- [ ] Integration tests cover direct and re-embedded retries on a populated DB.

## Validation criteria

- `cargo test --test integration_expanded` (or a dedicated migration suite)
  proves no duplicate retrieval hits after a forced retry.
- `cargo test` and `cargo clippy -- -D warnings` pass.
