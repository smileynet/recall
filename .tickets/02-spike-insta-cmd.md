---
id: "02"
title: "Spike: evaluate insta-cmd for CLI snapshot testing"
status: done
priority: high
type: spike
blocked_by: []
estimate: 30min
---

# Spike: Evaluate insta-cmd for CLI Snapshot Testing

## Question

Does `insta-cmd` work well for testing recall's CLI output, given that:
- Output contains volatile values (paths, timestamps, chunk counts)
- Commands need a seeded DB (via RECALL_DB env var)
- Some commands load the embedding model (~500ms cold start)

## What to do

1. Add `insta` + `insta-cmd` + `assert_cmd` + `predicates` to dev-dependencies
2. Write one snapshot test for `recall status` against a temp DB with known data
3. Write one snapshot test for `recall health --json`
4. Verify filter redaction works for paths and counts
5. Run `cargo insta review` to confirm the workflow

## Success criteria

- [x] Snapshot tests pass with filtered volatile values
- [x] `cargo insta review` shows clean diffs
- [x] Decide: use insta-cmd vs assert_cmd+predicates vs both

## Output

A brief finding in `.scratch/spikes/insta-cmd.md` with recommendation.
