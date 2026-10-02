---
id: "074"
title: "Ingest kiro-cli v3 SQLite sessions (ticket 28 reopening criteria met)"
status: open
priority: high
blocked_by: []
---

# Ingest kiro-cli v3 SQLite sessions

## Reopening criteria from ticket 28 are now met

Ticket 28 (done, 2026-08-06, at kiro-cli 2.16.1) said: *"Reopen this ticket when
`kiro-cli --version` reports 3.x OR session files appear with a different structure."*
Both conditions are now true. Research 2026-10-02
(`../crew-research/.scratch/research/kiro-v3-2026-10/`, verified on this host):

- kiro-cli **2.27.0** ships v3 as the **default** engine.
- Sessions now live in **SQLite**, not (only) JSONL:
  - Linux: `~/.local/share/kiro-cli/data.sqlite3` — **confirmed live, 3.5 GB, 10,984 rows**
  - macOS: `~/Library/Application Support/kiro-cli/data.sqlite3`
  - Table `conversations_v2`: `key TEXT` (session cwd/project dir), `conversation_id TEXT`,
    `value TEXT` (conversation serialized as JSON), `created_at`/`updated_at` (unix ms),
    PK `(key, conversation_id)`, indexed on `(key, updated_at DESC)`.
  - Older `conversations` table exists but is empty (superseded).

**Not broken yet:** on 2.27.0 the JSONL tree (`~/.kiro/sessions/cli/*.jsonl`) is still
dual-written alongside SQLite (verified: JSONL files timestamped seconds-fresh). So
`recall sync` still works today via the existing `parse_kiro_v2` path. **The risk** is
that the v3 migration guide says v2 sessions are not auto-migrated and tells users to
back up `~/.kiro/sessions` before upgrade — implying a true 3.0 build **stops the JSONL
dual-write**. When a user hits that build, `recall sync` silently ingests nothing new
from kiro sessions and reports success (last_ingest marker still written) — a
self-hiding memory gap.

## Current code (what needs the new path)

- `src/ingest.rs`: directory scan (`std::fs::read_dir`, ~line 122) over the session dir;
  `parse_session_file` (~line 474) dispatches to `parse_kiro_v2` (`version:"v1"` + `kind`,
  the current format) / `parse_kiro_v3` (an older variant, confusingly named) / `parse_codex`.
- `src/cli.rs`: default session dir `~/.kiro/sessions/cli` (~line 43).
- There is **no SQLite reader** — recall only reads JSONL/JSON files off disk.

## What to build

1. **SQLite session source** in `src/ingest.rs`: when `data.sqlite3` exists at the
   platform path, open it **read-only** (rusqlite `OpenFlags::SQLITE_OPEN_READ_ONLY` —
   the DB is actively written by kiro-cli; never take a write lock) and read
   `conversations_v2`. Each row's `key` → wing/room derivation (it's the project dir,
   same signal recall already derives wings from), `value` → parse the conversation JSON
   into `Vec<Message>`.
2. **Characterize `value` JSON shape** first (spike-sized): dump one row's `value`
   structure and map message/role/tool-result fields to recall's `Message` model. The
   crew-research session-analyzer (ticket 169) needs the SAME map — produce it once here
   (recall owns the canonical session-parsing patterns) and share it.
3. **Source selection**: prefer SQLite when present; keep JSONL scan as fallback for v2
   users. Dedup across sources if both are read in one sync (SQLite `conversation_id` vs
   JSONL filename UUID — verify they correlate or dedup by content hash).
4. **Platform paths**: resolve `~/.local/share/kiro-cli/` (Linux/XDG, honor
   `$XDG_DATA_HOME`) and `~/Library/Application Support/kiro-cli/` (macOS). Windows path
   TBD (likely `%LOCALAPPDATA%\kiro-cli\`) — confirm or defer with a logged gap.
5. **Zero-data guard**: if neither source yields sessions, `recall sync` must log a
   visible warning, not silently write the last_ingest marker on empty input.
6. **Incremental ingest**: use `updated_at` to ingest only rows newer than last_ingest
   (avoid re-embedding 10k+ conversations every sync).

## Dependencies / coordination

- `rusqlite` is a new dependency (check it isn't already pulled transitively). Keep it
  behind the same read-only, no-bundled-sqlite-if-system-available conventions recall
  uses elsewhere.
- Shares the `value` field map with crew-research ticket 169 (session-analyzer SQLite
  reader). Do the characterization here first.
- Nice-to-have context: crew-research spike 168 will also probe v3 — but recall can
  characterize `value` independently from the live DB on this host without waiting.

## Acceptance criteria

- [ ] `conversations_v2.value` JSON shape documented and mapped to recall's `Message` model
- [ ] `src/ingest.rs` reads sessions from `data.sqlite3` (read-only) on Linux + macOS paths
- [ ] JSONL path retained as fallback; SQLite preferred when present; no double-ingest of the same conversation
- [ ] Incremental ingest by `updated_at` (no full re-embed each sync)
- [ ] Zero-data sync logs a visible warning instead of silently succeeding
- [ ] `recall sync` ingests a v3 SQLite session and `recall search` returns content from it
- [ ] Read-only DB open verified (no write lock taken on the live kiro-cli DB)
- [ ] Unit/integration test with a fixture `data.sqlite3` (mirrors existing JSONL test fixtures)
