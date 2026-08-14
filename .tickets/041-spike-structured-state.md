---
id: "041"
title: "Spike: structured state per wing — recall state get/set for working memory"
status: open
blocked_by: []
env: either
priority: normal
---

# Spike: Structured State Per Wing

## Hypothesis

Adding a `recall state get/set` command — persistent structured data per wing (project) —
gives agents a fast path for continuity without searching. Active goals, user preferences,
project config, and "what I'm currently doing" persist across sessions without requiring
semantic search.

Inspired by mastra's Working Memory: a structured template updated after each turn with
slots for user info, goals, current facts, open questions.

## Baseline

- Current: agents use `recall search` for everything, including "what am I working on?"
- Problem: search is probabilistic — it might not return the most relevant active state.
  Agents re-discover context instead of reading it directly.

## Spike design

```bash
# Set structured state for a wing
recall state set --wing my-project '{"active_goal": "implement auth", "blocked_by": "API key", "preferences": {"style": "concise"}}'

# Get state for current wing
recall state get
# → {"active_goal": "implement auth", "blocked_by": "API key", ...}

# Update a specific field
recall state set --wing my-project --key active_goal "deploy to staging"
```

**Storage:** New SQLite table `wing_state (wing TEXT PRIMARY KEY, state JSON, updated_at TEXT)`.
No embeddings, no search — direct key-value per wing.

**Integration with prime:** `recall prime` includes the wing's state at the top of output
(before search results). Agents see it immediately at session start.

## Validation criteria

- [ ] `recall state get/set` works (round-trip JSON)
- [ ] State appears in `recall prime` output
- [ ] Agent can read state without searching (faster than search for known facts)
- [ ] State persists across sessions (survives process exit)
- [ ] No performance regression on existing commands

## Reject if

- Schema becomes too complex to maintain (keep it freeform JSON)
- Agents ignore the state in prime output (not useful in practice)

## References

- Mastra working memory: `.references/mastra/packages/memory/src/` (working memory template)
- Research: `.scratch/research/overlap-memory.md`
