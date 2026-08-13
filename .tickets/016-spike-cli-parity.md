---
id: "016"
title: "Spike: CLI parity gap analysis (Rust vs Python flags)"
status: done
priority: high
type: spike
blocked_by: []
estimate: 20min
---

# Spike: CLI Parity Gap Analysis

## Question

What flags/features does the Python recall CLI support that the Rust version doesn't?

## Known gaps from research

| Feature | Python | Rust | Impact |
|---------|:------:|:----:|--------|
| `--room` filter on search | ✓ | ✗ | Low (rarely used) |
| `--type` filter on search | ✓ | ✗ | Low |
| `--project` filter on ingest | ✓ | ✗ | Medium (scoped ingest) |
| `--force` on import | ✓ | ✗ | Medium (reimport after changes) |
| `recall gc --older-than N` | ✓ | ✗ | Low (forget covers this partially) |
| `--version` flag | ✓ | ✗ | Must have |
| Active-file skip (< 5min mtime) | ✓ | ✗ | Medium (avoids ingesting in-progress) |
| Auto-import .memory/ after ingest | ✓ | ✗ | Nice-to-have |
| Wing normalization (hyphens→underscores) | ✓ | ✓ | Done in wing derivation |

## What to do

1. Read Python CLI's argparse definitions vs Rust clap definitions
2. List all flags/behaviors present in Python but missing in Rust
3. Categorize: must-have for deployment / nice-to-have / skip

## Success criteria

- [ ] Complete gap list with priority annotations
- [ ] Decision on which gaps to fill before local deployment
