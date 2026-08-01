---
id: 20
title: "Auto-log to file when running non-interactively (scheduled task support)"
status: done
priority: normal
blocked_by: []
estimate: 30min
---

# Non-TTY Logging

## What to build

When recall detects it's running non-interactively (no TTY on stderr), auto-log to
`~/.recall/logs/YYYY-MM-DD.log` with timestamps. This captures scheduled task output
without needing a wrapper script.

### Detection

```rust
fn is_interactive() -> bool {
    atty::is(atty::Stream::Stderr) // or use std::io::IsTerminal (Rust 1.70+)
}
```

### Behavior

| Mode | stderr behavior |
|------|----------------|
| Interactive (TTY) | Print progress as-is (current behavior) |
| Non-interactive | Append timestamped lines to `~/.recall/logs/YYYY-MM-DD.log` |

### Log rotation

- One file per day
- Keep last 7 days (delete older on startup)
- Format: `[2026-07-30T15:30:00] Ingesting: ~/.kiro/sessions/cli`

## Acceptance criteria

- [x] Scheduled task output captured to log file automatically
- [x] Interactive use unchanged (no log file created)
- [x] Log rotation keeps 7 days
- [ ] `recall health` reports last log location/status (nice-to-have, deferred)

## Resolution (2026-08-01)

Implemented in commit 7989bdd. New module `src/logging.rs` with:
- `recall_log!` macro used throughout ingest/import code paths
- Non-TTY detection via `std::io::IsTerminal`
- Daily log rotation (7 days retained)
- 5 unit tests, 93 total passing
