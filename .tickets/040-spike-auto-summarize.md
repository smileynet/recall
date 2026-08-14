---
id: "040"
title: "Spike: automatic session summarization at ingestion (mastra observational memory pattern)"
status: open
blocked_by: []
env: personal
priority: normal
---

# Spike: Automatic Session Summarization at Ingestion

## Hypothesis

Adding an optional LLM-based summary at ingest time — stored as a boosted "summary" chunk
per session — will significantly improve `recall search` relevance for "what happened?"
queries without changing recall's offline-first architecture.

Inspired by mastra's Observational Memory pattern: a secondary process watches conversations
and extracts structured knowledge. Our adaptation: do it at ingestion time (batch, offline)
instead of real-time (streaming, online).

## Baseline

- Current: recall ingests raw chunks from session transcripts. Search finds relevant chunks
  via hybrid BM25+vector. No summarization.
- Problem: "what did we decide about X?" requires scanning multiple chunks to piece together
  the narrative. The answer is spread across 5-10 chunks.

## Spike design

1. At ingest time, after chunking a session, call a cheap model to generate a 2-3 sentence
   summary: "Key decisions: ..., Key actions: ..., Outcome: ..."
2. Store the summary as a special chunk (`type: summary`, boosted in RRF scoring)
3. Search: when results include a summary chunk, it provides instant orientation

**Implementation options:**
- A: `recall ingest --summarize` flag (opt-in, requires model access)
- B: Post-ingest batch job: `recall summarize --wing X` (runs after normal ingest)

**Measurement:** Run 10 golden queries before/after. Count how many require reading 3+ chunks
to answer vs getting the answer from a single summary chunk.

## Validation criteria

- [ ] Summary generation produces coherent 2-3 sentence summaries per session
- [ ] Summary chunks appear in top-3 results for "what happened" queries
- [ ] Golden query relevance improves (fewer chunks needed to answer)
- [ ] Ingest time increase <30s per session (cheap model, not frontier)
- [ ] Opt-in only (no model calls without explicit flag)

## Reject if

- Summaries hallucinate decisions not in the transcript
- Latency makes ingest impractical (>60s per session)
- Cheap model quality too low (summaries are generic/useless)

## References

- Mastra observational memory: `.references/mastra/packages/memory/src/processors/observational-memory/`
- Research: `.scratch/research/overlap-memory.md`
- recall repo: `~/code/recall`
