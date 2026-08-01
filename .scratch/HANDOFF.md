---
created_at: 2026-08-01T05:40:00-07:00
base_commit: be26bb8
handoff_key: recall-local-deployment
---

# Handoff

## Objective
Replace Python recall with a Rust single-binary equivalent. Deploy locally, validate, then publish v0.1.0.

## Constraints
- CLI interface matches Python recall (same commands, flags, output format)
- Single binary, no runtime deps beyond cached ONNX model (~83MB, auto-downloads on first use)
- Model: BGE-base-en-v1.5 (768-dim) — matches Python corpus, configurable via RECALL_MODEL

## Prior Decisions
- bge-base over bge-small: quality difference negligible (3%), keep base for migration compat
- Direct embedding copy during migration (same model = no re-embed needed)
- WAL mode + synchronous=NORMAL: correct tradeoff (process kill safe, power loss risk acceptable)
- Model cache at ~/.recall/models/ (not CWD-relative — was a bug, fixed)
- Scheduled task every 6 hours (direct binary, no wrapper script)

## Current State
Deployed locally and running. Binary at ~/.cargo/bin/recall.exe (v0.1.0, 25.4MB). Scheduled task fires every 6h. Corpus: 26,395 chunks, 56 wings, 38/38 project coverage. 81 tests passing. All features implemented. README, AGENTS.md, CONTEXT.md all current.

Open tickets (deferred polish): #010 telemetry, #018 profile hook spike, #019 ingest perf, #020 non-TTY logging.

## Next Steps
1. **Soak test** — let the scheduled task run for a week, verify no crashes or data issues
2. **cargo-dist setup** — cross-platform release binaries for v0.1.0 public release
3. **#014 remainder** — if crew-research is restored from sparse, update AGENTS.md and scripts there
4. **#010 telemetry** — local JSONL opt-in usage tracking (2h, no deps)

## Fog
- Whether cargo-dist handles the ONNX static-link correctly across platforms (research says yes, untested)
- Whether the 416MB model cache is the quantized or full-precision variant (research expected ~83MB)
- import-all doesn't run as part of the scheduled task — only ingest does. Projects need manual initial import.

## Evidence
- E2E validation: `recall health` → 38/38 coverage, 26K chunks
- Test suite: `cargo test` → 81 tests green
- Performance: search ~1.5s warm, ingest ~68 min full (incremental is seconds)
- Benchmark: src/bin/bench_models.rs and src/bin/bench_quality.rs
