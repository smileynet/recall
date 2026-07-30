---
id: 17
title: "Add missing CLI flags required for local deployment"
status: open
priority: high
blocked_by: [16]
estimate: 1h
---

# Missing CLI Flags for Deployment

## What to build

Based on spike #016 findings, implement the flags needed for crew-research compatibility.

### Must-have (blocks deployment)

1. **`--version`** — `recall --version` → `recall 0.1.0` (clap derive: `#[command(version)]`)
2. **Active-file skip** — during ingest, skip files with mtime < 5 minutes ago (in-progress sessions)
3. **`--force` on import** — delete all existing import chunks for the wing, then re-import

### Should-have (improves experience)

4. **`--project` on ingest** — filter sessions to only those from a specific project cwd
5. **`--room` on search** — filter search results by room
6. **`--type` on search** — filter search results by type (decision, fact, etc.)
7. **Wing normalization** on add/search — hyphens → underscores (prevent split wings)

### Nice-to-have (defer)

8. `recall gc --older-than 90d` — time-based cleanup
9. Auto-import .memory/ after ingest

## Acceptance criteria

- [ ] Must-have flags implemented and tested
- [ ] Should-have flags implemented
- [ ] All existing tests still pass
- [ ] New flags have CLI error tests (missing required args)
