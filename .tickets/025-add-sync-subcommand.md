---
id: 25
title: "Add `recall sync` subcommand (ingest + import-all)"
status: open
priority: normal
blocked_by: []
estimate: 2h
---

# Add `recall sync` Subcommand

## What to build

A single subcommand that runs all periodic maintenance in one process: ingest session
files then import-all project `.memory/` directories. Replaces the current scheduled
task (which only runs `recall ingest`) with a single `recall sync` invocation.

### Why

- Single scheduled task, single action — simplest Windows Task Scheduler config
- Shares the embedder instance (saves ~500ms model cold-start)
- Can detect new `.memory/` directories internally and skip import if nothing changed
- Atomic locking — one fs2 lock for the entire maintenance window
- Future-proof: add pruning, health checks, compaction without touching scheduler config

### CLI interface

```
recall sync [--force] [--skip-import] [--skip-ingest]
```

| Flag | Behavior |
|------|----------|
| (none) | Run ingest then import-all |
| `--force` | Force reimport (bypass hash-gate) |
| `--skip-import` | Only run ingest |
| `--skip-ingest` | Only run import-all |

### Implementation sketch

```rust
// In cli.rs
Sync {
    #[arg(long)]
    force: bool,
    #[arg(long)]
    skip_import: bool,
    #[arg(long)]
    skip_ingest: bool,
}

// In dispatch:
async fn run_sync(...) {
    let embedder = Embedder::new()?; // load once
    if !skip_ingest {
        run_ingest(default_session_path(), &embedder, &store)?;
    }
    if !skip_import {
        run_import_all(&embedder, &store, force)?;
    }
    // Combined summary output
}
```

### New project detection (stretch goal)

Scan `D:/code/*/` for directories containing `.memory/` that aren't yet registered as
wings. Report new discoveries in output:
```
Sync complete: ingested 12 files (47 chunks), imported 2 new projects (kc2-ui-workshop, tkt)
```

## Deployment change

After implementing, update the scheduled task:
```powershell
$action = New-ScheduledTaskAction -Execute "$env:USERPROFILE\.cargo\bin\recall.exe" -Argument "sync"
Set-ScheduledTask -TaskName "RecallIngest" -Action $action
```

## Acceptance criteria

- [ ] `recall sync` runs ingest then import-all in one process
- [ ] Embedder loaded once, shared across both operations
- [ ] `--skip-import` and `--skip-ingest` flags work
- [ ] `--force` passed through to import-all
- [ ] Output summarizes both operations
- [ ] Existing `recall ingest` and `recall import-all` commands unchanged
