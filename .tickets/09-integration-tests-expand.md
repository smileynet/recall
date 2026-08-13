---
id: "009"
title: "Integration tests: expand coverage for migrate, import, multi-format ingest"
status: done
blocked_by: ["008"]
estimate: 1h
---

# Integration Tests: Expanded Coverage

## What to build

Add integration tests for features not yet covered by the existing 3 tests.

### New tests (~7)

| Test | What it verifies |
|------|-----------------|
| `test_wing_scoped_search` | Results with --wing filter only return that wing's chunks |
| `test_ingest_v3_format` | v3 JSONL (payload.type format) parsed correctly |
| `test_ingest_codex_format` | Codex JSONL (event_msg format) parsed correctly |
| `test_import_hash_gate_skip` | Second import of unchanged files reports 0 new chunks |
| `test_import_detects_changes` | Modified file is re-imported (old chunks deleted, new stored) |
| `test_import_detects_deletions` | Deleted source file → chunks removed from DB |
| `test_room_classification` | Ingested chunks get correct room based on content keywords |

### Test for migrate (if feasible without real Python DB)

Create a small SQLite fixture with the Python schema (`drawers` table, `sources` table, `meta` table) containing 5-10 rows. Verify migration copies all rows correctly.

## Acceptance criteria

- [x] All three JSONL formats have test coverage
- [x] Import hash-gate has full lifecycle test (add → skip → modify → update → delete)
- [x] Wing scoping verified at search layer
- [x] Tests use shared_embedder() from common module (fast)
- [x] Total integration test time < 10s

## Notes

The v3 and codex format fixtures need to be realistic enough to pass the format
detection heuristics. Refer to normalize.py patterns.
