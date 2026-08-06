---
id: 32
title: "Fix log file session-start spam from test invocations"
status: open
priority: low
blocked_by: []
estimate: 15min
---

# Fix Log File Spam From Tests

## Problem

When the test suite runs, each test that invokes the `recall` binary writes a
"session start" marker to the log file (because test processes are non-TTY).
This clutters the log with dozens of markers per test run.

## Options

A. **Check for RECALL_DB env var** — if set to a temp path, skip logging init
B. **Add a `--no-log` flag** — but that changes test harness setup
C. **Check parent process** — if parent is cargo-test, skip
D. **Only write session-start on commands that do real work** — skip for status/health/telemetry

## Recommended: Option A

If `RECALL_DB` points to a temp directory (contains "Temp" or "tmp"), suppress
file logging. Tests always set `RECALL_DB` to a tempdir.

## Acceptance criteria

- [ ] Test suite doesn't write to ~/.recall/logs/
- [ ] Real scheduled task runs still log normally
