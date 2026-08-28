---
id: "056"
title: "Hygiene: delete duplicate 40/41/42 stubs, sync AGENTS.md modules/commands"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "no duplicate ticket IDs"
  - "AGENTS.md lists all modules and commands"
---

# Hygiene: delete duplicate 40/41/42 stubs, sync AGENTS.md modules/commands

## Context

From the 2026-08-23 review (deferred in 048). Ticket-file duplication and stale
AGENTS.md, both verified 2026-08-28.

## What to build

### Duplicate ticket stubs
There are unpadded duplicates of the padded tickets:
- `40-spike-auto-summarize.md` (295-byte stub) vs `040-spike-auto-summarize.md` (real, 2459 bytes)
- `41-spike-structured-state.md` vs `041-spike-structured-state.md`
- `42-spike-temporal-decay.md` vs `042-spike-temporal-decay.md`

`tkt ready` currently lists both (e.g. "40" and "040"). The unpadded stubs are
empty/duplicate.
- [ ] Confirm the padded (0NN) versions are canonical and contain the real content
- [ ] Delete the unpadded `40/41/42-*.md` stubs
- [ ] Re-run `tkt ready` / `tkt doctor` to confirm no duplicates remain

### AGENTS.md drift (verified against `src/*.rs`)
Workspace layout is missing modules that exist on disk:
- [ ] Add `guard.rs` (process lock + execution timeout)
- [ ] Add `logging.rs`, `telemetry.rs`, `update.rs`
- [ ] Update the recall CLI command list to include `sync`, `update`, `telemetry`
- [ ] Fix the stale ticket count line (".tickets/ — work tracking (23 tickets, 19 done)")
      or make it non-numeric to avoid re-staleness

### Optional: uncheck-vs-done drift on #36
- [ ] Reconcile #36 (learnable-preferences) status vs its checkbox state

## Acceptance criteria

- [ ] No duplicate ticket IDs (`tkt doctor` clean)
- [ ] AGENTS.md workspace layout lists all `src/*.rs` modules
- [ ] AGENTS.md recall CLI section lists all subcommands

## Validation criteria

- `tkt doctor` reports no duplicate/unparseable tickets
- Diff `src/*.rs` against the AGENTS.md layout → no missing module
