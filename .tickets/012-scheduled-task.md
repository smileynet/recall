---
id: 12
title: "Replace Windows scheduled task with direct Rust binary"
status: open
priority: high
blocked_by: [11]
estimate: 30min
---

# Replace Scheduled Task

## What to build

Replace the current PowerShell wrapper (`Invoke-RecallIngestAll.ps1`) with a direct
binary invocation. The Rust binary handles its own locking and error reporting.

### New scheduled task

```powershell
$action = New-ScheduledTaskAction -Execute "D:\code\recall\target\release\recall.exe" -Argument "ingest"
$trigger = New-ScheduledTaskTrigger -Once -At "00:00" -RepetitionInterval (New-TimeSpan -Minutes 30)
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 1)
Register-ScheduledTask -TaskName "RecallIngest-Rust" -Action $action -Trigger $trigger -Settings $settings
```

### Import all projects

Also schedule or run once: `recall import <path> --wing <name>` for all projects with .memory/ dirs.

### Cleanup

- Disable legacy `RecallIngest` and `Recall-Ingest` tasks
- Remove Python profile hook from $PROFILE (or update it to call Rust binary)

## Acceptance criteria

- [ ] New scheduled task registered and running every 30 minutes
- [ ] Legacy tasks disabled
- [ ] `recall health --json` shows `last_ingest_ts` updating
- [ ] No Python/uv/venv dependencies in the task
