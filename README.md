# recall

Cross-session semantic memory for AI coding assistants. Remembers decisions, past work, and preferences across sessions using hybrid BM25 + vector search.

## What It Does

Your AI assistant forgets everything between sessions. recall fixes that:

- **Search** past decisions with natural language
- **Ingest** session transcripts automatically (scheduled)
- **Import** project knowledge (`.memory/` directories)
- **Remember** facts written by the agent during sessions

Single binary, no server, no API keys. Runs locally with a cached embedding model.

## Install

```bash
# From source
cargo install recall

# Pre-built binary
cargo binstall recall
```

## Quick Start

```bash
# Ingest your kiro sessions
recall ingest ~/.kiro/sessions/cli

# Search for a past decision
recall search "what did we decide about authentication"

# Import project knowledge
recall import .memory/ --wing my-project

# Agent writes a fact during a session
recall add "decided to use Rust for the rebuild" --wing my-project --type decision

# Session start: get context
recall prime
```

## Performance

Built in Rust for instant startup and fast search:

| Operation | Time |
|-----------|:----:|
| No-change scan (2,800 files) | 42ms |
| Embed + store 64 chunks | 474ms |
| Single embedding | 19ms |
| Search query (25K chunks) | ~1.5s |
| Binary size | ~25 MB |

## How It Works

```
┌──────────────────────────────────────────────────────┐
│  Ingestion (background, scheduled)                    │
│  1. Stat-scan session files (42ms for 2,600)         │
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
│  Search (interactive, instant)                        │
│  1. Embed query (19ms)                               │
│  2. BM25 search (FTS5) + vector similarity           │
│  3. RRF fusion → ranked results                      │
└──────────────────────────────────────────────────────┘
```

## Inspired By

**[MemPalace](https://github.com/MemPalace/mempalace)** — The wings/rooms/drawers taxonomy and the concept of agent-owned memory. recall adapts MemPalace's architecture as a purpose-built single-binary implementation.

**[Google OKF](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf)** — The "nouns, not verbs" insight that shapes how project knowledge (`.memory/`) stays separate from behavior (skills). recall's import understands OKF-compatible frontmatter.

**[fastembed](https://github.com/Anush008/fastembed-rs)** — Local ONNX embedding inference that makes server-free semantic search practical. The Rust port by the Qdrant team powers recall's 19ms-per-embedding performance.

## License

MIT
