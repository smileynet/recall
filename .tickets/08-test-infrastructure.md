---
id: "08"
title: "Test infrastructure: shared helpers, fixtures, OnceLock embedder"
status: done
priority: high
blocked_by: []
estimate: 30min
---

# Test Infrastructure

## What to build

Shared test utilities in `tests/common/mod.rs` and additional fixture files
that all test files can reuse.

### tests/common/mod.rs

```rust
use std::sync::OnceLock;
use recall::embed::Embedder;
use recall::store;
use tempfile::TempDir;

static EMBEDDER: OnceLock<Embedder> = OnceLock::new();

/// Shared embedder — loads model once per test binary process.
pub fn shared_embedder() -> &'static Embedder {
    EMBEDDER.get_or_init(|| Embedder::new().unwrap())
}

/// Create a fresh test DB in a temp directory.
pub fn test_db() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.sqlite3");
    std::env::set_var("RECALL_DB", &db_path);
    let conn = store::open_db().unwrap();
    (dir, conn)
}

/// Create a seeded DB with a few chunks for search/status/health tests.
pub fn seeded_db() -> (TempDir, rusqlite::Connection) {
    let (dir, conn) = test_db();
    let embedder = shared_embedder();
    let facts = [
        ("Authentication uses JWT with 15-minute expiry", "web_app", "decisions"),
        ("The scan cache uses mtime plus file size", "recall", "architecture"),
        ("We chose Rust for the rebuild", "recall", "decisions"),
    ];
    for (content, wing, room) in &facts {
        let emb = embedder.embed_one(content).unwrap();
        store::insert_chunk(&conn, content, wing, room, "fact", "test", &emb).unwrap();
    }
    (dir, conn)
}
```

### Additional fixtures

- `tests/fixtures/session-v3/sess_test/messages.jsonl` — v3 format sample
- `tests/fixtures/session-v3/sess_test/session.json` — with workspacePaths
- `tests/fixtures/session-codex.jsonl` — codex format sample
- `tests/fixtures/memory/CONTEXT.md` — sample .memory file for import tests

### Dev-dependencies to add

```toml
[dev-dependencies]
tempfile = "03"
assert_cmd = "2.0"
predicates = "3.1"
insta = { version = "01", features = ["filters"] }
insta-cmd = "0.6"
```

## Acceptance criteria

- [ ] `shared_embedder()` loads model only once across all tests in a binary
- [ ] `test_db()` provides isolated DB per test (no cross-contamination)
- [ ] `seeded_db()` returns a searchable DB without each test needing setup
- [ ] Fixture files cover all three JSONL formats (v2, v3, codex)
- [ ] Refactor existing integration_test.rs to use common helpers

## Notes

This is foundational — other test tickets depend on these helpers existing.
Should be done early (before or alongside ticket #003).
