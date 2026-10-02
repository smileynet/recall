---
id: "074"
title: "Ingest kiro-cli v3 SQLite sessions (ticket 28 reopening criteria met)"
status: done
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
    PK `(key, conversation_id)`, indexed on `(key, updated_at DESC)` and `(updated_at DESC)`.
  - Older `conversations` table exists but is empty (superseded).

**Not broken yet:** on 2.27.0 the JSONL tree (`~/.kiro/sessions/cli/*.jsonl`) is still
dual-written alongside SQLite (verified: JSONL files timestamped seconds-fresh). So
`recall sync` still works today via the existing `parse_kiro_v2` path. **The risk** is
that the v3 migration guide says v2 sessions are not auto-migrated and tells users to
back up `~/.kiro/sessions` before upgrade — implying a true 3.0 build **stops the JSONL
dual-write**. When a user hits that build, `recall sync` silently ingests nothing new
from kiro sessions and reports success (last_ingest marker still written) — a
self-hiding memory gap.

---

## Findings from investigation (2026-10-02) — corrections to original premises

Four subagents reviewed the codebase and researched read-only SQLite + incremental
ingest. Raw findings: `.scratch/subagent-raw/review-ingest-code.md`,
`.scratch/subagent-raw/review-docs-config.md`, `.scratch/research/rusqlite-readonly.md`,
`.scratch/research/incremental-ingest.md`.

**Premise corrections:**

1. **`rusqlite` is NOT a new dependency.** It is already a direct dependency —
   `Cargo.toml:22`: `rusqlite = { version = "0.32", features = ["bundled", "vtab"] }`.
   `bundled` statically links a modern vendored SQLite, so `OpenFlags::SQLITE_OPEN_READ_ONLY`
   is available today with no Cargo change, and the version-gated read-only-WAL behavior
   (needs SQLite >= 3.22.0) is guaranteed across hosts. The "new dependency / coordination"
   section of the original ticket is moot.

2. **The scan entry point line references were off.** The real ingest scan is
   `scan::scan_for_changes` at `src/scan.rs:11` (jwalk, depth 1, `.jsonl` filter),
   invoked at `src/ingest.rs:181`. The `read_dir` at `src/ingest.rs:122` is only
   `count_session_files` (timeout scaling), not the ingest path.

3. **Read-only open is mandatory and verified.** On the live DB, `sqlite3` without
   `-readonly` returns `database is locked (5)`. Since the writer keeps `-wal`/`-shm`
   present, a `SQLITE_OPEN_READ_ONLY` open succeeds without creating files or needing
   directory write (SQLite >= 3.22.0 behavior). Do **NOT** use `immutable=1` on a live DB
   (stale/corrupt snapshots). Do **NOT** reuse `store::open_db_at` (`src/store.rs:14`) for
   the kiro DB — it sets WAL + runs `init_schema`, i.e. writes. Use a dedicated read-only
   helper: `Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)`, plus a
   modest `busy_timeout` (~2-5s) as free insurance against checkpoint-window `SQLITE_BUSY`.

**`conversations_v2.value` JSON shape — characterized from the live DB (AC #1 largely done):**

Top-level keys: `conversation_id, next_message, history, valid_history_range, transcript,
tools, context_manager, latest_summary, model_info, ...`.

- **Use `history` (role-accurate), not `transcript`.** `history` is a `List<Turn>`;
  each `Turn` has `user`, `assistant`, `request_metadata`.
  - `turn.user.content` is a tagged enum — only `{"Prompt": {"prompt": "<text>"}}` carries
    user prose. **Skip** `ToolUseResults` and `CancelledToolUses`.
  - `turn.assistant` is a tagged enum — both `{"ToolUse": {...}}` and `{"Response": {...}}`
    carry assistant prose in `.content`. `ToolUse.tool_uses[].name` is the analogue of the
    v2 `[tool: <name>]` summary — reuse that convention for parity. Drop `thinking`
    (contains `redacted_content` byte arrays).
  - Honor `valid_history_range` `[start, end]` — ingest only the valid slice (confirm
    inclusive end; sample len 3 had range `[0,2]`).
- `transcript` (flat `List<String>`) is a viable fallback but loses role boundaries.

**Incremental ingest — equal-timestamp boundary is the central trap:**

`updated_at` is unix **ms**; collisions across rows are common. Strict `updated_at >
watermark` can silently drop rows sharing the max millisecond (permanent gap). Chosen
approach: **`updated_at >= watermark` + idempotent upsert sink** (the deliberate 1-ms
overlap re-reads a boundary row, idempotency absorbs it). The sink is already idempotent
per-source: `BEGIN IMMEDIATE` -> `delete_chunks_by_source(source)` -> re-insert
(`src/ingest.rs:213-228`), with `source = "kiro-sqlite:<conversation_id>"` as the dedup
key. Persist the watermark as max(`updated_at`) **observed in the batch** (never from
wall clock), to `meta` via `store::set_meta`/`get_meta` keyed e.g.
`kiro_sqlite_watermark_ms` — the file-based `scan_cache` does not fit DB rows. Advance the
watermark only after the batch commits (crash reprocesses, never skips).

**Dedup (AC #3):** JSONL filename UUID and SQLite `conversation_id` UUID are NOT a
guaranteed shared id space — do not assume filename == conversation_id. Primary strategy:
**prefer SQLite when `data.sqlite3` exists, skip the JSONL dir entirely**; distinct
`source` namespaces (`kiro-sqlite:<id>` vs JSONL path) prevent clobbering. Only read both
in one sync as an explicit fallback; if both are read, dedup by content hash of produced
chunk text.

**Docs/config constraints to honor:** CLI parity (AGENTS.md: "Same CLI interface") —
source selection must be **automatic** (detect `data.sqlite3`), not a new flag. No daemon
(open read-only, read, close — no long-lived connection). No network at runtime. Honor
`$XDG_DATA_HOME`; precedent is `default_sessions_dir` (`src/ingest.rs:109-116`). Keep
RECALL_DB (recall's own destination DB) and `data.sqlite3` (foreign source) strictly
distinct — recall's `fs2` process lock is on RECALL_DB only, never the kiro DB.

---

## Current code (what needs the new path)

- `src/ingest.rs`: scan via `scan::scan_for_changes` (`src/scan.rs:11`, called at
  `ingest.rs:181`); `parse_session_file` (`ingest.rs:475`) dispatches `parse_kiro_v3`
  (`:495`, an OLDER JSONL variant — confusingly named) / `parse_kiro_v2` (`:552`, current
  format) / `parse_codex` (`:671`). The new SQLite v3 is a **4th source**, selected by
  file existence, not by sniffing a JSONL line.
- `struct Message { role: Role, text: String }` (`ingest.rs:466`); reused by
  `chunk_messages` (`:729`), `classify_room` (`:812`), `store::normalize_wing`
  (`store.rs:62`). The SQLite path must produce `Vec<Message>` so these are reused unchanged.
- Store loop (`ingest.rs:213-228`): `BEGIN IMMEDIATE` -> `delete_chunks_by_source` ->
  `insert_chunk(conn, content, wing, room, "session", source, embedding)` (`store.rs:122`).
- last_ingest marker: `write_last_ingest_marker` (`ingest.rs:75`), called only when
  `total_chunks > 0` (`ingest.rs:252`). **This gate is per-source, not cross-source** —
  the zero-data guard (AC #5) must move up to the sync layer.
- There is **no SQLite reader** — recall only reads JSONL/JSON files off disk.

## What to build

1. **Characterize + lock the `value` map first (spike, mostly done above).** Confirm
   `valid_history_range` inclusivity against a few rows; finalize the user/assistant enum
   mapping. Record it as the canonical session-parsing map (recall owns this; share with
   crew-research ticket 169).
2. **Read-only SQLite source** in `src/ingest.rs` (new helper, do NOT reuse
   `store::open_db_at`): `Connection::open_with_flags(path, SQLITE_OPEN_READ_ONLY)` +
   `busy_timeout`. New `scan_sqlite_for_changes(db_ro, since_ms) -> Vec<(key,
   conversation_id, value, updated_at)>` querying `SELECT key, conversation_id, value,
   updated_at FROM conversations_v2 WHERE updated_at >= ?1 ORDER BY updated_at` (uses
   `idx_conversations_v2_updated_at`).
3. **New parser** `parse_kiro_v3_sqlite(value: &str) -> Option<Vec<Message>>` (distinct
   from the JSONL `parse_kiro_v3`): map `history[valid_range]` per the characterization —
   `user.content.Prompt.prompt` -> User; `assistant.{ToolUse|Response}.content` ->
   Assistant (append `[tool: <name>]` summaries for parity); skip ToolUseResults /
   CancelledToolUses / thinking.
4. **Wing/room**: route `key` (project dir) through
   `store::normalize_wing(Path::new(key).file_name())`; reuse `classify_room` per chunk.
   `source = "kiro-sqlite:<conversation_id>"`.
5. **Source selection**: prefer SQLite when `data.sqlite3` exists; JSONL scan is the
   fallback for v2 users. Do not read both unless falling back (then dedup by content
   hash).
6. **Platform paths**: Linux `$XDG_DATA_HOME` else `~/.local/share/kiro-cli/`; macOS
   `~/Library/Application Support/kiro-cli/`; Windows `%LOCALAPPDATA%\kiro-cli\` — confirm
   or defer with a logged gap.
7. **Incremental ingest**: `updated_at >= watermark` + idempotent per-source upsert;
   persist max-observed `updated_at` to `meta` (`kiro_sqlite_watermark_ms`) only after
   commit. Never full-re-embed 10k+ conversations.
8. **Zero-data guard (move to sync/orchestration layer)**: if NEITHER source yields new
   conversations, log a visible warning and do **not** write the last_ingest marker as
   success.

## Dependencies / coordination

- `rusqlite` already present (`bundled`) — no Cargo change for the open; confirm no extra
  feature needed.
- Shares the `value` field map with crew-research ticket 169 (session-analyzer SQLite
  reader). Characterize here first (done above).
- crew-research spike 168 also probes v3 — recall characterized `value` independently
  from the live DB on this host; no need to wait.

## ADRs to write (none exist yet — net-new `.memory/adr/`)

- **ADR 0001** — SQLite as a session source: read-only open, prefer-when-present + JSONL
  fallback, cross-source dedup strategy.
- **ADR 0002** — Incremental ingest by `updated_at` watermark vs the file-stat scan cache
  (the model gap), including the `>=` + idempotent-sink equal-timestamp decision.
- Update `.memory/CONTEXT.md` glossary to disambiguate recall's own DB (RECALL_DB) vs the
  foreign kiro session-source DB, and note the three "v3" meanings (JSONL `parse_kiro_v3`
  = old variant; SQLite = the real v3). Update AGENTS.md ingest/architecture description
  and README once it lands.

## Acceptance criteria

- [x] `conversations_v2.value` JSON shape documented and mapped to recall's `Message`
      model (via `history` + `valid_history_range`, not `transcript`); `valid_history_range`
      inclusivity confirmed
- [x] Read-only SQLite source in `src/ingest.rs` using `SQLITE_OPEN_READ_ONLY` + a
      dedicated helper (NOT `store::open_db_at`); reads `conversations_v2` on Linux + macOS paths
- [x] Read-only DB open verified — no write lock taken on the live kiro-cli DB (test asserts
      a write attempt errors)
- [x] JSONL path retained as fallback; SQLite preferred when `data.sqlite3` present; no
      double-ingest of the same conversation (`source = kiro-sqlite:<conversation_id>`)
- [x] Incremental ingest by `updated_at >= watermark` with idempotent per-source upsert;
      watermark persisted to `meta` only after batch commit (no full re-embed each sync;
      equal-ms boundary rows not dropped)
- [x] Zero-data sync (neither source yields new conversations) logs a visible warning at the
      sync layer and does NOT write the last_ingest marker
- [x] `recall sync` ingests a v3 SQLite session and `recall search` returns content from it
- [x] Unit/integration test with a fixture `data.sqlite3` (`conversations_v2` with a
      realistic `value` payload), mirroring `tests/integration_test.rs::test_ingest_from_fixtures`
      and using `common::shared_embedder()`
- [x] Windows path confirmed or deferred with a logged gap

## Resolution (2026-10-02)

Added read-only kiro-cli v3 SQLite session source (src/sqlite_source.rs + parse_kiro_v3_sqlite in ingest.rs). Prefers SQLite when data.sqlite3 present, JSONL fallback, dedup via kiro-sqlite:<conversation_id>. Incremental by updated_at>=watermark in meta + idempotent sink (ADR 0002). Zero-data guard at orchestration layer. ADR 0001/0002, glossary, AGENTS.md updated. Windows path best-effort, deferred as logged gap.
