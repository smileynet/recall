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
├── ingest.rs         — session parsing (v2/v3-JSONL/codex + v3 SQLite) + chunking + ingestion
│                       (SQLite source preferred when present; JSONL fallback)
├── sqlite_source.rs  — read-only reader for kiro-cli v3 data.sqlite3 (conversations_v2)
├── search.rs         — hybrid search (BM25 + vector RRF fusion)
├── scan.rs           — stat-based file change detection (jwalk)
├── migrate.rs        — Python DB migration (direct embedding copy)
├── guard.rs          — process lock (single-instance) + scaled execution timeout
├── archive.rs        — archive (zip/tar) extraction for model + runtime downloads
├── logging.rs        — timestamped file logging + active-session detection
├── telemetry.rs      — opt-in local telemetry + crash reporting (path redaction)
├── update.rs         — self-update (version check, download, digest verify)
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
.tickets/             — work tracking (see `tkt status`)
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
recall forget --wing W [--older-than 90d] [--yes]  # delete a wing (--yes for non-interactive)
recall migrate --from <path> [--embed] [--force]   # migrate Python DB
recall sync [--force] [--skip-import|--skip-ingest] # ingest + import-all in one process
recall telemetry <status|enable|disable>           # manage local telemetry + crash reporting
recall update                                      # check for and install updates
recall --version                                   # version info
```

## Architecture

- **Storage:** SQLite WAL mode, single file (`~/.recall/recall.sqlite3`)
- **Text search:** FTS5 (BM25 ranking)
- **Vector search:** fastembed-rs (BGE-base-en-v1.5, 768-dim, ONNX Runtime)
- **Model cache:** `~/.recall/models/` (stable, not CWD-relative)
- **Change detection:** stat cache (mtime+size → only hash if metadata differs) for JSONL; `updated_at` watermark in `meta` (`kiro_sqlite_watermark_ms`) for the SQLite source
- **Session sources:** kiro-cli v3 SQLite (`~/.local/share/kiro-cli/data.sqlite3`, opened READ-ONLY, preferred when present) with the legacy JSONL tree as fallback; SQLite chunks keyed `source=kiro-sqlite:<conversation_id>`. recall never takes a write lock on the kiro DB. See `.memory/adr/0001`, `0002`.
- **Concurrency:** exclusive file lock (fs2) on the recall DB only — auto-releases on crash
- **Crash safety:** WAL mode + batch commits + checkpoint after bulk ops
- **Configuration:** RECALL_DB (path), RECALL_MODEL (bge-base/bge-small), FASTEMBED_CACHE_DIR

## Deployment

- Binary: `~/.cargo/bin/recall.exe` (v0.1.0)
- Scheduled task: `RecallIngest` runs `recall sync` every 4h (`IgnoreNew` — a run still in flight when the next trigger fires is skipped, not stacked; ExecutionTimeLimit `PT72H` on the scheduler, with recall's own 2h app-guard watchdog (exit 2) the effective ceiling)
- Corpus: ~44K chunks, 69 wings, 47/47 project coverage
- Model: BGE-base-en-v1.5 (~416MB cached ONNX)
- ONNX Runtime: load-dynamic (`~/.recall/lib/onnxruntime.dll`). A libloading pre-flight validates the cached dylib (`OrtGetApiBase` + `GetVersionString` minor == `ORT_VERSION`) before `ort` loads it, turning ort's internal panic on a bad DLL into a graceful domain error; `recall health` reports the loaded version. Version coupling (fastembed↔ort↔ort-sys↔ONNX RT) is pinned and documented in `.memory/adr/0003`.

### Updating

```bash
./scripts/deploy-local.ps1              # Windows (PowerShell)
./scripts/deploy-local.sh               # macOS/Linux
./scripts/deploy-local.ps1 -SkipTests   # skip unit tests (already passed)
```

Scripts do: test → build (--locked) → backup → copy → verify → health check → report scheduled task status. Rolls back automatically if verification fails.

Note: `cargo install --path .` works from a clean resolve. recall directly pins `ort`/`ort-sys` to the fastembed-dictated version (currently `=2.0.0-rc.13`, matching fastembed 7.1.0) so a lockfile-free resolve can't drift to an ABI-incompatible pre-release (the rc.9→rc.10 drift that originally broke it, ticket 068). Requires rust ≥ 1.88 (fastembed 7 MSRV). The deploy scripts remain the recommended path for updating the running install (they test, back up, and health-check). See `.memory/adr/0003`.

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
- ONNX RT download (`src/embed.rs`) is integrity-checked by a pinned per-platform SHA-256 of the archive, verified before extraction; hashes are vendored per `ORT_VERSION` in `ort_platform()` and MUST be re-vendored on any ORT version bump (ties into ticket 075). The tgz/zip ships `libonnxruntime_providers_shared.so` (~14KB sidecar) BEFORE the real multi-MB lib — the entry matcher rejects `libonnxruntime_*` sidecars and asserts a >1MB extracted size, else it caches a stub with no `OrtGetApiBase`. (ticket 064)
