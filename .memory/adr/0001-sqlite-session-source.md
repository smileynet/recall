# ADR 0001 — kiro-cli v3 SQLite sessions as an ingestion source

- Status: accepted
- Date: 2026-10-02
- Ticket: 074

## Context

kiro-cli 2.27.0 ships the v3 engine by default and stores conversations in a
SQLite database (`data.sqlite3`, table `conversations_v2`) instead of — or in
addition to — the legacy JSONL tree under `~/.kiro/sessions/cli/`. On 2.27.0 the
JSONL files are still dual-written, so recall's existing `parse_kiro_v2` path
keeps working. The v3 migration guide implies a true 3.0 build stops the JSONL
dual-write; when a user reaches that build, `recall sync` would silently ingest
nothing new from kiro sessions and still report success — a self-hiding memory
gap. recall previously had no SQLite reader for session sources.

## Decision

Add a read-only SQLite session source.

1. **Read-only open.** Open `data.sqlite3` with
   `OpenFlags::SQLITE_OPEN_READ_ONLY` via a dedicated helper
   (`sqlite_source::open_readonly`). Do NOT reuse `store::open_db_at` — it opens
   read-write and runs `init_schema`. The kiro DB is actively written by a live
   process; recall must never take a write lock on it. Never set `immutable=1`
   (would pin a stale/corrupt snapshot of a changing file). A modest
   `busy_timeout` guards the brief checkpoint-window contention.

2. **Prefer SQLite when present; JSONL is the fallback.** In auto-detect mode
   (no explicit path), if `data.sqlite3` exists at the platform path, ingest it
   and skip the JSONL dir entirely, to avoid double-ingesting dual-written
   sessions. If the DB is absent, fall back to the JSONL scan (v2 users). An
   explicitly supplied `ingest <path>` still targets JSONL and errors if the
   path is missing (preserved behavior).

3. **Dedup by source namespace.** SQLite chunks use
   `source = "kiro-sqlite:<conversation_id>"`, distinct from the JSONL path
   namespace, so the two sources never clobber each other and a re-ingest of one
   conversation deletes+reinserts only its own chunks. JSONL filename UUIDs and
   SQLite `conversation_id` UUIDs are not assumed to share an id space.

4. **Parse `history`, not `transcript`.** Each `conversations_v2.value` carries a
   `history` list of turns (role-accurate) and a flat `transcript` (role-blind).
   Use `history`, honoring `valid_history_range` (inclusive end, confirmed from
   the live DB): user prose only from `user.content.Prompt.prompt` (skip
   `ToolUseResults`/`CancelledToolUses`); assistant prose from
   `assistant.{ToolUse|Response}.content`, with `[tool: <name>]` summaries for
   parity with `parse_kiro_v2`.

## Consequences

- recall keeps ingesting kiro sessions across the v3 cutover without user action.
- The canonical `value` -> message map is owned here and shared with
  crew-research ticket 169 (session-analyzer).
- `rusqlite` was already a dependency (`bundled`), so no new crate and the
  version-gated read-only-WAL behavior (SQLite >= 3.22.0) is guaranteed.
- Windows path (`%LOCALAPPDATA%\kiro-cli`) is implemented best-effort but not yet
  verified on a Windows host (logged gap).
