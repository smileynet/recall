---
id: "055"
title: "recall health: report process-lock status; guard forget"
status: open
blocked_by: []
priority: low
validation_criteria:
  - "cargo test passes"
  - "health shows lock held/free"
---

# recall health: report process-lock status; guard forget

## Context

From the 2026-08-23 review (deferred in 048). Two low-priority operational
improvements around the process lock introduced in the guard work.

## What to build

### Lock status in `recall health` (`src/cli.rs` cmd_health, `src/guard.rs`)
Diagnosing "why isn't my scheduled task running / why did my command skip" is
hard without visibility into the lock.
- [ ] Add a helper in guard.rs that non-blockingly probes the lock (try-acquire,
      release immediately) and reports held/free
- [ ] Surface in `recall health` (and `--json`) as e.g. `process_lock: free|held`
- [ ] If held and the lock file has a PID, show it

### Guard `forget` (`src/cli.rs` cmd_forget)
`cmd_forget` deletes without the process lock. A concurrent ingest could
re-insert chunks that `forget` just deleted (logical race; WAL prevents hard
corruption). Low risk but easy to close.
- [ ] Acquire `ProcessGuard` in `cmd_forget` (skip gracefully if held)

Note: coordinate with 052 (forget confirmation) — both touch `cmd_forget`.

## Acceptance criteria

- [ ] `recall health` shows process-lock status (free/held + PID if held)
- [ ] `cmd_forget` acquires the process lock
- [ ] `cargo test` passes; cli_contract/snapshot updated if health output changed

## Validation criteria

- `recall health --json` includes a `process_lock` field
- Manual: hold the lock (long ingest) → `recall health` shows "held"
