---
id: "067"
title: "RecallIngest task: raise ExecutionTimeLimit above app-guard ceiling + fix stale docs"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "RecallIngest ExecutionTimeLimit is >= PT3H (strictly greater than the app guard's 2h SCALED_CEILING) so recall's own watchdog (exit 2) fires before the scheduler hard-kills"
  - "AGENTS.md, README, and ticket 12 describe the task's actual action (sync), interval (6h), and limit"
  - "The change is applied with Set-ScheduledTask and re-verified via Get-ScheduledTask"
---

# RecallIngest task: raise ExecutionTimeLimit above app-guard ceiling + fix stale docs

## Live task state (verified 2026-08-30 via Get-ScheduledTask)

| Setting | Live value | Notes |
|---------|-----------|-------|
| Action | `recall.exe sync` | already migrated from `ingest` (ticket 25 done) |
| Interval | `PT6H` (every 6h) | duration `P999D`, StopAtDurationEnd=True |
| ExecutionTimeLimit | **`PT30M`** | the problem |
| MultipleInstances | `IgnoreNew` | correct |
| Last run | result 0 | healthy |

## The problem (evidence-backed)

The scheduler kills `recall sync` at **30 minutes**, but a full sync (ingest+import)
is measured at ~59-68 min. So any substantial scheduled run is force-killed mid-work.

Research (`.scratch/subagent-raw/r4-kill-behavior.md`, `r5-timeout-coord.md`):

- **r4:** For a console/headless app, Task Scheduler's timeout kill is effectively an
  immediate hard `TerminateProcess` — WM_CLOSE lands nowhere (no window/message loop),
  and there is no SIGTERM equivalent. There is no graceful path when the *scheduler* is
  the killer. [L4]
- **r5:** Correct layering is inside-out: `worst-case runtime < APP watchdog < SCHEDULER
  limit <= interval`. The app watchdog must be **shorter** than the scheduler limit so
  the app self-terminates cleanly first (flush, checkpoint, release lock, distinct exit
  code); the scheduler limit is the outer backstop that only fires if the app watchdog
  failed. Inverting this makes the app watchdog dead code. [L4]

recall already HAS an app watchdog (`guard.rs`): default 90 min, scaled ceiling **2h**
(`SCALED_CEILING`), exits with **code 2** on trip (`c4-writepath-guard.md`). `sync` uses
the fixed 90-min timeout (cli.rs:362). But the scheduler's PT30M is *below* the app
watchdog, so the scheduler always wins — exactly the inverted anti-pattern.

**Data-integrity is NOT a blocker** (`c4`): all bulk writes are transactional
(`BEGIN IMMEDIATE`, per-file batch commit, `wal_checkpoint(TRUNCATE)` when >100 chunks),
WAL mode + `synchronous=NORMAL`. A hard mid-write kill = **lost work only, no corruption**
(WAL discards the incomplete trailing txn on reopen). Side artifacts (last_ingest marker,
.lock, meta row) are all idempotent/atomic. So the fix is about avoiding wasted/clipped
runs, not preventing DB damage.

## The fix

Raise `ExecutionTimeLimit` to **`PT3H`** — strictly greater than the app guard's 2h
ceiling — so recall's own watchdog (90 min for sync, exit 2) always fires first and
cleanly; the scheduler limit becomes a true backstop for a genuinely hung process.

```powershell
# Raise ExecutionTimeLimit PT30M -> PT3H. Everything else stays (action=sync,
# interval=PT6H, IgnoreNew, duration 999d).
$task = Get-ScheduledTask -TaskName "RecallIngest"
$task.Settings.ExecutionTimeLimit = "PT3H"
Set-ScheduledTask -TaskName "RecallIngest" -Settings $task.Settings
# Verify:
(Get-ScheduledTask RecallIngest).Settings.ExecutionTimeLimit   # -> PT3H
```

Why PT3H not PT2H: PT2H equals the app's `SCALED_CEILING`, a tie/race. PT3H keeps a clear
ordering `90min sync watchdog < 2h app ceiling < 3h scheduler < 6h interval`. [r5]

Interval (6h) already exceeds worst-case runtime (~68 min) with wide headroom, and
`IgnoreNew` + the fs2/fs4 app lock cover overlap. No interval change needed.

## Doc fixes (8 mismatches, from `c5-docs-accuracy.md`)

- [ ] AGENTS.md:80 — action `ingest`->`sync`; interval `30 min`->`6h`; add PT30M(->PT3H) limit + IgnoreNew
- [ ] README.md:134 — pipeline label implies ingest-only; note scheduled run is `sync` (ingest + import-all)
- [ ] ticket 12:22 `-Argument "ingest"` -> `sync`
- [ ] ticket 12:24 `-RepetitionInterval 30 min` -> `6h`
- [ ] ticket 12:25 `-RepetitionDuration [TimeSpan]::MaxValue` -> `999 days` (also aligns with CONTEXT.md:45)
- [ ] ticket 12:28 `-ExecutionTimeLimit 1 hour` -> `PT3H` (new value)
- [ ] ticket 12:47 acceptance "30 min interval" -> "6h"
- Correct-as-is (leave): ticket 12:27 IgnoreNew, CONTEXT.md:45 999-day cap, ticket 25 sync intent.

## What to build

- [ ] Apply the `Set-ScheduledTask` PT3H change; re-verify via Get-ScheduledTask
- [ ] Fix the 8 doc mismatches above
- [ ] (Optional follow-up, NOT this ticket) consider scaling sync's app timeout like ingest if session counts grow — advisory only (c4)

## Acceptance criteria

- [ ] ExecutionTimeLimit >= PT3H, verified live
- [ ] Docs (AGENTS.md, README, ticket 12) match live action/interval/limit
- [ ] app-guard-before-scheduler ordering documented

## Risk

Low, reversible. Only lengthens the pre-kill window; the 90-min app guard still bounds
runaway runs. Modifies a live OS task -> apply with user approval, revert via
`$task.Settings.ExecutionTimeLimit = "PT30M"`.
