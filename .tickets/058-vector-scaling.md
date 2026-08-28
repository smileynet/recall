---
id: "058"
title: "Vector search scaling: sqlite-vec or norm-cache at 100K+ chunks"
status: backlog
blocked_by: []
validation_criteria:
  - "search stays under 2s at 100K chunks"
---

# Vector search scaling: sqlite-vec or norm-cache at 100K+ chunks

## Context

From the 2026-08-23 review (deferred in 048). **Backlog — revisit at 100K+
chunks.** Current corpus is ~27K chunks and search is ~1.5s warm, within target.

Vector search currently does a brute-force cosine scan: it materializes all
embeddings per query (~80MB resident for the current corpus at 768-dim × 4 bytes
× chunk count). This grows linearly and will blow past the ~1.5s search budget as
the corpus scales.

## Trigger to activate

- Corpus exceeds ~100K chunks, OR
- Warm search latency exceeds ~2s (measure via existing benches)

## Options to evaluate (when triggered)

- [ ] **sqlite-vec** extension — ANN index inside SQLite; keeps the single-file,
      no-server model. Check load-dynamic compatibility with the ort/rusqlite setup.
- [ ] **Norm cache** — precompute and store L2 norms so cosine reduces to a dot
      product; cheaper than full recompute, smaller change, still linear.
- [ ] **Quantization** — smaller embedding footprint (trade recall quality).

Prefer the option that preserves the no-server, single-binary constraint
(AGENTS.md). Benchmark against `bench_quality.rs` to guard recall quality.

## Acceptance criteria

- [ ] Warm search stays under 2s at 100K chunks
- [ ] Search quality (golden queries) does not regress
- [ ] Single-binary / no-server constraint preserved

## Validation criteria

- Bench: search latency at 100K synthetic chunks < 2s
- `golden_queries` pass rate unchanged
