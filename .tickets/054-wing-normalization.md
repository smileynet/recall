---
id: "054"
title: "Unify wing normalization (3 divergent schemes)"
status: open
blocked_by: []
priority: medium
validation_criteria:
  - "cargo test passes"
  - "single normalization function used by all callers"
---

# Unify wing normalization (divergent schemes)

## Context

From the 2026-08-23 review (deferred in 048). Wing names are derived from
directory names in multiple places using inconsistent normalization, so the same
project can land in different wings depending on the code path.

## Divergent schemes (verified in code)

- `wing_from_cwd` (`src/cli.rs:218`): `name.replace('-', "_")` — dashes only
- `derive_wing_from_session` (`src/ingest.rs:807,818`): `name.replace('-', "_")` — dashes only
- `cmd_import_all` / `cmd_sync` (`src/cli.rs`): `name.replace('-', "_").replace('.', "")`
  — dashes AND dots stripped

Consequence: a project dir like `sci.phoenix` becomes wing `sci_phoenix` via one
path but `sciphoenix` via another (the import path strips the dot). Session
ingest and import can disagree, splitting one project across two wings. Also no
`to_lowercase()` anywhere, so case differences fragment further.

## Live evidence (2026-08-30 deploy health check)

`recall health` on the production corpus reports **33 duplicate wing pairs** —
the same project split across hyphen and underscore variants. This confirms the
code-review diagnosis is causing real corpus fragmentation, not just a
theoretical risk. Sample pairs:

- `sci-phoenix` / `sci_phoenix`
- `crew-research` / `crew_research`
- `gdquest-vault` / `gdquest_vault`
- `torrent-stack` / `torrent_stack`
- `shadowrun-sega` / `shadowrun_sega`
- (+28 more; full list in the deploy health output)

Every pair is a search/prime blind spot: a query scoped to one variant misses
memories filed under the other. This makes the migration decision below
(re-ingest vs. leave historical) higher-priority — the corpus already has ~33
fragmented projects.

## What to build

- [ ] Add a single `normalize_wing(name: &str) -> String` (in store.rs or a util
      module) that all callers use
- [ ] Decide the canonical rule (recommend: lowercase + replace `[-. ]` with `_`,
      collapse repeats). Document it.
- [ ] Replace all three call sites with the shared function
- [ ] Unit tests covering dashes, dots, spaces, case, and repeats
- [ ] Note migration impact: existing corpus may have wings under the old scheme.
      Decide whether to re-ingest/re-import or leave historical wings as-is
      (document the decision).

## Acceptance criteria

- [ ] One `normalize_wing` function; no ad-hoc `.replace('-', "_")` chains remain
- [ ] Session ingest and import produce identical wings for the same project dir
- [ ] `cargo test` passes with normalization unit tests

## Validation criteria

- Unit test: `normalize_wing("sci.phoenix") == normalize_wing("sci-phoenix")`
- Grep: no `.replace('-', "_")` outside the shared function
