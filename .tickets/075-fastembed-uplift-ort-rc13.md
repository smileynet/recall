---
id: "075"
title: "Uplift fastembed 4.9.1 -> 7.x (ort rc.13 + api-NN + ONNX RT 1.23)"
status: open
blocked_by: ["068"]
priority: medium
---

# Uplift fastembed 4.9.1 -> 7.x (ort rc.13 + api-NN + ONNX RT 1.23)

## Context

Deferred from ticket 068 (which stabilizes the *current* rc.9 stack). This ticket
is the forward move: get recall onto a newer, maintained ONNX Runtime line.

recall cannot upgrade `ort` directly — `fastembed` exact-pins it. The version
chain (verified 2026-10-02 from crates.io + pykeio/ort source,
`.scratch/research/ort-fastembed-compat.md`):

| fastembed | ort | ort-sys | ONNX Runtime native lib |
|-----------|-----|---------|-------------------------|
| 4.9.1 (current) | =2.0.0-rc.9 | =2.0.0-rc.9 | 1.20.x (hardcoded) |
| 5.x | rc.10 | rc.10 | 1.22.0 |
| 6.x | rc.13 | rc.13 | up to 1.23.x |
| 7.1.0 (latest) | rc.13 | rc.13 | up to 1.23.x, `api-NN` selectable (min API 17) |

The payoff of moving to fastembed 7.x / ort rc.13:
- `api-NN` cargo features let recall pin the LOWEST ONNX RT API it needs (bge
  inference is basic session+run — likely api-17/18), widening the set of
  acceptable DLLs instead of hard-matching 1.20.x.
- Lands on a maintained ort line (rc.13 is 2026-07-28 vs rc.9 2024-11-21).

## Why this is high blast radius (do NOT fold into 068)

1. **Required native DLL moves 1.20 -> 1.23.** recall's `src/embed.rs` hardcodes
   `ORT_VERSION`/`ONNXRUNTIME_VERSION = "1.20.0"` and builds the per-platform
   download URL from it. The deploy scripts fetch that DLL. All of this changes.
2. **ort rc.10+ ABI break** (`Option<fn>` -> bare `fn`): if recall ever touched
   the raw OrtApi it would break, but recall only uses `ort::init_from().commit()`
   — confirm no public-API signatures recall calls changed across rc.9->rc.13.
3. **Embedding parity is not guaranteed.** A different ONNX RT version can produce
   bit-different embeddings. The golden-query suite must be re-baselined
   deliberately, and the corpus may need a re-embed if vectors shift materially
   (embeddings are incompatible across model/runtime changes — same caution as a
   model switch, per README).

## What to build

- [ ] Bump `fastembed` 4.9.1 -> 7.x (latest) in `Cargo.toml`; update recall's
      direct `ort`/`ort-sys` pins to the matching rc.13 (or drop the direct `ort`
      dep if fastembed re-exports what recall needs — recall only calls
      `ort::init_from(..).commit()`).
- [ ] Select the lowest viable `api-NN` feature on `ort` by auditing recall's ort
      call sites (just init + session/run via fastembed) — start at `api-17`,
      raise only if a needed symbol is missing.
- [ ] Update `src/embed.rs` `ORT_VERSION` + the download URL to the ONNX RT
      version rc.13/api-NN expects; update deploy scripts' DLL fetch to match.
      Coordinate with 064's SHA-256 pin (new DLL = new hash per platform).
- [ ] Re-baseline `tests/golden_queries.rs`; decide + document whether a corpus
      re-embed is required (if embeddings shift beyond the suite's tolerance).
- [ ] Update the ADR from 068 with the post-uplift matrix row.

## Acceptance criteria

- [ ] `cargo build --release` and `cargo install --path .` succeed on the new stack
- [ ] Model loads (BGE-base-en-v1.5) against ONNX RT 1.23.x via the api-NN pin
- [ ] All tests pass; golden-query suite re-baselined with a documented rationale
- [ ] `recall health` reports the new ONNX RT version
- [ ] Deploy scripts fetch + verify (064) the correct new DLL per platform
- [ ] Decision recorded on corpus re-embed (needed / not needed, with evidence)

## Notes / relations

- blocked_by **068** (stabilize rc.9 first: fix the resolve, graceful init,
  version logging — gives a known-good baseline to diff against).
- Pairs with **064** (per-platform SHA-256 for the DLL download — the uplift
  changes the DLL, so the pinned hashes change).
- Research: `.scratch/research/ort-fastembed-compat.md` (version table, L1 sources).
