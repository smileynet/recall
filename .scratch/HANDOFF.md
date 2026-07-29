---
created_at: 2026-07-29T12:20:00-07:00
base_commit: 5cd2368
handoff_key: recall-rust-scaffold
---

# Handoff

## Objective
Build recall into a production Rust CLI that replaces the Python recall (crew-research/tools/recall). The scaffold is working — search, add, ingest, import, status, health, prime, forget all compile and the core path (add + search) is verified. Next: integration tests, DB migration, remaining spikes, then ship v0.1.0.

## Constraints
- CLI interface must match Python recall (same commands, flags, output format)
- Single binary, no runtime dependencies beyond the model cache (~33MB, auto-downloaded on first use)
- Must read/migrate the existing Python recall SQLite DB (~822 MB, 139K chunks)
- crew-research still uses the Python recall — it stays until the Rust binary is proven stable
- Model: BGE-small-en-v1.5 (384-dim), same as the Python version

## Prior Decisions
- Rust over Go: fastembed-rs gives native local embeddings without CGO (spike S1 validated)
- Raw float32 embeddings for v1 (int8 quantization is spike S2, deferred)
- jwalk for parallel file scanning (spike S3 validated: 42ms for 2,624 files)
- SQLite WAL mode + exclusive file lock for crash safety (no wrapper scripts)
- FTS5 for BM25 + brute-force cosine for vector (sqlite-vec deferred to later optimization)
- Hybrid RRF fusion for search (same algorithm as Python version)

## Current State
- Repo: https://github.com/smileynet/recall (2 commits, main branch)
- All 8 commands have entry points in cli.rs (search, add, ingest, import, prime, status, health, forget)
- Verified: `add` + `search` produce correct hybrid results (BM25 + vector RRF)
- Storage: SQLite with WAL, FTS5 index, scan_cache table, embedding BLOBs
- Compiles clean, no tests yet (next step)

## Spike Results (from D:/code/recall-spikes, disposable)
- **S1 (fastembed-rs):** 201ms cold start (model from disk), 3.3ms/embed hot, 210 chunks/sec batch, 465ms total for 45 chunks. ✅ PASS.
- **S3 (stat cache):** jwalk parallel scan: 42ms for 2,624 JSONL files on NTFS. ✅ PASS.
- **S2 (int8 quantization):** NOT RUN. Question: does int8 preserve search ranking on our corpus?
- **S4 (model2vec-rs quality):** NOT RUN. Question: is 10-20% quality loss acceptable for query embedding?

## Architecture (reference: crew-research/.memory/specs/recall-rust-architecture.md)

```
src/
├── main.rs      — entry point
├── cli.rs       — clap commands + dispatch (166 lines)
├── store.rs     — SQLite WAL + FTS5 + scan_cache (268 lines)
├── embed.rs     — fastembed-rs wrapper (29 lines)
├── ingest.rs    — session scanning + chunking + embedding (205 lines)
├── scan.rs      — jwalk stat-based change detection (65 lines)
└── search.rs    — hybrid BM25 + vector RRF fusion (66 lines)
```

## Next Steps

### Immediate (ship v0.1.0)
1. **Integration tests** — add + search round-trip, ingest from fixture files, scan cache hit/miss
2. **Test against real sessions** — point ingest at ~/.kiro/sessions/cli, verify chunks are stored and searchable
3. **DB migration script** — read Python recall's DB (same schema name but different column layout), write to Rust schema
4. **Prime command** — verify output matches Python recall's format (steering integration depends on it)
5. **Spike S2** — quantize a sample of the real 139K embeddings to int8, measure search quality degradation
6. **cargo-dist setup** — same pattern as tkt (5 platforms, shell+powershell installers)
7. **Scheduled task** — replace the PowerShell wrapper with direct binary invocation

### Parity gaps (Python features not yet in Rust)
8. **Session chunking fidelity** — Python uses message-pair chunking with role detection; current Rust uses simplified content extraction. Port the exact chunking logic.
9. **Wing derivation from session metadata** — Python reads cwd from session JSONL to determine the wing; current Rust uses directory name.
10. **recall prime format** — match the exact output format that crew-research's recall-check steering expects.
11. **recall health --json** — full parity with Python's output (discoverable_projects, covered_projects, stale_wings, duplicates).
12. **Import hash-gate** — Python uses file-level SHA-256 to skip re-importing unchanged .memory/ files (ticket 53 in crew-research).

### Optimization (post-v0.1.0)
13. **Int8 quantization** — if spike S2 passes, migrate embeddings to int8 (4× size reduction)
14. **sqlite-vec** — replace brute-force cosine with in-DB vector distance (eliminates loading all embeddings into RAM)
15. **Batch embedding during search** — currently loads model for every search; consider keeping model in memory for interactive sessions vs cold start for scheduled runs

## Fog
- Whether the Python DB schema is directly readable or needs a migration step (column names may differ)
- Whether the current brute-force vector search is fast enough on 139K embeddings (it loads ALL into RAM)
- Whether cargo-dist works with the fastembed/ONNX dependency (native library linking)
- Exact session JSONL format (v2 vs v3 — Python handles both, Rust currently only does simplified parsing)

## Evidence
- Build: `cargo build` (compiles clean)
- Smoke test: `$env:RECALL_DB="D:\tmp\test-recall.sqlite3"; recall add "test" --wing test --type fact; recall search "test"`
- Spike project: D:\code\recall-spikes (disposable — has measured benchmarks)
- Architecture spec: C:\Users\uosmi\code\crew-research\.memory\specs\recall-rust-architecture.md
- Performance research: C:\Users\uosmi\code\crew-research\.scratch\research\{rust-embedding-performance,fast-file-scanning,embedding-db-optimization,robust-background-ingestion}.md
