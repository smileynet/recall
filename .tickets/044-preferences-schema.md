---
id: "044"
title: "Schema migration — implement the constrained preference model"
status: open
blocked_by: ["043"]
priority: medium
---

# Schema migration — implement the constrained preference model

## What to build

Implement the enforceable preference representation selected by #043. Do not
pre-commit to nullable metadata columns on every `chunks` row: preference-only
state must not be representable on arbitrary facts, and scope/lifecycle rules
must be enforced by the selected storage model or typed boundary.

The migration must be backward-compatible: existing data remains untouched and
does not require re-embedding.

## Acceptance criteria

- [ ] Migration adds the selected representation without data loss (test with
      populated DB)
- [ ] Storage representation, scope values, and lifecycle combinations enforce
      the invariants decided in #043
- [ ] `recall add --type preference` stores a chunk with the new metadata
- [ ] Existing chunks queryable with new columns defaulting correctly
- [ ] Unit test: roundtrip preference storage and retrieval
