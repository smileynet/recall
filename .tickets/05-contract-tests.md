---
id: "005"
title: "Contract tests: health --json and prime output format"
status: done
priority: high
blocked_by: ["002"]
estimate: 45min
---

# Contract Tests: Machine-Readable Output

## What to build

Tests in `tests/cli_contract.rs` that validate the JSON schema and structure of
commands consumed by downstream tools (kiro-cli steering, scripts).

### health --json contract (~3 tests)

```rust
#[test]
fn health_json_has_required_fields() {
    // Seed a DB, run health --json, parse as serde_json::Value
    // Assert all required fields exist with correct types:
    // total_chunks: number
    // import_chunks: number
    // session_chunks: number
    // agent_chunks: number
    // wing_count: number
    // wings: object (string → number)
    // import_wings: array of strings
    // duplicates: array
    // last_ingest_ts: number | null
    // discoverable_projects: number
    // covered_projects: number
    // missing_projects: array of strings
    // stale_wings: array
}

#[test]
fn health_json_is_valid_json() {
    // Even with empty DB, output is parseable JSON
}

#[test]
fn health_json_wing_counts_sum_to_total() {
    // Internal consistency: sum of wings values == total_chunks
}
```

### prime contract (~3 tests)

```rust
#[test]
fn prime_starts_with_header() {
    // Output begins with "## Recall - Cross-Session Memory"
}

#[test]
fn prime_contains_usage_instructions() {
    // Contains "recall search" and "recall add" usage examples
}

#[test]
fn prime_with_wing_shows_scoped_header() {
    // --wing test → header mentions the wing name
}
```

### Setup

These tests need a seeded DB with known data. Use a shared fixture setup
that adds a few chunks (requires model loading — use OnceLock pattern).

## Acceptance criteria

- [ ] Tests fail if any required field is removed from health --json
- [ ] Tests fail if prime output structure changes
- [ ] Tests document the contract (serve as living documentation)
