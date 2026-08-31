---
id: "12"
title: "Replace Windows scheduled task with direct Rust binary"
status: done
priority: high
blocked_by: ["11", "17", "22"]
estimate: 30min
---

# Replace Scheduled Task

## Installation (from spike #021)

1. Copy `D:\code\recall\target\release\recall.exe` to `~/.cargo/bin/recall.exe`
2. This shadows the Python shim at `~/.local/bin/recall.exe` (cargo/bin is earlier in PATH)
3. Verify: `where recall` → `C:\Users\uosmi\.cargo\bin\recall.exe`

## New scheduled task

> **Superseded (2026-08-31, ticket 067):** the task originally shipped as shown below
> (`ingest`, 30-min interval, 1h limit). It has since been updated to run **`recall sync`**
> (ingest + import-all) every **6 hours** (`PT6H`, duration 999 days), with
> **`ExecutionTimeLimit PT3H`** (raised above the 2h app-guard ceiling so recall's own
> watchdog self-terminates before the scheduler hard-kills). `MultipleInstances IgnoreNew`
> is unchanged. The snippet below is the historical original.

```powershell
$RecallExe = "$env:USERPROFILE\.cargo\bin\recall.exe"
$Action = New-ScheduledTaskAction -Execute $RecallExe -Argument "ingest"
$Trigger = New-ScheduledTaskTrigger -Once -At "00:00" `
    -RepetitionInterval (New-TimeSpan -Minutes 30) `
    -RepetitionDuration ([TimeSpan]::MaxValue)
$Settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -StartWhenAvailable -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Hours 1)

Register-ScheduledTask -TaskName "RecallIngest" -Action $Action `
    -Trigger $Trigger -Settings $Settings -Force
```

## Cleanup

- The above `-Force` flag replaces the existing `RecallIngest` task
- Disable `Recall-Ingest` in `\CrewResearch\` path (legacy):
  ```powershell
  Disable-ScheduledTask -TaskName "Recall-Ingest" -TaskPath "\CrewResearch\"
  ```

## Acceptance criteria

- [x] Rust binary installed to ~/.cargo/bin/recall.exe
- [x] `recall --version` shows Rust version (not Python 0.2.0)
- [x] New scheduled task registered (30 min interval)
- [x] Legacy task disabled
- [x] `recall health --json` shows `last_ingest_ts` updating after task fires
