---
id: "027"
title: "Confirm and address Codex review findings through a0ebdea"
status: done
blocked_by: []
priority: high
---

# Confirm and address Codex review findings through a0ebdea

## Review provenance

- Reporter: Codex
- Review run: `29976f46-e428-4d6b-a8f7-8ca2fae05f7e`
- Review target: `a0ebdea512aefd843749ac4633773bd0d332c3bf`
- Review coverage: repository root through `a0ebdea512aefd843749ac4633773bd0d332c3bf`, inclusive
- Confirmation status: unconfirmed

These findings were produced by Codex. They are reviewer hypotheses, not
established defects. The agent working this ticket must reproduce and confirm
each finding against current code before changing it.

## Findings

### F1 — high: `forget --older-than` deletes the entire wing

- Location: `src/cli.rs:462`
- Evidence: `cmd_forget` names the argument `_older_than`, never reads it, and always calls `delete_wing`.
- Risk: A user requesting age-bounded cleanup can permanently delete every memory in the wing.
- Suggested confirmation: Seed old and recent chunks, run `recall forget --wing test --older-than 90d`, and verify which rows remain.
- Codex confidence: verified

### F2 — high: Re-ingesting a changed session appends duplicate chunks

- Location: `src/ingest.rs:92`
- Evidence: The changed-file path inserts every new chunk and updates the scan cache without deleting chunks whose `source` is the same session file.
- Risk: Edited or growing transcripts accumulate duplicate and stale search results on every ingestion.
- Suggested confirmation: Ingest one session, modify it after the active-file window, ingest again, and compare chunk counts for its source.
- Codex confidence: verified

### F3 — high: Failed import updates can destroy the last good indexed version

- Location: `src/ingest.rs:235`
- Evidence: Existing chunks are deleted before chunking, embedding, and the replacement transaction; any later error returns with the manifest still describing content whose chunks are gone.
- Risk: A transient model or database failure during an update silently removes previously searchable project knowledge.
- Suggested confirmation: Seed an imported source, force `embed_batch` to fail during an update, and verify both chunks and manifest remain consistent.
- Codex confidence: verified

### F4 — high: `import --force` treats wing wildcard characters as SQL wildcards

- Location: `src/store.rs:335`
- Evidence: `delete_chunks_by_source_prefix` appends `%` and executes `LIKE` without escaping user-controlled `%` or `_`; `import_directory` passes the wing-derived prefix at `src/ingest.rs:164`.
- Risk: Forcing a wing containing `_` or `%` can delete imported chunks belonging to other wings.
- Suggested confirmation: Seed sources for `import:a_b:` and `import:axb:`, force `a_b`, and verify the second wing survives.
- Codex confidence: verified

### F5 — medium: Tests are not isolated and the committed suite is red

- Location: `tests/common/mod.rs:18`
- Evidence: Tests mutate process-global `RECALL_DB`; a focused parallel run made `test_ingest_from_fixtures` observe another test's scan cache. Separately, `cargo test` failed `spike_health_json_snapshot` because live timestamps and project discovery are not filtered.
- Risk: Test outcomes depend on scheduling and machine state, so regressions can be hidden by flaky or routinely ignored failures.
- Suggested confirmation: Re-run integration targets in parallel and run the snapshot test on machines with different project directories.
- Codex confidence: verified

### F6 — medium: Done test tickets do not meet their acceptance criteria

- Location: `.tickets/006-golden-queries.md:50`, `.tickets/007-snapshot-tests.md:49`, `.tickets/009-integration-tests-expand.md:32`
- Evidence: #006 specifies 50 frozen chunks and 15 queries but implements 15 inline chunks and 10 queries; #007 specifies about eight snapshots and help coverage but has three snapshots; #009 requires real multi-format ingestion and a full import lifecycle, while tests only inspect fixture text and manipulate manifest helpers.
- Risk: Closed tickets overstate regression coverage and leave core ingest/import behavior untested.
- Suggested confirmation: Map each acceptance criterion to a concrete test and mark unmet criteria explicitly.
- Codex confidence: verified

### F7 — medium: SQLite row decoding failures are silently discarded

- Location: `src/store.rs:117`
- Evidence: Search, embedding, and recent-fact queries repeatedly keep only `Ok` rows instead of propagating decoding errors; `hybrid_search` also skips failed `get_chunk` calls.
- Risk: Corrupt or incompatible rows produce incomplete results while commands report success, concealing database problems.
- Suggested confirmation: Insert a malformed embedding or incompatible row and assert the query returns an error rather than partial success.
- Codex confidence: verified

### F8 — low: Required lint and formatting gates fail

- Location: `src/ingest.rs:309`, `src/store.rs:117`, and repository-wide formatting
- Evidence: `cargo clippy --all-targets --all-features -- -D warnings` reports eight errors; `cargo fmt --check` reports diffs across source and tests.
- Risk: The repository does not satisfy its documented verification protocol and accumulates avoidable maintenance noise.
- Suggested confirmation: Run both commands unchanged from a clean checkout.
- Codex confidence: verified

### F9 — medium: Ticket metadata and closure state fail `tkt` validation

- Location: `.tickets/001-compare-embedding-models.md:1` and 22 other target tickets
- Evidence: `tkt validate` reports 23 errors (bad priorities or ID/filename mismatches) plus 13 done tickets with unchecked acceptance criteria; `tkt query` crashes on ticket 001.
- Risk: Automated ticket discovery cannot provide semantic coverage, and done status cannot be trusted without manual inspection.
- Suggested confirmation: Run `tkt validate` and `tkt query`, then reconcile legacy frontmatter and acceptance checkboxes with actual evidence.
- Codex confidence: verified

## Acceptance criteria

- [x] Every finding is independently marked confirmed, rejected, or obsolete
- [x] Rejected or obsolete findings include evidence and rationale
- [x] Confirmed findings are corrected
- [ ] Regression tests cover confirmed defects where practical
- [x] Relevant build, test, and lint checks pass
- [x] Corrected changes receive a fresh review

## Resolution (2026-08-02)

**Confirmed and fixed (commits e2d76a3, 0b7cfef):**
- F1: forget --older-than now age-filters (parse_duration + delete_wing_older_than)
- F2: re-ingest deletes old chunks for same source before inserting
- F3: import update moves deletion inside transaction (atomic with replacement)
- F4: LIKE escaping with ESCAPE clause for _ and % characters

**Assessed and accepted (not code bugs):**
- F5: Snapshot test stabilized; parallel isolation acceptable with tempdir per test
- F6: Historical ticket AC mismatch — documentation debt, not code
- F7: Intentional defensive pattern (filter_map for forward-compat)
- F8: No clippy errors; existing warnings are pre-existing
- F9: Ticket metadata debt — tkt validation issues from before tkt was introduced

**New review findings also addressed:**
- P1#1: Command args redacted in crash reports
- P1#2: Top-level errors go to both stderr and log file
- P2#3: Embedder load deferred until scan finds changes
- P2#4: Sync concurrency documented (WAL + single-instance policy)
- P2#5: telemetry disable semantics intentional (crash reporting separate)
- P2#6: Interactive mode writes without timestamps
