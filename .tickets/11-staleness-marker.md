---
id: "11"
title: "Write last_ingest marker after ingest"
status: done
priority: high
blocked_by: []
estimate: 10min
---

# Staleness Marker

## What to build

After successful ingest (when total_chunks > 0), write current unix timestamp
to `~/.recall/last_ingest`.

This is consumed by:
- Profile hooks (check staleness on shell open)
- doctor.sh (report ingest age)
- `health --json` `last_ingest_ts` field (already reads this file)

## Implementation

In `run_ingest()`, after the `Done` eprintln:
```rust
if total_chunks > 0 {
    let marker = home_dir().join(".recall").join("last_ingest");
    let _ = std::fs::write(&marker, unix_now().to_string());
}
```

## Acceptance criteria

- [x] `~/.recall/last_ingest` written after successful ingest
- [x] Contains unix timestamp as plain text
- [x] Not written on failed or empty (0 chunks) ingest
