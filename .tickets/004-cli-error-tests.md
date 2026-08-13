---
id: "004"
title: "E2E tests: CLI error handling and arg validation"
status: done
priority: high
blocked_by: ["002"]
estimate: 45min
---

# E2E Tests: Error Handling

## What to build

Tests in `tests/cli_errors.rs` exercising the compiled binary with `assert_cmd`.
Each test verifies correct exit code, helpful error message, and no panics.

### Tests (~10)

| Scenario | Expected exit | Expected output |
|----------|:---:|---|
| `recall` (no subcommand) | 2 | Help text |
| `recall add "fact"` (missing --wing) | 2 | Error mentions --wing |
| `recall search` (no query) | 2 | Error mentions query |
| `recall ingest /nonexistent/path` | 1 | "not found" or "not a directory" |
| `recall import /nonexistent --wing x` | 1 | "not a directory" |
| `recall import somefile.txt --wing x` | 1 | "not a directory" |
| `recall migrate --from /nonexistent` | 1 | "not found" |
| `recall forget` (missing --wing) | 2 | Error mentions --wing |
| `recall search "q"` against empty DB | 0 | "No results found." |
| RECALL_MODEL=invalid `recall status` | 0 | Warning about unknown model on stderr |

### Shared setup

```rust
fn recall_cmd() -> Command {
    Command::cargo_bin("recall").unwrap()
}

fn with_empty_db(cmd: &mut Command) -> &mut Command {
    let dir = tempfile::tempdir().unwrap();
    cmd.env("RECALL_DB", dir.path().join("empty.sqlite3"))
}
```

## Acceptance criteria

- [ ] All error cases produce non-panic exits with helpful messages
- [ ] No test requires the embedding model (error cases hit before model load)
- [ ] Tests run in < 3s total
