---
id: "17"
title: "Add missing CLI flags required for local deployment"
status: done
priority: high
blocked_by: []
estimate: 1h
---

# Missing CLI Flags for Deployment

Based on spike #016 comparison of Python vs Rust CLI.

## Must-have (blocks deployment)

1. **`--version` flag** — `recall --version` → `recall 0.1.0`
   - clap: add `#[command(version)]` to the Cli struct
   - Scripts and steering check this

2. **`import --force`** — delete existing imports for the wing, then re-import
   - Python: deletes all `import:%` source chunks for the wing, then reimports
   - Use: `recall import .memory/ --wing X --force`

## Should-have (improves agent workflow)

3. **`add --wing` optional** — auto-detect from cwd when not provided
   - Python: `Path.cwd().name.replace('-', '_')`
   - Agent write-back (`recall add "fact" --type decision`) shouldn't require --wing

4. **Active-file skip during ingest** — skip JSONL files with mtime < 5 minutes ago
   - Prevents ingesting sessions that are still being written
   - Simple: `if age_seconds < 300 { continue; }`

## Low priority (defer to post-deployment)

- `search --room`, `search --type` — rarely used filters
- `ingest --project` — scoped ingest (scheduled task doesn't use it)
- `forget --dry-run`, `forget --yes` — Rust version doesn't prompt so less needed
- `gc` command — `forget` covers basic cleanup
- `health --projects-root` — defaults work fine

## Acceptance criteria

- [x] `recall --version` outputs version
- [x] `recall import .memory/ --wing X --force` deletes and reimports
- [x] `recall add "fact" --type decision` auto-detects wing from cwd
- [x] Ingest skips files modified in last 5 minutes
- [x] All existing tests still pass
