---
id: "042"
title: "Spike: temporal decay in recall search scoring (recency boost)"
status: open
blocked_by: []
env: either
priority: normal
---

# Spike: Temporal Decay in Search Scoring

## Hypothesis

Adding a time-decay factor to RRF scoring will surface recent decisions over old ones when
both are relevant. "What did we decide about auth?" should prefer last week's decision over
last month's exploration — currently they score equally if keywords/embeddings match.

## Baseline

- Current RRF: `score = 1/(k + bm25_rank) + 1/(k + vector_rank)`. No time component.
- Problem: stale decisions rank equally with fresh ones. Agents sometimes cite outdated
  approaches that were superseded.

## Spike design

Add a decay multiplier to the final RRF score:

```
decay = exp(-lambda * age_days)
final_score = rrf_score * (alpha + (1 - alpha) * decay)
```

Where:
- `lambda` = decay rate (e.g., 0.01 = half-life ~70 days)
- `alpha` = floor (e.g., 0.3 = old results never drop below 30% of their score)
- `age_days` = days since chunk was ingested

**Parameters (tunable):**
- `--recency-boost 0.7` (default: 1.0 = no boost, 0.0 = pure recency)
- Or configurable in recall config

**Measurement:** Run golden queries where a newer decision supersedes an older one. Verify
the newer one ranks higher with decay vs without.

## Validation criteria

- [ ] Decay formula implemented in search scoring
- [ ] Recent chunks rank higher than old chunks of equal semantic relevance
- [ ] Very old chunks still findable (floor prevents complete disappearance)
- [ ] Golden queries with known superseded decisions show correct ordering
- [ ] Search latency impact <50ms (decay is a multiplication, should be trivial)

## Reject if

- Decay makes important old decisions (ADRs, architectural choices) unfindable
- The tuning is too sensitive (small parameter changes flip rankings dramatically)
- No measurable improvement on golden queries (timestamps don't correlate with relevance)

## References

- Research: `.scratch/research/overlap-memory.md`
- recall search: `~/code/recall/src/search.rs`
