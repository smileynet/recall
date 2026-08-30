---
id: "069"
title: "telemetry: run redact_paths on error_type to prevent path/PII leak into telemetry.jsonl"
status: open
blocked_by: []
priority: medium
validation_criteria:
  - "record_event applies redact_paths() to error_type before writing"
  - "a unit test asserts an error string containing an absolute path is redacted to [PATH] in the stored event"
  - "cargo test passes"
---

# telemetry: run redact_paths on error_type to prevent path/PII leak into telemetry.jsonl

## Context

From telemetry code review `.scratch/subagent-raw/c3-telemetry-code.md` (Concern C2).

`record_event` (src/telemetry.rs:174-180) derives `error_type` as
`format!("{}", e).split(':').next()...` — the raw error Display string, with **no path
redaction**. `redact_paths()` exists but is applied only to crash reports, not telemetry
events. Error Display strings frequently embed drive letters / partial paths (e.g.
"Load model from D:\\..."), so absolute paths leak into `~/.recall/telemetry.jsonl`. For
a privacy-marketed, local-first tool this is a gap worth closing.

## What to build

- [ ] In `record_event`, wrap the derived `error_type` in `redact_paths(..)` before it is
      stored (telemetry.rs ~174-180).
- [ ] Add a unit test: an `anyhow::Error` whose message contains an absolute path stores
      an event whose `error_type` contains `[PATH]`, not the raw path.

## Acceptance criteria

- [ ] `error_type` is path-redacted in stored telemetry events
- [ ] Unit test proves redaction
- [ ] `cargo test` passes

## Notes

- Cheap, contained. `redact_paths` already handles Windows `[A-Z]:\...` and Unix
  `/home|/Users/...` patterns.
