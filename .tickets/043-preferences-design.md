---
id: "043"
title: "Design doc — learnable preferences architecture"
status: open
blocked_by: []
priority: medium
---

# Design doc — learnable preferences architecture

## What to build

Write `.memory/specs/learnable-preferences.md` covering:

- **Schema**: columns to add (`usage_count`, `last_used_at`, `scope`), types, defaults
- **Entry paths**: explicit (`recall add --type preference`), imported (`.memory/`), and why extraction belongs in a spellbook skill not recall
- **Search integration**: how preferences participate in RRF scoring (boost factor, recency weight)
- **Decay**: staleness threshold, demotion mechanics, health reporting
- **Rejected alternatives**: full ML ranking, server-side tracking, auto-extraction in recall

Reference CodeRabbit learnings and Greptile memory patterns from the original research.

## Acceptance criteria

- [ ] Design doc at `.memory/specs/learnable-preferences.md`
- [ ] Schema section with exact column definitions and migration strategy
- [ ] Explicit decision: extraction belongs in skill layer, not recall
- [ ] At least 2 rejected alternatives documented with rationale
