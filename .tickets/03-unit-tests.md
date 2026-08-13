---
id: "03"
title: "Unit tests: chunking, parsing, RRF, classification"
status: done
priority: high
blocked_by: []
estimate: 1h
---

# Unit Tests: Pure Logic

## What to build

Add `#[cfg(test)] mod tests` to each module with pure logic tests (no DB, no model, no I/O).

### ingest.rs (~15 tests)

**chunk_messages:**
- Single message under limit → one chunk
- Two messages exceeding limit → splits
- User prefix `> ` applied correctly
- Chunk below MIN_CHUNK_SIZE discarded
- Empty messages list → empty result
- Message exactly at CHUNK_SIZE boundary

**parse_kiro_v3:**
- Valid v3 JSONL → correct messages
- Non-v3 format → returns None
- Consecutive assistant messages merged
- Minimum 2 messages required

**parse_kiro_v2:**
- Valid v2 with Prompt + AssistantMessage → messages
- Tool use summarized as `[tool: name] purpose`
- Non-v2 format → returns None

**classify_room:**
- Technical keywords → "technical"
- Architecture keywords → "architecture"
- No keywords → "general"
- Mixed keywords → highest score wins
- UTF-8 content doesn't panic (already fixed, prevent regression)

**chunk_markdown:**
- Splits at `## ` headings
- Oversized sections split at paragraphs
- Chunks below MIN_CHUNK_SIZE dropped
- Frontmatter stays with first chunk

### search.rs (~5 tests)

- cosine_similarity: identical vectors → 1.0
- cosine_similarity: orthogonal → 0.0
- cosine_similarity: zero vector → 0.0
- RRF fusion: known rank lists → deterministic merge
- RRF fusion: empty second list → first list scores only

### migrate.rs (~5 tests)

- parse_iso8601_to_epoch: standard format with timezone
- parse_iso8601_to_epoch: Z suffix
- parse_iso8601_to_epoch: no timezone → treats as UTC
- parse_iso8601_to_epoch: short string → returns None
- parse_tz_offset: +0700, -07:00, Z, empty

### embed.rs (~4 tests)

- Model::from_name("bge-base") → Some(BgeBase)
- Model::from_name("small") → Some(BgeSmall)
- Model::from_name("invalid") → None
- Model::from_name case insensitive

## Acceptance criteria

- [ ] `cargo test --lib` passes in <1s (no model loading)
- [ ] All pure functions have at least boundary + happy path coverage
- [ ] No test depends on external state (DB, files, model)

## Notes

These functions need to be made `pub(crate)` or have test wrappers if they're currently private.
Some may need refactoring to be testable (extract pure logic from I/O).
