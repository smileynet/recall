---
id: "082"
title: "Decide and lock the health JSON nullability contract"
status: open
blocked_by: []
env: either
priority: medium
---

# Decide and lock the health JSON nullability contract

## Context

The committed `cli_snapshot` fixture expects an empty database to produce
`"last_ingest_ts": 0`, while the current `HealthReport` serializes its
`Option<i64>` as `null`. The review deliberately did not accept the generated
snapshot because this is a machine-readable public-contract change, not a
cosmetic update.

## What to build

1. Decide whether unknown/never-ingested timestamps are represented as `null`,
   `0`, or an omitted field. Prefer semantic correctness and document the
   compatibility consequence.
2. Apply the decision consistently to JSON and human output, including other
   future optional health fields.
3. Update the contract/snapshot tests only after the representation is chosen;
   add an explicit serialization assertion so an incidental Rust-type refactor
   cannot change it again.

## Acceptance criteria

- [ ] Empty and populated databases have an explicit, documented timestamp
      representation.
- [ ] `health --json` has a stable compatible schema for the selected policy.
- [ ] Snapshot and contract tests agree without an unreviewed `.snap.new`.

## Validation criteria

- `cargo test --test cli_contract --test cli_snapshot` passes in the hermetic
  test environment established by #081.
