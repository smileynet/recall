---
id: "052"
title: "forget/parse_duration: add confirmation, reject negatives, fix multibyte panic"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "cargo test passes"
  - "forget prompts before deleting; negative/invalid durations rejected"
---

# forget/parse_duration: add confirmation, reject negatives, fix multibyte panic

## Context

From the 2026-08-23 review (deferred in 048). `recall forget` is destructive and
`parse_duration` has two correctness bugs. Verified against current `src/cli.rs`
(`cmd_forget` ~L719, `parse_duration` ~L742).

## Problems (verified in code)

1. **No confirmation before delete.** `recall forget --wing X` deletes all chunks
   in a wing (or older-than a cutoff) with no prompt. A typo'd wing name silently
   wipes data. This is the only destructive command and it has no guardrail.

2. **`parse_duration` accepts negatives.** `parse_duration("-5d")` parses to a
   negative i64. In `cmd_forget` the cutoff becomes `now - (negative)` = a future
   timestamp, so `delete_wing_older_than` matches *everything* — an unbounded
   delete from what looks like a scoped one.

3. **Multibyte panic.** `s.split_at(s.len() - 1)` splits on a byte boundary. A
   duration string ending in a multibyte UTF-8 char (e.g. a pasted `90d…`) panics
   with "byte index is not a char boundary".

## What to build

- [ ] Add a confirmation prompt to `cmd_forget` (skip when stdin is not a TTY, or
      add a `--yes`/`--force` flag for scripted use). Show wing + chunk count to be deleted.
- [ ] Reject non-positive durations in `parse_duration` (return `None` for `<= 0`)
- [ ] Use `s.chars().last()` / char-safe split instead of byte `split_at`
- [ ] Unit tests: negative → None, multibyte → None (no panic), zero → None

## Acceptance criteria

- [ ] `recall forget` prompts (or requires `--yes`) before deleting; non-TTY safe
- [ ] `parse_duration` rejects negative, zero, and multibyte-suffixed input without panic
- [ ] `cargo test` passes with new cases

## Validation criteria

- Unit test: `parse_duration("-5d")`, `parse_duration("0d")` → None
- Unit test: `parse_duration("90ð")` (multibyte) → None, no panic
- Manual: `recall forget --wing nonexistent` shows a prompt / respects `--yes`
