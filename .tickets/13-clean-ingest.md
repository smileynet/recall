---
id: "013"
title: "Clean full ingest: delete old DB, ingest all sessions + import all .memory/"
status: done
priority: high
blocked_by: ["011", "012"]
estimate: 30min (+ ~20min ingest time)
---

# Clean Full Ingest

## What to do

1. Delete or rename old Python DB: `~/.recall/recall.sqlite3` → `~/.recall/recall-python.sqlite3.bak`
2. Fresh ingest of all sessions: `recall ingest`
3. Import all projects with .memory/ directories:

```powershell
$projects = @(
    "C:\Users\uosmi\code",
    "D:\code"
)
foreach ($root in $projects) {
    Get-ChildItem $root -Directory | ForEach-Object {
        $mem = Join-Path $_.FullName ".memory"
        if (Test-Path $mem) {
            $wing = $_.Name -replace '-','_'
            recall import $mem --wing $wing
        }
    }
}
```

4. Verify with `recall health --json`
5. Verify search works: `recall search "what did we decide about authentication"`

## Acceptance criteria

- [ ] Old DB backed up (not deleted permanently)
- [ ] Fresh ingest completes (~1600 sessions, ~20 min)
- [ ] All discoverable projects imported
- [ ] `health --json` shows covered_projects matching discoverable_projects
- [ ] Search returns relevant results for known queries
