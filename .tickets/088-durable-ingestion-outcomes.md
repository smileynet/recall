---
id: "088"
title: "Model durable session-ingestion outcomes and feed-scoped cursors"
status: open
blocked_by: ["083"]
env: either
priority: high
---

# Model durable session-ingestion outcomes and feed-scoped cursors

## Context

SQLite session ingestion advances `max_seen` before parsing each row. A
malformed older row can therefore be silently skipped once a later valid row
advances the persisted watermark. A changed conversation that produces no
chunks is skipped without removing its prior chunks. JSONL ingestion has the
same empty/unparseable ambiguity: it refreshes scan state without clearly
representing the replacement outcome.

The SQLite watermark uses one fixed metadata key and sources are keyed as
`kiro-sqlite:<conversation_id>`, even though the public API accepts arbitrary
SQLite database paths. Two source databases can therefore collide or inherit an
unrelated checkpoint.

The invariant is: each identified source revision has exactly one durable
outcome—`Indexed`, `KnownEmpty`, or `Rejected`—and a cursor may not pass an
unaccounted revision. Coordinate the cache mechanics with #079.

## What to build

1. Replace ambiguous `Option`/`continue` ingestion flow with an explicit
   outcome that records indexed, valid-empty, and rejected input.
2. Make a valid empty revision atomically remove its prior source chunks and
   record `KnownEmpty`.
3. Preserve prior indexed content for malformed or unsupported input, record a
   diagnosable durable rejection, and prevent later watermarks from hiding it.
4. Define durable source-feed identity and scope both SQLite source keys and
   checkpoint state to it. Account for recreated/restored databases and
   non-monotonic timestamps.

## Acceptance criteria

- [ ] Parse/index flow uses an explicit durable outcome rather than silently
      skipping empty or malformed source revisions.
- [ ] A valid empty revision atomically removes only its own prior chunks and
      records `KnownEmpty`.
- [ ] A rejected revision preserves prior content, is visible to diagnostics,
      and cannot be silently bypassed by a later watermark.
- [ ] Checkpoint and source-key identity distinguish separate, recreated, and
      restored SQLite feeds.
- [ ] The model is implemented through #083's migration framework, preserving
      existing corpus data safely.

## Validation criteria

- Integration tests cover malformed-old-plus-valid-new rows, valid-to-empty
  replacements, two source databases with matching conversation IDs, and a
  restored source whose timestamps move backward.
- JSONL tests cover empty/unparseable replacement behavior and preserve the
  cache guarantees established in #079.
