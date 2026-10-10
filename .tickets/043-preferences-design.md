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

- **Schema**: whether preference-only state belongs in an extension relation
  keyed by `chunk_id` or another enforceable representation; exact types,
  defaults, constraints, and migration strategy
- **Entry paths**: explicit (`recall add --type preference`), imported (`.memory/`), and why extraction belongs in a spellbook skill not recall
- **Search integration**: how preferences participate in RRF scoring (boost factor, recency weight)
- **Decay**: staleness threshold, demotion mechanics, health reporting
- **Rejected alternatives**: full ML ranking, server-side tracking, auto-extraction in recall

Reference CodeRabbit learnings and Greptile memory patterns from the original research.

The 2026-10-10 data-model review found that generic nullable preference columns
on every chunk would let arbitrary facts carry preference lifecycle state and
would leave `scope` as unchecked free text. The design must enforce the model,
not merely describe it.

## Acceptance criteria

- [ ] Design doc at `.memory/specs/learnable-preferences.md`
- [ ] Schema section selects an enforceable preference representation and gives
      exact definitions, constraints, and migration strategy
- [ ] Scope is constrained at the storage/typed-boundary level to `wing` or
      `global`, with an explicit definition of how a global preference relates
      to a chunk's mandatory wing
- [ ] Lifecycle states define valid combinations of usage count and last-used
      timestamp; arbitrary facts cannot accidentally acquire preference-only
      state
- [ ] Explicit decision: extraction belongs in skill layer, not recall
- [ ] At least 2 rejected alternatives documented with rationale
