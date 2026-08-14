---
id: "047"
title: "Golden query comparison — does preference boosting help?"
status: open
blocked_by: ["045"]
priority: medium
---

# Golden query comparison — does preference boosting help?

## What to build

Validate that preference boosting actually improves search quality:

1. Add 3-5 preference-typed chunks to the golden query test fixture corpus
2. Run golden queries with boosting enabled vs disabled
3. Document: do preferences surface at the right rank?

This is a validation gate — if boosting doesn't measurably help, reconsider the approach.

## Acceptance criteria

- [ ] Test fixture includes preference-typed chunks
- [ ] Comparison run documented (boost on vs off, rank positions)
- [ ] Decision: boosting helps (keep) or doesn't (revert/rethink)
- [ ] Results recorded in the design doc or as a comment on this ticket
