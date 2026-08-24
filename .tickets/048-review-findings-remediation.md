---
id: "048"
title: "Review remediation - validated fixes from full code review"
status: open
blocked_by: []
priority: high
estimate: 4h
---

# Review Remediation — Validated Fixes from Full Code Review

## Context

A deep-dive review (2026-08-23) covered all source against AGENTS.md intent and the
ticket backlog. Overall health: solid B. All findings below were independently
verified against the code and production corpus before being recorded here.

Research (prior art, best practices) and code review (line-level) completed via
subagent dispatch (2026-08-23). Findings in `.scratch/research/` and `.scratch/review/`.

## Implementation Plan

### P1 — Timeout default kills documented workload (`src/guard.rs:16`)

**Problem:** `DEFAULT_TIMEOUT` = 30 min, but AGENTS.md documents full ingest at ~68 min.
The scheduled task's own guard kills its documented workload mid-flight.

**Research finding:** Linear scaling (`base + per_file * count`, clamped to ceiling) plus
an idle watchdog is the recommended pattern for CLI tools with predictable per-item cost.
Partial-result semantics (exit 124) are best practice for long-running ingestion.

**Implementation:**

1. Raise `DEFAULT_TIMEOUT` to 90 minutes (30% headroom over measured 68 min)
2. Add `install_timeout_for(command, file_count)` that computes:
   `max(60s, 100ms × file_count).min(2h)` — env var `RECALL_TIMEOUT` still overrides
3. Warn on invalid `RECALL_TIMEOUT` value (currently silent fallback)
4. Add guard to `cmd_migrate` (bulk write, same concurrency risk as ingest)

```rust
// guard.rs
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90 * 60);

pub fn install_timeout_scaled(file_count: usize) {
    let per_file = Duration::from_millis(100); // 4x safety over measured 24ms
    let floor = Duration::from_secs(60);
    let ceiling = Duration::from_secs(2 * 60 * 60);
    let scaled = floor + per_file * file_count as u32;
    install_timeout_with_default(scaled.min(ceiling));
}
```

- [ ] Raise `DEFAULT_TIMEOUT` to 90 min
- [ ] Add `install_timeout_scaled(file_count)` with floor/ceiling
- [ ] Warn on invalid `RECALL_TIMEOUT`
- [ ] Add guard to `cmd_migrate`
- [ ] Unit test: timeout parsing, scaling, disabled-when-zero

### P2 — Import manifest commits after chunks → duplicate imports (`src/ingest.rs`)

**Problem:** Chunk-insert transaction COMMITs at line 264; `upsert_import_source` runs
at line 268. Crash between → chunks stored, manifest stale → next import re-inserts.

**Code review confirmed:** Session ingest correctly wraps `scan::update_cache` inside
the same BEGIN/COMMIT block (lines 143–155). Import path does not.

**Implementation:** Single-line move — put `upsert_import_source` inside the existing
`BEGIN IMMEDIATE` / `COMMIT` block, matching the session ingest pattern.

```rust
conn.execute("BEGIN IMMEDIATE", [])?;
if is_update {
    store::delete_chunks_by_source(&conn, &source_key)?;
}
for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
    store::insert_chunk(&conn, chunk, wing, &room, &dtype, &source_key, embedding)?;
}
// Move INSIDE txn (was outside):
store::upsert_import_source(&conn, &rel_path, wing, &content_hash, file_size, chunks.len())?;
conn.execute("COMMIT", [])?;
```

- [ ] Move `upsert_import_source` inside the chunk transaction
- [ ] Test: verify re-import after simulated crash between old commit points produces no duplicates

### P3 — Migration writes to scan_cache; idempotence guard missing (`src/migrate.rs`)

**Problem:** Python `sources` rows → `scan_cache` is correct (Python tracked sessions),
but running `migrate` twice doubles the entire corpus. No guard prevents re-migration.

**Code review confirmed:** `migrate_sources` correctly targets scan_cache (Python sources
were session files). But no idempotence check exists anywhere — each run blindly re-inserts.

**Implementation:**

1. Add meta key `migrated_from:<sha256_of_source_path>` on successful completion
2. On entry: if meta key exists, bail with "already migrated from this DB"
3. Add `--force` to bypass (deletes existing migrated data first)
4. Validate each embedding row: `blob.len() == expected_dim * 4`, skip with warning on mismatch

```rust
let guard_key = format!("migrated_from:{}", sha256_hex(source_path));
if store::get_meta(&dst, &guard_key)?.is_some() && !force {
    eprintln!("recall: already migrated from this database. Use --force to re-migrate.");
    return Ok(0);
}
// ... do migration ...
store::set_meta(&dst, &guard_key, "1")?;
```

- [ ] Add idempotence guard via meta key
- [ ] Add `--force` flag that clears prior migrated data first
- [ ] Validate `embedding_blob.len() == expected_dim * 4` per row; warn+skip on mismatch
- [ ] Test: second migrate without --force exits cleanly

### P4 — Atomic downloads + self-update recovery (`src/embed.rs`, `src/update.rs`)

**Problem:** ORT runtime written directly to final path — truncated DLL poisons all
commands. Windows self-update: rename exe→`.old`, write new, delete backup immediately
— interrupted write leaves no working binary.

**Research finding:** `tempfile::NamedTempFile::new_in(target_dir)` + `sync_all()` +
`persist(target)` is the canonical Rust pattern (used by Cargo itself). SHA-256
verification should happen BEFORE `persist()`. Windows exe replacement needs rollback
if write fails (rename `.old` back).

**Implementation:**

embed.rs (ORT download):
1. Download to `NamedTempFile::new_in(lib_dir)`
2. Extract to temp path (not final)
3. Validate: `size > 1MB` minimum sanity check
4. Pin SHA-256 for each platform's ORT archive as const
5. `tmp.persist(final_path)` — atomic rename
6. Skip zero-size tar entries (Linux symlink hazard)
7. On startup: if `lib_path.exists() && size < 1MB`, delete and re-download

update.rs (self-update, Windows):
1. Download + extract to temp file (same dir as exe)
2. Verify SHA-256 against release checksums file
3. Rename current → `.old` (fails fast if rename blocked)
4. Write new binary to original path
5. **On write failure: restore `.old` → original** (rollback)
6. Do NOT delete `.old` — schedule cleanup on next successful launch
7. Add `cleanup_old_binary()` called at startup

update.rs (asset matching):
8. Use exact suffix match instead of `contains(target)` — resolve gnu vs musl ambiguity

```rust
// embed.rs — atomic ORT download
fn download_ort_runtime(target: &Path) -> Result<()> {
    let dir = target.parent().unwrap();
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    // ... download + extract into tmp ...
    let size = tmp.as_file().metadata()?.len();
    anyhow::ensure!(size > 1_000_000, "ORT download too small ({size} bytes), likely corrupt");
    tmp.as_file().sync_all()?;
    tmp.persist(target)?;
    Ok(())
}

// update.rs — Windows rollback
fn replace_self_windows(current: &Path, new_binary: &[u8]) -> Result<()> {
    let backup = current.with_extension("exe.old");
    fs::rename(current, &backup)?;
    if let Err(e) = fs::write(current, new_binary) {
        let _ = fs::rename(&backup, current); // rollback
        return Err(e).context("write failed, rolled back to previous version");
    }
    Ok(()) // .old cleaned up on next launch
}
```

- [ ] embed.rs: download to NamedTempFile, persist atomically
- [ ] embed.rs: validate minimum size (>1MB) before persist
- [ ] embed.rs: on startup, delete corrupt cache (size < 1MB)
- [ ] embed.rs: skip zero-size tar entries during extraction
- [ ] update.rs (Windows): rollback .old if write fails
- [ ] update.rs: deferred .old cleanup on next launch
- [ ] update.rs: exact suffix match for asset platform detection
- [ ] Pin SHA-256 for ORT archive (can be follow-up if hashes aren't published)

### P5 — Delete helpers lack transactions → FTS drift (`src/store.rs`)

**Problem:** FTS + chunks deletes run as two bare statements; crash between them
desyncs BM25 search. Same for `insert_chunk` when called from `cmd_add`.

**Code review confirmed:** All four delete helpers (`delete_wing`, `delete_wing_older_than`,
`delete_chunks_by_source`, `delete_chunks_by_source_prefix`) have the same pattern:
FTS delete then chunks delete, no transaction wrapper. `insert_chunk` similarly does
chunks INSERT then FTS INSERT without a wrapper.

**Research finding:** rusqlite's `Transaction` type provides RAII rollback-on-drop.
`conn.transaction()` takes `&mut self` (compile-time safety); `unchecked_transaction()`
takes `&self` (works behind Arc). Both produce `BEGIN IMMEDIATE` when configured via
`set_transaction_behavior`. For store.rs helpers that take `&Connection`, use
`unchecked_transaction`.

**Implementation:**

Wrap each delete helper internally:
```rust
pub fn delete_wing(conn: &Connection, wing: &str) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM fts_chunks WHERE rowid IN (...)", [wing])?;
    let deleted = tx.execute("DELETE FROM chunks WHERE wing = ?1", [wing])?;
    tx.commit()?;
    Ok(deleted)
}
```

For `insert_chunk`, add a self-wrapping variant for `cmd_add`:
```rust
pub fn insert_chunk_atomic(conn: &Connection, ...) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let rowid = insert_chunk_inner(&tx, ...)?;
    tx.commit()?;
    Ok(rowid)
}
```

Batch callers (ingest, import) continue to use `insert_chunk` inside their own
transaction — no double-wrapping.

- [ ] Wrap `delete_wing` in `unchecked_transaction`
- [ ] Wrap `delete_wing_older_than` in `unchecked_transaction`
- [ ] Wrap `delete_chunks_by_source` in `unchecked_transaction`
- [ ] Wrap `delete_chunks_by_source_prefix` in `unchecked_transaction`
- [ ] Add `insert_chunk_atomic` for single-item callers (`cmd_add`)
- [ ] Verify existing batch callers don't nest transactions (rusqlite savepoints)

## Explicitly deferred (recorded, not in scope here)

- Wing-normalization unification (3 divergent schemes), `parse_duration` panic +
  negative durations + missing `forget` confirmation — batch as follow-up consistency ticket
- sqlite-vec / norm-cache for brute-force vector scaling (~80MB materialized per process)
  — revisit at 100K+ chunks
- scan.rs `max_depth(1)` v3 exclusion — latent only (verified: 56 prod session dirs,
  0 nested today); fix when kiro ships nested layouts
- Ticket hygiene: delete empty stubs `40/41/42-*.md`; sync AGENTS.md (missing modules
  guard/logging/telemetry/update, commands sync/update/telemetry); uncheck-vs-done drift on #36
- Replace `unsafe static mut ORT_INIT_ERROR` with `OnceLock` (embed.rs) — modernization, not a bug
- Hardcoded ORT URLs → computed from constants (embed.rs M1) — quality-of-life
- `forget` command guard (low-risk logical race with concurrent ingest)
- Lock file health reporting in `recall health` output

## Acceptance criteria

- [ ] P1–P5 implemented with tests passing (`cargo test`)
- [ ] `cargo clippy` clean, `cargo fmt` applied
- [ ] No behavior change to documented CLI output contracts (`cli_contract.rs`, snapshots)
- [ ] Deferred items above either have their own tickets or explicit notes in this one
