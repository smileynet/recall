---
id: "07"
title: "Snapshot tests: CLI output format regression"
status: done
blocked_by: ["02"]
estimate: 45min
---

# Snapshot Tests: Output Format Regression

## What to build

Tests in `tests/cli_snapshot.rs` using `insta-cmd` to catch unintentional
changes to user-facing output format.

### Snapshots to capture (~8)

| Command | Filters needed |
|---------|---------------|
| `recall status` (with seeded DB) | Chunk counts |
| `recall health` (human, seeded DB) | Counts, timestamps, paths |
| `recall search "query"` (with results) | Scores, source paths |
| `recall search "nonexistent"` (no results) | None |
| `recall add "fact" --wing x --type decision` | None |
| `recall --help` | None |
| `recall search --help` | None |
| `recall ingest --help` | None |

### Filters

```rust
let mut settings = insta::Settings::clone_current();
settings.add_filter(r"\d+\.\d{3}", "[SCORE]");          // similarity scores
settings.add_filter(r"[A-Z]:\\[^\s]+", "[PATH]");       // Windows paths
settings.add_filter(r"/[\w/.-]+\.jsonl", "[PATH]");     // Unix paths
settings.add_filter(r"\d+ drawers", "[N] drawers");     // counts
settings.add_filter(r"\(\d+\)", "([N])");               // wing counts
```

### Workflow

When output changes intentionally:
1. Tests fail with diff
2. Developer runs `cargo insta review`
3. Reviews and accepts new snapshots
4. Commits updated snapshots

## Acceptance criteria

- [x] All happy-path outputs captured as snapshots
- [x] Volatile values filtered (tests don't break on count changes)
- [x] `cargo insta review` workflow documented in AGENTS.md
- [x] --help snapshots catch accidental flag renames/removes

## Notes

Depends on spike #002 confirming insta-cmd works for our use case.
If spike rejects insta-cmd, fall back to assert_cmd with contains/regex predicates.
