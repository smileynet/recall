# AGENTS.md

## Project

recall — Cross-session semantic memory for AI coding assistants. Single Rust binary providing hybrid BM25 + vector search over ingested session transcripts and project knowledge.

Rebuilt in Rust from the original Python implementation (crew-research/tools/recall). Same CLI interface, dramatically faster (42ms scan vs 96s, 465ms ingest vs 701s).

## Workspace Layout

```
src/
├── main.rs           — entry point
├── cli.rs            — clap derive commands + dispatch
├── store.rs          — SQLite layer (FTS5, scan_cache, embeddings)
├── embed.rs          — fastembed-rs wrapper (model load, batch embed)
├── ingest.rs         — session file scanning + chunking + ingestion
├── search.rs         — hybrid search (BM25 + vector RRF fusion)
└── scan.rs           — stat-based file change detection (jwalk)
tests/                — unit + integration tests
.memory/CONTEXT.md    — project glossary
.memory/specs/        — architecture decisions
.tickets/             — work tracking
```

## Commands

```bash
cargo build                    # debug build
cargo build --release          # release (stripped, LTO)
cargo test                     # all tests
cargo clippy                   # lint
cargo fmt                      # format
```

## recall CLI (the product)

```bash
recall search "query" [--wing W] [--results N]     # hybrid semantic search
recall add "fact" --wing W --room R --type T       # agent write-back
recall ingest [path]                               # background ingestion
recall import .memory/ --wing W                    # bulk import markdown
recall prime                                       # session start payload
recall status                                      # corpus overview
recall health --json                               # machine-readable diagnostics
recall forget --wing W [--older-than 90d]          # GC
```

## Architecture

- **Storage:** SQLite WAL mode, single file (`~/.recall/recall.sqlite3`)
- **Text search:** FTS5 (BM25 ranking)
- **Vector search:** fastembed-rs (BGE-small-en-v1.5, 384-dim, ONNX Runtime)
- **Change detection:** stat cache (mtime+size → only hash if metadata differs)
- **Concurrency:** exclusive file lock (fs2) — auto-releases on crash
- **Crash safety:** WAL mode + batch commits + checkpoint/resume

## Performance Targets (from spikes)

| Operation | Target | Spike result |
|-----------|--------|:------------:|
| No-change scan (2,600 files) | < 100ms | 42ms ✅ |
| Model cold start | < 500ms | 201ms ✅ |
| Single embedding | < 10ms | 3.3ms ✅ |
| Batch 45 chunks (cold) | < 5s | 465ms ✅ |
| Search (single query) | < 200ms | TBD |

## Constraints

- Same CLI interface as Python recall (commands, flags, output format)
- No daemon / no server — single binary, OS scheduler for background tasks
- Must read existing Python recall's SQLite DB (migration path)
- No network dependencies at runtime (model cached locally after first download)
