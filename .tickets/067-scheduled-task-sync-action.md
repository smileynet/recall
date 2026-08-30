---
id: "067"
title: "RecallIngest task: switch action ingest -> sync and raise ExecutionTimeLimit above guard ceiling"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "RecallIngest scheduled task runs `recall sync` (not `recall ingest`), so scheduled runs refresh both sessions and .memory/ imports"
  - "ExecutionTimeLimit exceeds the 90-min guard timeout ceiling (e.g. PT2H) so the app-level timeout, not the scheduler, bounds runaway runs"
  - "AGENTS.md Deployment section updated to match the live task action + interval"
---

# RecallIngest task: switch action ingest -> sync and raise ExecutionTimeLimit above guard ceiling

## Context

Found by the docs/config review (`.scratch/subagent-raw/c2-docs-config.md`) plus
scheduling research (`r3-scheduling.md`).

**3a — stale scheduled action.** Ticket 25 shipped `recall sync` (ingest + import-all in
one lock-holding process) and *intended* to replace the task action, but its Resolution
says "scheduled task update deferred until soak-tested." AGENTS.md:80 and ticket 12 still
describe the task as running `recall ingest`. If the live task still runs `ingest`:
- scheduled runs never refresh `.memory/` project knowledge (needs `import-all`/`sync`) —
  a silent coverage gap;
- `import`/`import-all` then run via some *separate* path, producing exactly the
  cross-command lock contention behind the 62 errors in ticket 066.
Switching the action to `sync` makes one process hold the lock for the whole maintenance
cycle, eliminating cross-command contention by construction. [r3: prefer one guarded job]

**3b — ExecutionTimeLimit < job duration (latent data-integrity bug).** Ticket 12 sets
`-ExecutionTimeLimit` = 1h, but measured full ingest is ~68 min (one `sync` ran 59 min;
the guard's own default timeout is 90 min). The scheduler can kill a cold full ingest
mid-write. r3 Open-Q2 flags this. Raise the limit to comfortably exceed the 90-min guard
ceiling (e.g. `PT2H`) so the app-level timeout — not the scheduler — is the authority.

**3c — keep IgnoreNew + app-lock (already correct).** r3 [L4] confirms scheduler
`IgnoreNew` (skip) as primary + fs2/fs4 lock as defense-in-depth for manual runs is best
practice; prefer skip over queue for ingest. No change needed beyond documenting both
layers.

## What to build

- [ ] Verify the live `RecallIngest` action (`schtasks /query /tn RecallIngest /xml` or
      `Get-ScheduledTask`). Record actual action + interval + ExecutionTimeLimit.
- [ ] Update the task to run `recall sync` (keep 30-min interval, `IgnoreNew`).
- [ ] Set `ExecutionTimeLimit` to `PT2H` (or > 90-min guard ceiling).
- [ ] Update AGENTS.md Deployment to match (action = sync, correct interval + limit).
- [ ] Resolve the CONTEXT.md vs ticket-12 conflict on `RepetitionDuration`
      (~999 days cap vs `[TimeSpan]::MaxValue`) noted by c2.

## Acceptance criteria

- [ ] Task action is `sync`; scheduled runs refresh sessions + `.memory/` imports
- [ ] ExecutionTimeLimit > 90 min
- [ ] AGENTS.md matches the live task
- [ ] IgnoreNew + app-lock documented as the two overlap-prevention layers

## Validation criteria

- `Get-ScheduledTask RecallIngest | Select Actions, Settings` shows `sync` + PT2H
- `recall sync` runs end-to-end and holds the lock for the whole cycle

## Notes

- High-risk-ish: modifies a live scheduled task. Confirm the current action before
  changing. Reversible (re-point to `ingest`).
