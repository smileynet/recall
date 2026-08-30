---
id: "067"
title: "RecallIngest task: switch action ingest -> sync and raise ExecutionTimeLimit above guard ceiling"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "RecallIngest scheduled task runs `recall sync` (not `recall ingest`), so scheduled runs refresh both sessions and .memory/ imports"
  - "ExecutionTimeLimit exceeds the 90-min guard timeout ceiling (e.g. PT2H+) so the app-level timeout, not the scheduler, bounds runaway runs"
  - "Task runs logged-off (S4U principal, not Interactive)"
  - "scripts/register-task.ps1 reproducibly registers/updates the task idempotently; pure ASCII"
  - "AGENTS.md Deployment section updated to match the live task action + interval"
---

# RecallIngest task: switch action ingest -> sync and raise ExecutionTimeLimit above guard ceiling

## Context

Found by the docs/config review (`.scratch/subagent-raw/c2-docs-config.md`),
scheduling research (`r3-scheduling.md` + `.scratch/research/schtasks-best-practices.md`),
and a live-state audit of the running task (2026-08-30).

### Live-state audit (2026-08-30)

| Setting | Current (live) | Wanted |
|---------|----------------|--------|
| Action.Arguments | `sync` (repointed this session from a deleted wrapper) | `sync` ✓ |
| ExecutionTimeLimit | `PT72H` (unset default) | `PT2H`+ (above the 90-min guard ceiling) |
| MultipleInstances | `IgnoreNew` ✓ | `IgnoreNew` |
| Principal.LogonType | **`Interactive`** | **`S4U`** (run logged-off) |
| Trigger | daily 00:00 + 30-min repetition ✓ | keep |

Note the drift: ticket #12 registered the task with `-ExecutionTimeLimit 1h`, but the
live task shows `PT72H` (the unset default) — so #12's registration was later
superseded/re-created without the limit. Either way it is wrong: 1h is *below* the job
duration, 72h is not a real bound. The action was earlier found pointing at a **deleted**
Python wrapper (`Invoke-RecallIngestAll.ps1`) → `LastTaskResult=64`; repointed to
`recall.exe sync` this session (64 → 0). That fix is an undocumented one-machine edit and
will regress — hence part B below.

### 3a — stale scheduled action → cross-command lock contention

Ticket 25 shipped `recall sync` (ingest + import-all in one lock-holding process) and
*intended* to replace the task action, but its Resolution says "scheduled task update
deferred until soak-tested." AGENTS.md:80 and ticket 12 still describe the task as running
`recall ingest`. When the task runs `ingest` only:
- scheduled runs never refresh `.memory/` project knowledge (needs `import-all`/`sync`) —
  a silent coverage gap;
- `import`/`import-all` then run via a *separate* path, producing exactly the cross-command
  lock contention behind the 62 errors in ticket 066.
Switching the action to `sync` makes one process hold the lock for the whole maintenance
cycle, eliminating cross-command contention by construction. [r3: prefer one guarded job]

### 3b — ExecutionTimeLimit vs the guard ceiling (latent data-integrity bug)

`src/guard.rs` enforces recall's own timeouts: `DEFAULT_TIMEOUT = 90min`,
`SCALED_CEILING = 2h`, `SCALED_FLOOR = 60s`. Measured full ingest is ~68 min (one `sync`
ran 59 min). The Task Scheduler `ExecutionTimeLimit` must sit **above** recall's guard
ceiling so recall's own graceful timeout (logs `TIMEOUT:`, exits 2) is the authority — an
OS kill below that truncates a legitimate long ingest mid-write and skips recall's logging.
#12's 1h limit is below the job; the live 72h is above but not a real bound. Set `PT2H`
(> the 90-min/2h guard) up to `PT3H` for headroom.

### 3c — keep IgnoreNew + app-lock (already correct)

r3 [L4] confirms scheduler `IgnoreNew` (skip) as primary + fs2/fs4 lock as
defense-in-depth for manual runs is best practice; prefer skip over queue for ingest. No
change needed beyond documenting both layers.

## What to build

### A. Correct the live task settings

- [ ] Verify the live action/settings (`Get-ScheduledTask` / `schtasks /query /xml`);
      record actual action + interval + ExecutionTimeLimit + principal.
- [ ] Action = `recall.exe sync` (keep 30-min interval, `IgnoreNew`).
- [ ] `ExecutionTimeLimit` = `PT2H` (or up to `PT3H`) — above the 90-min guard ceiling.
- [ ] Principal = S4U so it runs logged-off against the **user** profile (SYSTEM resolves
      `~` to the system profile and breaks `~/.recall` + `~/.kiro/sessions`):
      `New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType S4U -RunLevel Limited`.
- [ ] Battery flags: `-StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries`
      (keep the daily-00:00 + repetition trigger; repetition-only triggers don't recover
      missed ticks under `-StartWhenAvailable`).

### B. Make it reproducible (prevents the deleted-wrapper regression)

- [ ] `scripts/register-task.ps1` (pwsh; pure ASCII per #065) that creates or updates
      `RecallIngest` idempotently via `Register-ScheduledTask -Force` (atomic overwrite —
      NOT Unregister+Register, which drops run history).
- [ ] Resolve the binary from the deploy target (`$env:CARGO_HOME\bin\recall.exe` else
      `$env:USERPROFILE\.cargo\bin\recall.exe`, matching deploy-local.ps1) and **fail
      loudly if the binary is absent** — never register a dead-path task.
- [ ] `-DryRun` (print resolved path + rendered action/trigger/settings, write nothing)
      and `-Unregister` (symmetric removal) switches.
- [ ] Wire into deploy-local.ps1: replace the "RecallIngest not found" WARNING branch with
      a pointer to (or opt-in invocation of) this script.

### C. Docs

- [ ] Update AGENTS.md Deployment to match the live task (action = sync, interval, limit)
      and reference `register-task.ps1`.
- [ ] Resolve the CONTEXT.md vs ticket-12 conflict on `RepetitionDuration` (~999-day cap
      vs `[TimeSpan]::MaxValue`) noted by c2.

## Relationship to other tickets

- Completes the deferred deployment step from **#25** (done).
- Corrects the manual snippet in **#12** (done — registered `ingest`, `Interactive`, 1h).
- Fills the gap **#050** (done) left open ("Out of scope: creating/modifying the task").
- Reduces the cross-command contention **#066** addresses (single guarded `sync` job).
- **Superseded by #060** (`recall setup` cross-platform subcommand, blocked by #37). #060
  is the durable cross-platform fix; this Windows script is the unblocked stopgap, and B's
  `-DryRun`/`-Unregister` deliberately prewalk #060's `--dry-run`/`--uninstall` contract.
  Retire `register-task.ps1` when #060's Windows leg lands.

## Acceptance criteria

- [ ] Task action is `sync`; scheduled runs refresh sessions + `.memory/` imports
- [ ] `ExecutionTimeLimit` > 90 min (PT2H+); S4U principal; `IgnoreNew`; battery flags
- [ ] `scripts/register-task.ps1` registers/updates the task idempotently (`-Force`, no
      duplicate, no history-dropping Unregister); pure ASCII; fails loudly if binary absent
- [ ] `-DryRun` writes nothing and prints the plan; `-Unregister` removes the task
- [ ] AGENTS.md matches the live task and references the script; deploy-local.ps1
      "task not found" branch points to it
- [ ] IgnoreNew + app-lock documented as the two overlap-prevention layers
- [ ] After running + `Start-ScheduledTask`, `Get-ScheduledTaskInfo` shows
      `LastTaskResult=0` and `recall health` last-ingest updates

## Validation criteria

- `Get-ScheduledTask RecallIngest | Select Actions, Settings, Principal` shows `sync`,
  PT2H+, S4U.
- `recall sync` runs end-to-end and holds the lock for the whole cycle.
- Idempotency: run `register-task.ps1` twice → exactly one `RecallIngest` task.
- Byte scan: `register-task.ps1` has no byte > 0x7F (per #065).
- Note: Task Scheduler wraps process exit codes as HRESULT (`0x8007xxxx`); a raw `64` =
  launcher/file-not-found (the deleted-wrapper case), distinct from a recall binary error.

## Evidence backing this ticket

- Live-state audit (2026-08-30): action `recall.exe sync`, `ExecutionTimeLimit=PT72H`,
  `Principal.LogonType=Interactive`, `IgnoreNew`.
- Research `.scratch/research/schtasks-best-practices.md` — `-Force` idempotency,
  S4U vs SYSTEM/Interactive, `IgnoreNew`, 72h default ExecutionTimeLimit, exit-code →
  HRESULT mapping (Microsoft Learn + SO).
- Research `.scratch/research/setup-prior-art.md` — no single Rust crate schedules tasks
  on all of Windows+macOS+Linux (`service-install` = Linux-only; `service-manager` =
  daemons, no interval), so a Windows stopgap need not wait on a crate and #060 needs a
  hand-rolled 3-impl abstraction.
- `src/guard.rs` — `DEFAULT_TIMEOUT=90min`, `SCALED_CEILING=2h` (the ceiling the OS limit
  must exceed).

## Notes

- Medium-risk: modifies a live scheduled task. Confirm the current action before changing.
  Reversible (re-point to `ingest`).
