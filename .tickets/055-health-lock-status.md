---
id: "055"
title: "Centralize exclusive corpus-operation policy and report lock status"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "cargo test passes"
  - "health shows lock held/free"
---

# Centralize exclusive corpus-operation policy and report lock status

## Context

From the 2026-08-23 review (deferred in 048). Two low-priority operational
improvements around the process lock introduced in the guard work.

## Review update — 2026-10-09

The current review confirmed that `forget` is still the exception among
mutating maintenance commands: ingest/import/sync/migrate acquire
`ProcessGuard`, while `forget` does not. WAL prevents physical corruption, but
an overlapping ingest can restore content a user just deleted.

The 2026-10-10 architecture review found a broader policy problem: lock
acquisition, graceful contention behavior, and timeouts are independently
implemented in multiple command handlers. `migrate-wings` and `add` also mutate
the corpus but have no explicit concurrency classification. Raise this to high:
the fix must centralize the policy rather than add another bespoke guard branch.

## What to build

### Lock status in `recall health` (`src/cli.rs` cmd_health, `src/guard.rs`)
Diagnosing "why isn't my scheduled task running / why did my command skip" is
hard without visibility into the lock.
- [ ] Add a helper in guard.rs that non-blockingly probes the lock (try-acquire,
      release immediately) and reports held/free
- [ ] Surface in `recall health` (and `--json`) as e.g. `process_lock: free|held`
- [ ] If held and the lock file has a PID, show it

### One corpus-operation policy

Put exclusive lock acquisition, lock-held result, timeout selection, and health
diagnostics behind one concrete operational-policy implementation. Long or
corpus-wide commands must use it instead of hand-copying lock branches. Every
mutating command, including `forget`, `migrate-wings`, and `add`, must be
explicitly classified as exclusive or as a safe short atomic write with a
tested rationale.

Do not add a generic lock-backend trait: Recall has one process lock. The goal
is one policy, not a shallow abstraction.

Note: coordinate with 052 (forget confirmation) — both touch `cmd_forget`.

## Acceptance criteria

- [ ] `recall health` shows process-lock status (free/held + PID if held)
- [ ] All multi-step or corpus-wide mutations use one exclusive-operation
      policy; exception classifications are documented and tested
- [ ] A held lock produces one consistent user-facing result and is represented
      in health text and JSON
- [ ] Timeout selection belongs to the same policy and preserves scaled-session
      behavior
- [ ] #060 initial setup sync uses this policy; #071 can record a lock-held run
      as skipped rather than successful
- [ ] `cargo test` passes; cli_contract/snapshot updated if health output changed

## Validation criteria

- `recall health --json` includes a `process_lock` field
- Manual: hold the lock (long ingest) → `recall health` shows "held"
