---
id: "045"
title: "Preference-aware search boosting in RRF"
status: open
blocked_by: ["044"]
priority: medium
---

# Preference-aware search boosting in RRF

## What to build

When a chunk has `type = 'preference'`, apply a boost factor during RRF fusion:

- Configurable boost multiplier (e.g. 1.5×) on the RRF score
- Recency weight: preferences with recent `last_used_at` get stronger boost
- Update `last_used_at` when a preference appears in returned search results

## Acceptance criteria

- [ ] Preferences rank higher than equivalent-score facts in search results
- [ ] `last_used_at` updated on retrieval
- [ ] Boost factor is a constant (easy to tune later via config)
- [ ] Existing non-preference search behavior unchanged (no regression on golden queries)
