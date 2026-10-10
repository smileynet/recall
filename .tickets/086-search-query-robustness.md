---
id: "086"
title: "Make natural-language search queries safe for FTS5"
status: open
blocked_by: []
env: either
priority: medium
---

# Make natural-language search queries safe for FTS5

## Context

`bm25_search` passes the user-provided query directly to FTS5 `MATCH`. Natural
language prompts containing quotes, punctuation, operators, or malformed FTS
syntax can make the entire hybrid-search command error before vector retrieval
has a chance to help. That is a poor default for an assistant-facing semantic
memory CLI.

## What to build

1. Decide the public query contract: the default `recall search` path should
   safely interpret ordinary natural language; advanced FTS syntax, if retained,
   needs an explicit opt-in or clearly documented escaping rule.
2. Normalize/escape or tokenize the BM25 query without altering the original
   text passed to the embedding model.
3. Provide a graceful vector-only/fallback behavior if FTS parsing still fails;
   never turn a recoverable lexical parse issue into a command crash.
4. Preserve wing filtering, deterministic result limits, and documented search
   semantics.

## Acceptance criteria

- [ ] Queries containing quotes, apostrophes, punctuation, and FTS operator
      words return normally rather than producing SQLite syntax errors.
- [ ] Default natural-language search remains useful for multi-word queries.
- [ ] Explicit advanced FTS behavior is either supported and tested or excluded
      from the public contract.
- [ ] Vector retrieval remains available if BM25 has no valid terms.

## Validation criteria

- Unit/integration tests cover malformed FTS syntax and common assistant query
  punctuation, both globally and wing-scoped.
- Golden search quality does not regress for the existing corpus.
