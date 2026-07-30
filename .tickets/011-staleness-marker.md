---
id: 11
title: "Write last_ingest marker + --version flag"
status: open
priority: high
blocked_by: []
estimate: 15min
---

# Staleness Marker + Version Flag

## What to build

### 1. last_ingest marker

After successful ingest, write current unix timestamp to `~/.recall/last_ingest`.
This is consumed by:
- Profile hooks (shell open checks staleness)
- doctor.sh health checks
- The `health --json` output (already reads this file)

### 2. --version flag

`recall --version` should output: `recall 0.1.0`
(clap already supports this via `#[command(version)]`)

## Acceptance criteria

- [ ] `~/.recall/last_ingest` written after successful ingest (contains unix timestamp as text)
- [ ] `recall --version` outputs version string
- [ ] Marker file not written on failed/empty ingest
