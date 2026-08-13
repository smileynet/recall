---
id: "01"
title: "Compare bge-base vs bge-small for performance and accuracy"
status: done
blocked_by: []
---

# Compare bge-base-en-v1.5 vs bge-small-en-v1.5

## Result: Keep bge-base. The speed difference doesn't justify the quality tradeoff.

## Benchmark (194K chunks, 20 real queries, release build)

### Performance

| Metric | bge-base (768-dim) | bge-small (384-dim) | Ratio |
|--------|:--:|:--:|:--:|
| Cold start | 474ms | 214ms | 2.2× faster |
| Single embed | 18.9ms | 15.5ms | 1.2× faster |
| Batch embed (64) | 474ms (135/sec) | 130ms (492/sec) | 3.6× faster |
| Search latency | 1.0s/query | 886ms/query | 1.1× faster |
| Embedding storage | 570MB | 285MB | 2× smaller |

### Quality (cross-model, not a fair comparison — see caveat)

| Metric | Result |
|--------|--------|
| Top-5 overlap | 23% (23/100) |
| Exact top-5 match | 0/20 queries |
| Top-1 agreement | 4/20 queries |
| Divergent queries (< 60% overlap) | 16/20 |

### Caveat

This comparison used bge-small **query** embeddings against bge-base **stored** embeddings.
The 23% overlap does NOT mean bge-small is worse — it means the models produce incompatible
embedding spaces. A fair quality comparison would require re-embedding the entire corpus with
bge-small and comparing human-judged relevance. We decided this isn't worth the effort given
the speed difference is modest.

## Recommendation

**Keep bge-base-en-v1.5 as default.** Rationale:

1. **Speed difference is small where it matters.** Single embed (used in search) is only 1.2× faster with bge-small. Batch embedding (used in ingest) is 3.6× faster but ingest is background work.
2. **Search latency is dominated by loading embeddings from disk**, not the query embed itself. Both models produce ~1s/query at 194K chunks. sqlite-vec would fix this regardless of model choice.
3. **Compatibility with existing corpus.** The Python recall already has 180K chunks embedded with bge-base. Switching to bge-small would require re-embedding everything (13+ minutes) for a modest speed gain.
4. **Storage is manageable.** 570MB for 194K chunks is fine for a single-machine tool. If storage becomes a constraint, int8 quantization (4× reduction) is a better lever than switching models.

## Future optimization path (priority order)

1. **sqlite-vec** — eliminate brute-force cosine, reduce search from 1s to <50ms regardless of model
2. **Int8 quantization** — 4× embedding storage reduction while preserving bge-base quality
3. **Model re-evaluation** — only revisit if a clearly superior small model emerges (e.g., nomic-embed)
