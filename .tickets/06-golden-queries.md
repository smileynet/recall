---
id: "006"
title: "Golden query regression tests for search quality"
status: done
blocked_by: ["003"]
estimate: 1h
---

# Golden Query Regression Tests

## What to build

A frozen test corpus + query set that prevents search quality regressions.

### 1. Create golden corpus (`tests/fixtures/golden_corpus.json`)

50 chunks covering:
- Technical decisions (auth, model choice, architecture)
- Code discussion (refactoring, error handling, testing)
- Project management (planning, tickets, milestones)
- Domain-specific (games, shaders, UI)
- Vague/conversational (paraphrased recollections)

Each chunk: `{"id": "golden-001", "content": "...", "wing": "...", "room": "...", "type": "..."}`

### 2. Create golden queries (`tests/fixtures/golden_queries.json`)

15 queries with expected behavior:

```json
{
  "query": "what did we decide about authentication",
  "must_contain_keywords_in_top5": ["auth", "jwt", "token"],
  "notes": "Should find the auth decision chunk"
}
```

### 3. Implement test in `tests/golden_queries.rs`

- Load golden corpus into a temp DB (embed all 50 chunks)
- For each query: run hybrid_search, check that at least one top-5 result
  contains at least one expected keyword
- NOT testing exact ranking — just "relevant content found"

### 4. Test helper: `setup_golden_corpus() -> (TempDir, Connection)`

Uses shared embedder (OnceLock), embeds all 50 chunks once, returns ready DB.

## Acceptance criteria

- [x] 50-chunk corpus covers diverse content types
- [x] 15 queries all pass (relevant results in top-5)
- [x] Test runs in < 10s (embedding 50 chunks + 15 searches)
- [x] Corpus is frozen (changes require deliberate PR review)
- [x] Tests prevent accidental search degradation (e.g., if chunking changes break content)

## Notes

This is the last line of defense against "search got worse." If a change breaks
golden queries, the developer must either fix the regression or deliberately update
the expectations with justification.
