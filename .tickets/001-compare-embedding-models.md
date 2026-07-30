---
id: 1
title: "Compare bge-base vs bge-small for performance and accuracy"
status: open
priority: normal
blocked_by: []
---

# Compare bge-base-en-v1.5 vs bge-small-en-v1.5

## Context

We currently default to bge-base-en-v1.5 (768-dim) to match the Python recall corpus. bge-small-en-v1.5 (384-dim) is an alternative that trades quality for speed and storage.

Both models are already supported in `embed.rs` via the `Model` enum.

## What to measure

### Performance
- Cold start time (model load from disk)
- Single embedding latency
- Batch embedding throughput (64 texts)
- Search latency at 168K embeddings (brute-force cosine)
- RAM usage during search (768×4×168K = ~500MB vs 384×4×168K = ~250MB)
- DB size difference for the full corpus

### Accuracy
- Run a set of 20-30 known-good queries against the real corpus
- Compare top-5 result rankings between models
- Measure NDCG@5 or a simpler rank-correlation metric
- Identify queries where bge-small fails to surface the correct result

## Acceptance criteria

- [ ] Benchmark table comparing both models on all performance metrics
- [ ] Accuracy comparison on real queries with documented methodology
- [ ] Recommendation: which model to default to, with rationale
- [ ] If bge-small is chosen: migration plan for existing bge-base embeddings
- [ ] If bge-base is kept: document whether sqlite-vec or int8 quantization is needed for scale

## Notes

- The Python recall used `bge-base-en-v1.5-int8` (quantized) — int8 quantization could be a middle ground
- fastembed-rs spike S1 measured bge-small at 3.3ms/embed — need equivalent measurement for bge-base
- sqlite-vec (deferred optimization) would eliminate brute-force RAM loading regardless of model choice
