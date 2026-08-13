---
id: "031"
title: "Deploy profile hook (background import on shell open)"
status: done
priority: low
blocked_by: ["029"]
estimate: 10min
---

# Deploy Profile Hook

## What to do

Place the profile hook script (designed in spike #018) and source it from the
PowerShell profile.

### Steps

1. Write `~/.recall/profile-hook.ps1` (from #018 decision)
2. Add `. "$env:USERPROFILE\.recall\profile-hook.ps1"` to PowerShell profile
3. Verify: open new shell, confirm no blocking delay
4. Verify: after 6h staleness, confirm background import fires

## Acceptance criteria

- [x] Script placed at `~/.recall/profile-hook.ps1`
- [x] Sourced from PowerShell profile
- [x] Shell opens without visible delay
- [x] Background import fires when stale (check logs after)
