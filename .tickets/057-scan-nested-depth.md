---
id: "057"
title: "scan.rs: support nested session dirs for kiro v3 layouts"
status: backlog
blocked_by: []
validation_criteria:
  - "nested v3 session files detected by scan"
---

# scan.rs: support nested session dirs for kiro v3 layouts

## Context

From the 2026-08-23 review (deferred in 048). **Latent, not currently biting** —
filed as backlog to activate when the trigger appears.

`scan.rs` walks the sessions directory with `max_depth(1)` (top-level files
only). If kiro ships a nested session layout (v3 with subdirectories), those
files would be silently skipped by ingest.

Verified at review time: 56 production session dirs, 0 nested today — so no
current data loss. This is pre-emptive.

## Trigger to activate

- kiro-cli starts writing session files into subdirectories under
  `~/.kiro/sessions/cli/`, OR
- `recall health` coverage drops unexpectedly after a kiro upgrade

## What to build (when triggered)

- [ ] Confirm the actual nested layout kiro produces (don't guess the depth)
- [ ] Increase/relax the `jwalk` `max_depth` in `scan.rs` to cover it
- [ ] Ensure `derive_wing_from_session` still derives the right wing from a nested path
- [ ] Add a fixture with a nested session file + a scan test

## Acceptance criteria

- [ ] Nested v3 session files are detected by `scan_for_changes`
- [ ] Wing derivation correct for nested paths
- [ ] `cargo test` passes with a nested fixture
