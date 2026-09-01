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

## Proposal (2026-08-31, code-verified)

Re-grepped current source. The diagnosis holds but the count was understated:
it is **5 call sites across 2 files, 2 divergent schemes** (not "3 sites / 3
schemes"). Additionally, user-supplied `--wing` args are normalized *nowhere*,
which is a third divergence the original plan missed.

### Verified call sites

| # | Location | Current transform | Scheme |
|---|----------|-------------------|--------|
| 1 | `src/cli.rs:225` (`wing_from_cwd`) | `.replace('-', "_")` | dashes only |
| 2 | `src/cli.rs:435` (`cmd_prime` inline cwd) | `.replace('-', "_")` | dashes only |
| 3 | `src/cli.rs:327` (`cmd_import_all`) | `.replace('-', "_").replace('.', "")` | dashes + dots |
| 4 | `src/cli.rs:399` (`cmd_sync`) | `.replace('-', "_").replace('.', "")` | dashes + dots |
| 5 | `src/ingest.rs:807,818` (`derive_wing_from_session`) | `.replace('-', "_")` | dashes only |
| 6 | user `--wing` (`add`/`import`/`search`/`prime`) | **none** | raw passthrough |

Site 6 is why `recall add --wing my-project` files under `my-project` while
auto-derivation of the same dir yields `my_project`. Any fix that only touches
1-5 leaves this gap open.

### Canonical rule

```
normalize_wing(name):
  lowercase
  replace each of [ '-', '.', ' ' ] with '_'
  collapse runs of '_' to a single '_'
  trim leading/trailing '_'
  if empty -> "global"
```

Rationale: lowercasing + unifying the three separators covers every duplicate
pair observed in the live health output. Collapsing runs prevents `a--b` and
`a-.b` diverging. `"global"` fallback matches the existing `wing_from_cwd`
default.

### Placement

Put `pub fn normalize_wing(&str) -> String` in `store.rs` (already the shared
low-level module that both `cli.rs` and `ingest.rs` depend on — no new module,
no dependency cycle). `cmd_prime`'s inline closure (site 2) folds into a call to
`wing_from_cwd`, which itself calls `normalize_wing`.

### Change list

- [ ] `store::normalize_wing` + unit tests (dashes, dots, spaces, case, repeats, empty)
- [ ] Rewrite `wing_from_cwd` to `normalize_wing(file_name)`; delete the inline
      closure at site 2 and call `wing_from_cwd`
- [ ] Sites 3,4,5: replace the `.replace(...)` chains with `normalize_wing`
- [ ] Site 6: normalize `--wing` at the CLI boundary (normalize in each handler,
      or once in `dispatch` before the value is used) so user input and
      auto-derived wings converge
- [ ] Grep gate: no `.replace('-', "_")` remains outside `normalize_wing`

### Migration decision (needs sign-off before coding)

Two options for the ~33 already-fragmented pairs in the live corpus:

- **A — Leave historical, fix forward.** New writes land canonically; old
  variant wings persist until they age out. Zero risk, but the 33 pairs keep
  splitting search until re-ingested. Cheapest.
- **B — One-shot merge migration.** Add a `recall migrate-wings` (or fold into
  `health --fix`) that `UPDATE`s `wing = normalize_wing(wing)` across all rows,
  merging variants. Fixes the corpus immediately but is a bulk mutation —
  needs the process-lock (ticket 055) and a dry-run/count preview first.

Recommendation: ship the normalization (A behavior) as the mergeable unit, file
B as a **separate follow-up ticket** (`--blocked-by 054`) so the code fix isn't
gated on the riskier bulk mutation. Confirm before implementing.

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
