# AGENTS.md

## Project

recall — Cross-session semantic memory for AI coding assistants. Single Rust binary providing hybrid BM25 + vector search over ingested session transcripts and project knowledge.

Rebuilt in Rust from the original Python implementation. Same CLI interface, dramatically faster. Deployed locally and running as a scheduled task.

## Workspace Layout

```
src/
├── main.rs           — entry point
├── lib.rs            — public module re-exports (for integration tests)
├── cli.rs            — clap derive commands + dispatch
├── store.rs          — SQLite layer (FTS5, scan_cache, embeddings, meta)
├── embed.rs          — fastembed-rs wrapper (configurable model, cache path)
├── ingest.rs         — session parsing (v2/v3/codex) + chunking + ingestion
├── search.rs         — hybrid search (BM25 + vector RRF fusion)
├── scan.rs           — stat-based file change detection (jwalk)
├── migrate.rs        — Python DB migration (direct embedding copy)
├── bin/
│   ├── bench_models.rs   — model comparison benchmark
│   └── bench_quality.rs  — search quality comparison
tests/
├── common/mod.rs         — shared helpers (OnceLock embedder, test_db)
├── integration_test.rs   — add+search, ingest, scan cache
├── integration_expanded.rs — wing scoping, import lifecycle, formats
├── golden_queries.rs     — search quality regression (15-chunk corpus)
├── cli_errors.rs         — error handling (assert_cmd)
├── cli_contract.rs       — health/prime output format contracts
├── cli_snapshot.rs       — insta-cmd output snapshots
├── fixtures/             — JSONL fixtures (v2, v3, codex), memory/ sample
.memory/CONTEXT.md    — project glossary + environment + gotchas
.tickets/             — work tracking (23 tickets, 19 done)
```

## Commands

```bash
cargo build                    # debug build
cargo build --release          # release (stripped, LTO, ~25MB)
cargo test                     # all 81 tests
cargo test --lib               # unit tests only (no model, <1s)
cargo clippy                   # lint
cargo fmt                      # format
cargo insta review             # review snapshot changes
```

## recall CLI (the product)

```bash
recall search "query" [--wing W] [--results N]     # hybrid semantic search
recall add "fact" [--wing W] --room R --type T     # agent write-back (wing auto from cwd)
recall ingest [path]                               # background ingestion (skips active files)
recall import .memory/ --wing W [--force]          # bulk import markdown
recall import-all [--force]                        # import all .memory/ from D:/code
recall prime [--wing W]                            # session start payload
recall status                                      # corpus overview
recall health [--json]                             # diagnostics (doctor.sh compatible)
recall forget --wing W [--older-than 90d]          # delete a wing
recall migrate --from <path> [--embed]             # migrate Python DB
recall --version                                   # version info
```

## Architecture

- **Storage:** SQLite WAL mode, single file (`~/.recall/recall.sqlite3`)
- **Text search:** FTS5 (BM25 ranking)
- **Vector search:** fastembed-rs (BGE-base-en-v1.5, 768-dim, ONNX Runtime)
- **Model cache:** `~/.recall/models/` (stable, not CWD-relative)
- **Change detection:** stat cache (mtime+size → only hash if metadata differs)
- **Concurrency:** exclusive file lock (fs2) — auto-releases on crash
- **Crash safety:** WAL mode + batch commits + checkpoint after bulk ops
- **Configuration:** RECALL_DB (path), RECALL_MODEL (bge-base/bge-small), FASTEMBED_CACHE_DIR

## Deployment

- Binary: `~/.cargo/bin/recall.exe` (v0.1.0)
- Scheduled task: `RecallIngest` (every 30 min, direct binary)
- Corpus: ~44K chunks, 69 wings, 47/47 project coverage
- Model: BGE-base-en-v1.5 (~416MB cached ONNX)
- ONNX Runtime: load-dynamic (`~/.recall/lib/onnxruntime.dll`)

### Updating

```bash
./scripts/deploy-local.ps1              # Windows (PowerShell)
./scripts/deploy-local.sh               # macOS/Linux
./scripts/deploy-local.ps1 -SkipTests   # skip unit tests (already passed)
```

Scripts do: test → build (--locked) → backup → copy → verify → health check → report scheduled task status. Rolls back automatically if verification fails.

Note: `cargo install --path .` is broken (ticket #049, ort dependency). Use the deploy scripts instead.

## Performance (measured on production corpus)

| Operation | Result |
|-----------|:------:|
| No-change scan (2,800 files) | 42ms |
| Model cold start | ~500ms |
| Single embedding | 19ms |
| Batch 64 chunks | 474ms (135/sec) |
| Search (25K chunks, warm) | ~1.5s |
| Search (cold start) | ~5s |
| Full ingest (2,765 files) | ~68 min |
| Import unchanged (hash-gate) | instant |

## Constraints

- Same CLI interface as Python recall (commands, flags, output format)
- No daemon / no server — single binary, OS scheduler for background tasks
- No network dependencies at runtime (model cached locally after first download)
- Keep helper `.ps1` scripts ASCII-only (no em-dashes/smart quotes). Windows PowerShell 5.1 decodes BOM-less files with the ANSI codepage and mangles non-ASCII bytes into a parse error; `pwsh` 7 defaults to UTF-8 so the bug is invisible there. ASCII works under both. (ticket 065)
