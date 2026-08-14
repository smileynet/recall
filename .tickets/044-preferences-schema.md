---
id: "044"
title: "Schema migration — add preference metadata columns"
status: open
blocked_by: ["043"]
priority: medium
---

# Schema migration — add preference metadata columns

## What to build

Add columns to the `chunks` table per the design doc:

- `usage_count INTEGER DEFAULT 0`
- `last_used_at INTEGER` (epoch, nullable)
- `scope TEXT DEFAULT 'wing'` (wing | global)

Must be backward-compatible: existing data untouched, no re-embed required.

## Acceptance criteria

- [ ] Migration adds columns without data loss (test with populated DB)
- [ ] `recall add --type preference` stores a chunk with the new metadata
- [ ] Existing chunks queryable with new columns defaulting correctly
- [ ] Unit test: roundtrip preference storage and retrieval
