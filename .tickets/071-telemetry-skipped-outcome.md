---
id: "071"
title: "telemetry: record graceful skip as distinct outcome (currently indistinguishable from success)"
status: open
blocked_by: []
priority: low
---

# telemetry: record graceful skip as distinct outcome (currently indistinguishable from success)

## Context

When `recall sync`/`ingest` finds the process lock already held by another instance,
`ProcessGuard::try_acquire()` returns `Ok(None)` and the process exits gracefully
(ticket 066). That graceful skip currently records a success-looking telemetry event:
`TelemetryEvent` carries only `exit_code: i32` and optional `error_type`
(`src/telemetry.rs:143-148`) — there is no field to express "ran but skipped work."
A skip is therefore **indistinguishable from a real success** in `telemetry.jsonl`,
which corrupts any success-rate / activity analysis.

Verified 2026-10-09 review: no `outcome`/`status`/`skipped` field exists; the only
`serde(skip_serializing_if)` in the file is on `error_type` (unrelated).

## Design decisions required (scope before implementing)

1. **Outcome taxonomy.** Minimum viable: `success | skipped | error`. Decide whether
   "skipped" needs a reason sub-field (lock-held vs. nothing-to-do vs. active-session).
2. **Schema field.** Add `outcome: Outcome` (enum, serialized as lowercase string) to
   `TelemetryEvent`. Keep `exit_code` for backward signal.
3. **Backfill / migration.** Existing `telemetry.jsonl` lines have no `outcome`. Decide:
   treat missing as `unknown` at read time (serde default), or leave old lines as-is
   (no rewrite — append-only log). Recommended: `#[serde(default)]` -> `Outcome::Unknown`.
4. **Emission points.** Set `outcome = Skipped` on the `Ok(None)` lock path
   (`src/guard.rs:40-42`); `Error` when an error is recorded; `Success` otherwise.

## Acceptance criteria

- [ ] `TelemetryEvent` has an `outcome` field with a defined taxonomy
- [ ] Graceful lock-skip (`try_acquire` -> `Ok(None)`) records `outcome: skipped`
- [ ] Old jsonl lines without the field deserialize without error (serde default)
- [ ] Unit test: a skipped run and a successful run produce distinguishable events
- [ ] Redaction behavior (ticket 069) preserved for any new string fields

## Notes

- Depends conceptually on the lock-skip path from ticket 066 (already shipped).
- Keep aligned with ticket 069's path-redaction guarantee if any new field carries text.
