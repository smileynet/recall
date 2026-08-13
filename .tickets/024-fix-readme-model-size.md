---
id: "024"
title: "Fix README model cache size claim (~83MB → ~416MB)"
status: done
blocked_by: []
estimate: 5min
---

# Fix README Model Cache Size

## What to do

The README states the embedding model is "~83MB" in two places (Quick Start and Installation sections). The actual on-disk size is ~416MB (FP32 ONNX model, confirmed via HuggingFace metadata and local measurement).

AGENTS.md already has the correct figure. The README is the only place with the wrong number.

## Research findings

- `BGEBaseENV15` (fastembed-rs) downloads from `Xenova/bge-base-en-v1.5`, file `onnx/model.onnx`
- FP32 model: 415.72MB (confirmed by Teradata/HuggingFace listing)
- INT8 quantized: 104.75MB (available as `BGEBaseENV15Q`, not what recall uses)
- The ~83MB was likely the compressed transfer size or confusion with a quantized variant

## Changes required

1. `README.md` line in Quick Start: `~83MB` → `~416MB`
2. `README.md` line in Installation section: `~83MB` → `~416MB`

## Acceptance criteria

- [x] Both README references updated to ~416MB
- [x] No other docs reference the wrong size

## Resolution (2026-08-01)

Fixed in commit 3f5576b. Both README occurrences updated. AGENTS.md already had the correct figure. Handoff updated (gitignored).
