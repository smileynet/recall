---
id: "012"
title: "Replace Windows scheduled task with direct Rust binary"
status: done
priority: high
blocked_by: ["011", "017", "022"]
estimate: 30min
---

# Replace Scheduled Task

## Installation (from spike #021)

1. Copy `D:\code\recall\target\release\recall.exe` to `~/.cargo/bin/recall.exe`
2. This shadows the Python shim at `~/.local/bin/recall.exe` (cargo/bin is earlier in PATH)
3. Verify: `where recall` → `C:\Users\uosmi\.cargo\bin\recall.exe`

## New scheduled task

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
