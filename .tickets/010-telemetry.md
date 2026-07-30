---
id: 10
title: "Add local telemetry + crash reporting (opt-in)"
status: open
priority: normal
blocked_by: []
estimate: 2h
---

# Local Telemetry + Crash Reporting

## What to build

Two separate opt-in systems:

### 1. Usage telemetry (local JSONL)

- Append events to `~/.recall/telemetry.jsonl`
- Event: command name, version, OS, arch, duration_ms, exit_code, error_type (class only), date (day granularity)
- Disabled by default
- Respect `DO_NOT_TRACK=1` and `CI=true`

### 2. Crash reporting (local file via human-panic pattern)

- On panic: write structured crash report to `~/.recall/crashes/`
- Include: version, OS, arch, panic message (redacted paths), command, backtrace (no file paths)
- Always writes locally (no network)

### CLI commands

```
recall telemetry status    — show what's enabled
recall telemetry enable    — enable usage telemetry
recall telemetry disable   — disable all
recall telemetry stats     — summary of local data
recall telemetry clear     — delete telemetry data
```

### Config

```toml
# ~/.recall/config.toml
[telemetry]
enabled = false
crash_reporting = true  # local crash files (no network)
```

## Acceptance criteria

- [ ] `recall telemetry status` shows current state
- [ ] When enabled: every command appends a JSONL event
- [ ] When disabled: no file writes beyond the config
- [ ] `DO_NOT_TRACK=1` overrides config (always off)
- [ ] Crash reports saved locally with no PII (paths redacted)
- [ ] No network calls (phase 1 is local-only)
