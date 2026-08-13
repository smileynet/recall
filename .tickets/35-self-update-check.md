---
id: "035"
title: "Self-update check on startup (configurable, enabled by default)"
status: done
blocked_by: ["030"]
estimate: 2h
---

# Self-Update Check

## What to do

On startup, check if a newer version of recall is available on GitHub Releases.
Notify the user if an update exists. Configurable and enabled by default.

### Behavior

- Check at most once per 24 hours (cache last-check timestamp)
- Non-blocking: check happens after command output, or in background
- Print a one-line notice if update available:
  ```
  recall: update available v0.1.0 → v0.2.0 (run `recall update` to install)
  ```
- Configurable via config.toml:
  ```toml
  [update]
  check = true         # enable/disable update checks
  interval_hours = 24  # minimum time between checks
  ```
- Respect `DO_NOT_TRACK=1` (suppress network calls)
- Skip when non-interactive (scheduled task shouldn't print update notices)

### Implementation sketch

1. On command exit (after output), check if `~/.recall/last_update_check` is older than interval
2. If stale: HTTP GET `https://api.github.com/repos/smileynet/recall/releases/latest`
3. Compare `tag_name` against current `env!("CARGO_PKG_VERSION")`
4. If newer: print notice to stderr
5. Write current timestamp to `~/.recall/last_update_check`

### `recall update` subcommand (stretch goal)

Download and replace the binary:
- Detect platform, download correct archive from latest release
- Extract binary, replace self (rename trick on Windows)
- Print "Updated recall v0.1.0 → v0.2.0"

### Disable

```
recall config set update.check false
# or
[update]
check = false
```

## Acceptance criteria

- [x] Update check runs on first interactive command (after 24h gap)
- [x] Notice printed when newer version exists
- [x] No network call when within interval, non-interactive, or DO_NOT_TRACK=1
- [x] Configurable via config.toml
- [x] `recall update` downloads and installs latest (stretch)

## Resolution (2026-08-13)

Implemented in src/update.rs. Checks GitHub Releases via ureq (5s timeout, max once per 24h). Non-blocking notice after command output. Configurable via [update] section in config.toml. `recall update` downloads correct platform archive, extracts binary, and replaces self. Skips in non-interactive or DO_NOT_TRACK environments. 69 unit tests pass.
