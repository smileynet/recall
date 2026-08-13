---
id: "36"
title: "Explore learnable preferences — feedback-driven memory with scoping and decay"
status: open
blocked_by: []
---

# Explore Learnable Preferences

## Context

Research into CodeRabbit and Greptile (2026-08-06) surfaced a pattern both tools use:
review preferences accumulate through natural interaction (thumbs up/down, natural-language
corrections) and are stored with metadata enabling scoping and decay. CodeRabbit's
implementation tracks usage-count + last-used-date per learning, making stale preferences
surface naturally without manual cleanup.

Currently recall stores facts and decisions with equal weight. There is no mechanism to:
- Mark a memory as a reusable **preference** (vs one-time decision)
- Track whether a preference is still actively useful (usage/freshness)
- Scope preferences differently than facts (org-wide vs project-local)
- Suppress or reinforce memories based on feedback

This ticket explores whether recall should grow a "learnings" layer — structured preferences
that accumulate through use, decay when stale, and participate in search with boosted
relevance when contextually matched.

## Research Questions (spikes)

### S1: What metadata distinguishes a preference from a fact?

A decision ("we chose SQLite") is a fact recorded once. A preference ("always use explicit
error types, not anyhow") is a recurring pattern that should influence future work. What
schema additions would distinguish them? Candidates:
- `type: preference` (already have `--type` flag)
- `usage_count` + `last_used_at` columns
- `scope: wing | global` (project-local vs everywhere)
- `confidence: reinforced | tentative` (feedback-adjusted)

### S2: How would preferences enter the system?

Three paths to investigate:
1. **Explicit** — `recall add "..." --type preference` (agent writes after correction)
2. **Extracted** — agent notices a correction mid-session and proposes a preference
3. **Imported** — `.kiro/steering/` rules become searchable preferences automatically

CodeRabbit uses path (2) via PR comment interaction. For recall, path (1) is simplest; path
(2) would need a crew-research skill to trigger extraction.

### S3: How should preferences participate in search?

Options:
- Boost preferences in RRF scoring when they match context
- Return preferences in a separate section of `recall prime` output
- Only surface preferences when the query matches their scope/wing

### S4: What does decay look like for a local tool?

CodeRabbit tracks usage server-side. For recall:
- `last_used_at` updated when a preference appears in search results the agent acts on?
- Or: `last_cited_at` updated only when the agent explicitly references it?
- Decay threshold: preferences unused for N days get demoted (lower boost) or flagged

## What this is NOT

- Not a replacement for `.kiro/steering/` (those are always-loaded rules, not search results)
- Not a rating system for all memories (facts/decisions stay as-is)
- Not requiring network access or external services

## Acceptance Criteria

- [ ] Design doc in `.memory/specs/` covering schema, entry paths, search integration, and
      decay — with explicit rejected alternatives
- [ ] Spike implementation of S1 (schema) demonstrating backward-compatible migration
- [ ] Decision on whether S2 (extraction) belongs in recall or in a crew-research skill
- [ ] Search quality comparison: does boosting preferences improve `recall prime` relevance
      on golden queries?

## References

- CodeRabbit learnings system: https://docs.coderabbit.ai/knowledge-base/learnings
- Greptile memory (thumbs up/down): https://www.greptile.com/docs/code-review/key-features
- crew-research guidance-sync skill (the extraction-side analog)
- crew-research research: `.scratch/research/coderabbit.md`, `.scratch/research/greptile.md`
