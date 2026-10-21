---
id: "37"
title: "Explore config options for user-tunable behaviors"
status: in_progress
blocked_by: []
estimate: 2h
priority: high
---

# User-Tunable Config Options

## Context

recall currently has minimal configuration (RECALL_DB, RECALL_MODEL, FASTEMBED_CACHE_DIR
as env vars, and ~/.recall/config.toml with only telemetry settings). As the tool grows,
users need knobs to tune behavior without editing source.

## Review update — 2026-10-09

The architecture review promotes import-root configuration from a convenience
to a prerequisite for safe, portable operation. Root discovery is currently
duplicated in `import-all`, `sync`, and health coverage, and hard-codes both
`~/code` and `D:/code`. This ticket must define one shared project-discovery
API and make `import.roots`/`import.exclude` the first v0.2 configuration
surface. It remains the dependency for #060; #080 fixes root identity and data
loss independently.

The 2026-10-10 architecture review also found that the telemetry save path can
erase unrelated settings because configuration and `~/.recall` path resolution
have no owner. This ticket must decide the configuration document contract,
unknown-key preservation, and the local-state boundary implemented by #087.

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
6. **Ownership:** What one local-state/config boundary owns path resolution and
   targeted, lossless configuration updates?
7. **Forward compatibility:** Which unknown sections/keys must survive a
   read/modify/write by an older binary?

## Acceptance criteria

- [ ] Design document: which options to expose in v0.2
- [ ] Decision on config format and parsing approach
- [ ] Decision on CLI interface for config management
- [ ] Priority ranking of options (which ones users actually need first)
- [ ] One shared discovery implementation replaces the three hard-coded root
      scans before additional root-related behavior is added
- [ ] `import.roots` and `import.exclude` are included in the first delivery
- [ ] The design specifies full-document, lossless config update semantics and
      the local-state ownership boundary consumed by #087
