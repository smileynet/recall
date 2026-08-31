# recall

Cross-session semantic memory for AI coding assistants. Remembers decisions, past work, and preferences across sessions using hybrid BM25 + vector search.

## What It Does

Your AI assistant forgets everything between sessions. recall fixes that — a single binary that ingests session transcripts, embeds them locally, and provides instant hybrid search across all your past work.

| When I'm... | I want to... | So I can... |
|-------------|-------------|-------------|
| Starting a session | get context from last time | continue without re-discovery |
| Asked "what did we decide about X?" | search past decisions | give accurate answers, not guesses |
| Making a decision | persist it with rationale | recall it months later |
| Switching between projects | search cross-project | find patterns and reuse approaches |
| Onboarding to a codebase | import .memory/ docs | have project knowledge searchable |

No server, no API keys, no network at runtime. Runs locally with a cached ONNX embedding model.

## Quick Start

```bash
# Install (requires Rust toolchain)
cargo install --path .

# First run downloads the embedding model (~416MB, cached for future use)
recall ingest ~/.kiro/sessions/cli
#   Ingesting: ~/.kiro/sessions/cli
#   Files: 47 changed of total
#   Done: 47 files, 312 chunks ingested

# Search past decisions
recall search "what did we decide about authentication"
#   Results for: "what did we decide about authentication"
#
#   [1] web_app / decisions
#       Score: 0.031
#       > We decided to use JWT tokens with 15-minute expiry...

# Import project knowledge
recall import .memory/ --wing my-project
#   Done: 12 new (87 chunks indexed)

# Agent writes back during a session
recall add "chose Rust for the rebuild — fastembed-rs gives native embeddings" --type decision
#   Stored in my_project/general (type: decision)

# Session start payload
recall prime --wing my-project
#   ## Recall - Cross-Session Memory
#   ## Recent Memories (my_project)
#   - [decision] chose Rust for the rebuild...
```

## Installation

**From source** (requires Rust 1.70+):
```bash
cargo install --path .
# Installs to ~/.cargo/bin/recall
```

**Copy pre-built binary:**
```bash
# After building with: cargo build --release
cp target/release/recall ~/.cargo/bin/
```

**First run:** The BGE-base-en-v1.5 embedding model (~416MB) downloads automatically on first use and caches at `~/.recall/models/`. No network needed after that.

**Verify:**
```bash
recall --version
# recall 0.1.0
```

## Usage

### Search

```bash
recall search "query"                     # search all projects
recall search "auth" --wing web_app       # scope to one project
recall search "architecture" --results 10 # more results
```

### Write back

```bash
recall add "We chose X because Y" --type decision
recall add "Never use approach Z" --type lesson
recall add "User prefers dark mode" --type preference
# Wing auto-detects from cwd. Types: decision | fact | lesson | preference
```

### Ingest sessions

```bash
recall ingest                    # default: ~/.kiro/sessions/cli
recall ingest /path/to/sessions  # custom path
# Skips files modified < 5 min ago (in-progress sessions)
# Skips unchanged files (stat cache)
```

### Import project knowledge

```bash
recall import .memory/ --wing my_project        # import markdown files
recall import .memory/ --wing my_project --force # reimport from scratch
recall import-all                               # import all projects' .memory/
```

### Monitor

```bash
recall status         # corpus overview (wings, chunk counts)
recall health         # coverage, staleness, diagnostics
recall health --json  # machine-readable (for scripts/doctor.sh)
```

## Configuration

| Variable | Purpose | Default |
|----------|---------|---------|
| `RECALL_DB` | Database path | `~/.recall/recall.sqlite3` |
| `RECALL_MODEL` | Embedding model | `bge-base` (768-dim) |
| `FASTEMBED_CACHE_DIR` | Model cache | `~/.recall/models/` |

Supported models: `bge-base` (768-dim, default), `bge-small` (384-dim, faster). Switching models requires re-ingesting — embeddings are incompatible between models.

## How It Works

```
┌──────────────────────────────────────────────────────┐
│  Ingestion (background, scheduled)                    │
│  1. Stat-scan session files (42ms for 2,800)         │
│  2. Hash changed files only                          │
│  3. Chunk → Embed → Store (135 chunks/sec)           │
└──────────────────────────────────────────────────────┘
                        ↓
┌──────────────────────────────────────────────────────┐
│  Storage (SQLite, single file)                        │
│  - FTS5 index for keyword search (BM25)              │
│  - Vector embeddings for semantic search             │
│  - Scan cache for change detection                   │
└──────────────────────────────────────────────────────┘
                        ↓
┌──────────────────────────────────────────────────────┐
│  Search (interactive)                                 │
│  1. Embed query (19ms)                               │
│  2. BM25 search (FTS5) + vector cosine similarity    │
│  3. RRF fusion → ranked results                      │
└──────────────────────────────────────────────────────┘
```

Scheduled runs use `recall sync` (ingest + import-all in one process) — on Windows a `RecallIngest` task every 6 hours. This refreshes both session transcripts and project `.memory/` knowledge in a single lock-holding run.

## Performance

| Operation | Time |
|-----------|:----:|
| No-change scan (2,800 files) | 42ms |
| Embed + store 64 chunks | 474ms |
| Single embedding | 19ms |
| Search query (25K chunks) | ~1.5s |
| Import unchanged (hash-gate) | instant |
| Binary size | ~25 MB |

## Development

```bash
cargo build              # debug build
cargo test               # 81 tests (~8s)
cargo test --lib         # unit tests only (<1s, no model)
cargo clippy             # lint
cargo build --release    # optimized (~25MB, stripped + LTO)
```

## Inspired By

**[MemPalace](https://github.com/MemPalace/mempalace)** — The wings/rooms/drawers taxonomy and the concept of agent-owned memory. recall adapts MemPalace's architecture as a purpose-built single-binary implementation.

**[Google OKF](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf)** — The "nouns, not verbs" insight that shapes how project knowledge (`.memory/`) stays separate from behavior (skills). recall's import understands OKF-compatible frontmatter.

**[fastembed](https://github.com/Anush008/fastembed-rs)** — Local ONNX embedding inference that makes server-free semantic search practical. The Rust port by the Qdrant team powers recall's 19ms-per-embedding performance.

## License

MIT
