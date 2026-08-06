---
id: 33
title: "recall health reports log file location (nice-to-have from #020)"
status: done
priority: low
blocked_by: []
estimate: 10min
---

# Health Reports Log File Location

## What to do

Add log file information to `recall health` output:

```
  Last log:        ~/.recall/logs/2026-08-06.log (1.2 KB)
```

Use `logging::current_log_path()` which already exists.

## Acceptance criteria

- [ ] `recall health` shows log file path and size when it exists
- [ ] Shows "(no log)" when no log file exists
