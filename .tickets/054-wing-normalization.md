---
id: "054"
title: "Unify wing normalization (3 divergent schemes)"
status: done
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

## Proposal (2026-08-31, code-verified + research/review-backed)

Re-grepped current source and dispatched subagents for prior-art research and an
internal code/docs/config review. The diagnosis holds but was understated on
**every** axis: it is **7 wing-handling sites across 2 files, up to 4 divergent
schemes**, plus a user-`--wing` passthrough that normalizes *nowhere*.

### Verified sites (full inventory from code review)

| # | Location | Current transform | Scheme |
|---|----------|-------------------|--------|
| 1 | `cli.rs:223` (`wing_from_cwd`) | `.replace('-', "_")` | A: dashes only |
| 2 | `cli.rs:435` (`cmd_prime` inline dup of #1) | `.replace('-', "_")` | A |
| 3 | `cli.rs:327` (`cmd_import_all`) | `.replace('-', "_").replace('.', "")` | B: dash+dot(strip) |
| 4 | `cli.rs:399` (`cmd_sync`) | `.replace('-', "_").replace('.', "")` | B |
| 5 | `cli.rs:551` (`discover_project_coverage`) | `.replace('-', "_").replace('.', "")` | B — **missed by first pass; drives health coverage math** |
| 6 | `cli.rs:601` (`detect_wing_duplicates`) | `.replace(['-','.'], "_").to_lowercase()` | C — **4th scheme; lowercases but no run-collapse/space** |
| 7 | `ingest.rs:805,816` (`derive_wing_from_session`) | `.replace('-', "_")`, else `"sessions"` | A |
| 8 | user `--wing` on `add`/`search`/`prime`/`import`/`import-all`/`forget` | **none** (raw passthrough) | "none" |

Every `store.rs` wing comparison is exact SQL equality (`wing = ?`) or an
`import:{wing}:` prefix match — **nothing normalizes at read time**. So a row is
only reachable if producer and consumer applied the identical transform to the
identical input. That is precisely the 33 duplicate pairs. Read/compare paths are
safe under a shared function (they take whatever string they're given); the risk
lives entirely in the write side and in historical data.

### Canonical rule (validated against prior art)

```
normalize_wing(name):
  lowercase (NFKC + casefold if non-ASCII dir names are in scope)
  replace each of [ '-', '.', ' ' ] with '_'
  collapse runs of '_' to a single '_'
  trim leading/trailing '_'
  if empty -> "global"
```

Prior art directly supports this:
- **Python PEP 503** normalizes by lowercasing and collapsing *runs* of `[-_.]`
  to one separator — `Friendly-Bard`, `friendly.bard`, `friendly_bard`,
  `friendly--bard` all collapse to one key. This is the strongest match.
- **Cargo** treats `-`/`_` as a two-way equivalence class and canonicalizes to
  `_` (Rust identifiers can't hold `-`) — matches recall's existing `_`
  convention (`web_app`, `my_project`).
- **npm** validates only (no folding) — the anti-pattern that produced our bug.
- **Kubernetes** rejects rather than normalizes; borrow its *guardrails*:
  start/end-alphanumeric and a length cap.
- **Unicode UAX #31 / #15**: NFKC+casefold ("NFKC_Casefold") is the recommended
  identifier fold; NFKC does NOT strip accents (add NFD+strip only if we want
  pure-ASCII wings). Idempotent given no unassigned code points.

Collision caveat (from research): every fold step is many-to-one, so genuinely
distinct dirs (`a.b` vs `a-b`) collapse to one wing. The live health data implies
these are already duplicates in practice, but the migration must confirm no
*intentionally distinct* pair exists before merging (open question 2 below).

### Placement

`pub fn normalize_wing(&str) -> String` in `store.rs` (shared low-level module
both `cli.rs` and `ingest.rs` already depend on — no new module, no cycle).

**Normalize at TWO boundaries** (review finding — this is the correction that
makes all commands agree):
1. **Derivation sites** (1-5, 7): replace each `.replace(...)` chain with
   `normalize_wing`. Fold site 2's inline closure into `wing_from_cwd`. Point
   site 6 (`detect_wing_duplicates`) at `normalize_wing` too, keeping it as a
   permanent regression guard in `health` (should then report ~0 dupes).
2. **CLI arg boundary** (site 8): normalize the user `--wing` **once centrally in
   `run()`/dispatch right after `Cli::parse()`** — cleaner than per-handler and
   covers `add`/`search`/`prime`/`forget` (`Option<String>`) AND
   `import`/`import-all` (required `String`) uniformly, including future commands.

### Change list

- [ ] `store::normalize_wing` + unit tests (dashes, dots, spaces, case, repeats,
      empty, run-collapse `a--b`==`a_b`, PEP-503 parity `a.b`==`a-b`==`a_b`)
- [ ] Rewrite `wing_from_cwd` → `normalize_wing`; delete site 2's inline closure,
      call `wing_from_cwd`
- [ ] Sites 3,4,**5**,7: replace `.replace(...)` chains with `normalize_wing`
      (site 5 = `discover_project_coverage`, or `health` coverage math breaks)
- [ ] Site 6: `detect_wing_duplicates` calls `normalize_wing`; keep as regression check
- [ ] Site 8: central `--wing` normalization in `run()` (covers Option + required-String)
- [ ] Unify fallback default: cwd/prime use `"global"`, session uses `"sessions"`,
      import uses `unwrap_or_default()` → **empty-string wing**. Pick one
      (`"global"` via the `normalize_wing` empty→global rule) and document it.
- [ ] Grep gate: no `.replace('-', "_")` and no `.replace('.', "")` remain outside `normalize_wing`

### Acceptance additions (from review)

- Output of `normalize_wing` must be a plain identifier (never leaks an
  FTS-special char into a `MATCH` expression elsewhere).
- `discover_project_coverage` and `detect_wing_duplicates` must use the same
  function as the import derivations, or coverage/dup reporting silently diverges.

### Migration decision (needs sign-off before coding)

Two options for the ~33 already-fragmented pairs:

- **A — Leave historical, fix forward.** New writes land canonically; old variant
  wings age out. Zero risk. Ship this as 054's unit.
- **B — One-shot merge migration.** Bulk-merge existing variants. **Under-specified
  in the original: review found the wing is baked into THREE places**, not just
  `chunks.wing`:
  - `chunks.wing` (the column)
  - `chunks.source` — the `import:{OLD_WING}:{path}` prefix
  - `import_sources` composite PK `(path, wing)` — the manifest/hash-gate
  A coherent B must rewrite all three in one transaction, **or** force a full
  re-import of affected wings — otherwise the hash-gate re-imports or orphan-deletes
  on next sync. Migration best-practice (research): one immutable plan object,
  dry-run/preview using the *same* logic, tested backup (`VACUUM INTO`) immediately
  before, wrap in one `BEGIN IMMEDIATE…COMMIT`, guard on `PRAGMA user_version`,
  make the transform idempotent, deterministic newest-wins winner rule for colliding
  rows.

**Recommendation:** ship A as 054. File B as a **separate follow-up ticket**:
`tkt new wing-merge-migration --blocked-by 054 --blocked-by 055` (055 supplies the
process-lock B requires). Cross-reference **072** (import `--force` guard) —
its `import:{wing}:` delete key is normalization-sensitive; land **054 before 072**
so 072's guard is written against the canonical wing. 054 itself stays
`blocked_by: []` (ships standalone). Confirm A-vs-B and the follow-up ticket
before implementing.

Research/review artifacts: `.scratch/research/{slug-normalization,migration-patterns,prior-art}.md`,
`.scratch/review/{code-review,docs-review,config-tickets-review}.md`.

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

- [x] One `normalize_wing` function; no ad-hoc `.replace('-', "_")` chains remain
- [x] Session ingest and import produce identical wings for the same project dir
- [x] `cargo test` passes with normalization unit tests

## Validation criteria

- Unit test: `normalize_wing("sci.phoenix") == normalize_wing("sci-phoenix")`
- Grep: no `.replace('-', "_")` outside the shared function

## Resolution (2026-09-01)

Implemented Option B: store::normalize_wing (PEP-503-style: lowercase, fold [-. _ space]->_, collapse runs, trim, empty->global) wired into all 7 derivation/compare sites (wing_from_cwd, cmd_import_all, cmd_sync, discover_project_coverage, detect_wing_duplicates, derive_wing_from_session) plus central --wing normalization at the CLI boundary. Added recall migrate-wings (dry-run default, --yes applies in one tx with VACUUM INTO backup) rewriting chunks.wing, the import:{wing}: source prefix, and the import_sources PK (newest-wins on collision). Applied to live corpus (wings 117->84, dup pairs 33->0, 0 data loss, coverage 11->49/50). Commits a491f3b + 2b742bf.

### Verification
1. ✓ cargo test passes — "cargo test: 108 lib tests + all integration suites pass (8 normalize_wing unit tests + 6 wing_migration integration tests: three-place rewrite, collision newest-wins, idempotency, read-only plan); clippy --all-targets clean"
2. ✓ single normalization function used by all callers — "grep confirms zero ad-hoc .replace('-','_') / replace(['-','.']) chains remain outside store::normalize_wing; all 7 sites + the --wing CLI boundary route through the single function; live health verifies 33->0 duplicate pairs"
