---
id: "037"
title: "Explore config options for user-tunable behaviors"
status: open
blocked_by: []
estimate: 2h
---

# User-Tunable Config Options

## Context

recall currently has minimal configuration (RECALL_DB, RECALL_MODEL, FASTEMBED_CACHE_DIR
as env vars, and ~/.recall/config.toml with only telemetry settings). As the tool grows,
users need knobs to tune behavior without editing source.

## What to explore

Design a unified config system that lets users control:

### Search behavior
- `search.max_results` — default number of results (currently hardcoded 5)
- `search.rrf_k` — RRF fusion parameter (currently 60)
- `search.min_score` — threshold below which results are hidden

### Ingest behavior
- `ingest.session_dir` — override default session path
- `ingest.active_threshold_secs` — how recently modified = "active" (currently 300)
- `ingest.chunk_size` — target chunk size in chars (currently 800)

### Import behavior
- `import.roots` — list of directories to scan for .memory/ (currently hardcoded D:/code, ~/code)
- `import.exclude` — patterns to skip (e.g., "node_modules", "archived_*")

### Sync behavior
- `sync.import_interval` — only run import every Nth sync (e.g., every 4th = daily)

### Display
- `display.color` — enable/disable ANSI colors
- `display.quiet` — suppress progress output

### Update (from #035)
- `update.check` — enable/disable update checks
- `update.interval_hours` — minimum time between checks

### Telemetry (existing)
- `telemetry.enabled` — usage telemetry
- `telemetry.crash_reporting` — crash reports

## Design questions

1. **Format:** Stick with hand-rolled TOML parser or add `toml` crate?
2. **CLI:** `recall config get/set/list` subcommands?
3. **Precedence:** env vars > config.toml > defaults?
4. **Validation:** Error on unknown keys? Warn?
5. **Migration:** What happens when config schema changes between versions?

## Acceptance criteria

- [ ] Design document: which options to expose in v0.2
- [ ] Decision on config format and parsing approach
- [ ] Decision on CLI interface for config management
- [ ] Priority ranking of options (which ones users actually need first)
