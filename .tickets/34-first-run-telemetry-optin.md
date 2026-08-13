---
id: "034"
title: "First-run opt-in prompt for telemetry and logging"
status: done
blocked_by: []
estimate: 1h
---

# First-Run Opt-In Prompt

## What to do

Before public release, telemetry and file logging should be OFF by default for new
users. On first run (no config file exists), prompt the user:

```
recall collects anonymous local usage data (command names, timing) to improve
the tool. No data leaves your machine. Enable? [Y/n]
```

### Behavior

| Scenario | Telemetry | Logging |
|----------|-----------|---------|
| First run, no config | Prompt user, save choice | Same prompt or bundle together |
| Config exists (enabled=true) | Active | Active |
| Config exists (enabled=false) | Inactive | Inactive |
| DO_NOT_TRACK=1 or CI=true | Inactive regardless | Inactive regardless |
| Dev machine (already has config) | No prompt, uses existing config | Same |

### Implementation

1. In `TelemetryConfig::load()`: if config file doesn't exist AND stdin is a TTY:
   - Print the prompt
   - Read y/n
   - Save to `~/.recall/config.toml`
2. If stdin is NOT a TTY (scheduled task, CI): default to disabled silently
3. Change `Default::default()` back to `enabled: false` for the public release
4. Our dev machine already has `config.toml` with `enabled = true` — unaffected

### Non-interactive fallback

If the binary is run non-interactively on first use (e.g., scheduled task before
any interactive run), silently default to disabled. The next interactive run will
prompt.

## Acceptance criteria

- [x] New users get prompted on first interactive run
- [x] Choice is persisted to config.toml
- [x] Non-interactive first run silently defaults to off
- [x] Existing users (config exists) are not prompted
- [x] DO_NOT_TRACK/CI overrides still work
- [x] Our dev machine: unchanged (config already exists with enabled=true)

## Release gate

This must be done before the public release binary reaches users. The v0.1.0
tag is already pushed but if CI builds succeed, we should do a v0.1.1 with
this change before promoting the release.

## Resolution (2026-08-12)

Implemented first-run opt-in prompt via `first_run_prompt()` in telemetry.rs. Default is disabled (N). Uses `std::io::IsTerminal` for TTY detection. Non-interactive environments silently save disabled config. Prompt outputs to stderr to avoid polluting command output. All 62 tests pass.
