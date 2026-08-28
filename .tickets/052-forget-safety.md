---
id: "052"
title: "forget/parse_duration: add confirmation, reject negatives, fix multibyte panic"
status: done
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

- [x] Add a confirmation prompt to `cmd_forget` (skip when stdin is not a TTY, or
      add a `--yes`/`--force` flag for scripted use). Show wing + chunk count to be deleted.
- [x] Reject non-positive durations in `parse_duration` (return `None` for `<= 0`)
- [x] Use `s.chars().last()` / char-safe split instead of byte `split_at`
- [x] Unit tests: negative → None, multibyte → None (no panic), zero → None

## Acceptance criteria

- [x] `recall forget` prompts (or requires `--yes`) before deleting; non-TTY safe
- [x] `parse_duration` rejects negative, zero, and multibyte-suffixed input without panic
- [x] `cargo test` passes with new cases

## Validation criteria

- Unit test: `parse_duration("-5d")`, `parse_duration("0d")` → None
- Unit test: `parse_duration("90ð")` (multibyte) → None, no panic
- Manual: `recall forget --wing nonexistent` shows a prompt / respects `--yes`

## Implementation notes & evidence (2026-08-28)

**Changes:**
- `parse_duration` (cli.rs): char-safe suffix split via `chars().next_back()` +
  `len_utf8()`; rejects `num <= 0`; `checked_mul` prevents overflow panic on huge
  values. Added `#[cfg(test)] mod tests` (6 tests — binary-crate unit tests since
  cli.rs isn't in the lib).
- `cmd_forget` (cli.rs): resolves cutoff once, counts impact via new
  `store::count_wing`, then confirms. `--yes` skips the prompt; a non-TTY without
  `--yes` refuses (bail) rather than deleting unattended; empty wing short-circuits
  with "Nothing to delete". Local `read_yes_no`/`stdin_is_tty` helpers (telemetry's
  are private).
- `--yes` flag added to the `Forget` clap command.
- `store::count_wing(conn, wing, older_than)` added.

**Evidence:**
- `cargo test --bin recall`: `6 passed` (parse_duration_valid_units,
  _trims_whitespace, _rejects_non_positive, _rejects_malformed,
  _multibyte_suffix_no_panic, _no_overflow_panic).
- `cargo test`: all suites pass (76 lib + 6 bin + integration/contract/snapshot).
- `cargo clippy --lib --bin recall`: no warnings. `cargo fmt`: applied.
- Manual (debug bin, temp DB): non-TTY `forget` without `--yes` →
  "refusing to delete all 1 chunks... (pass --yes)" exit 1; `--older-than=-5d` →
  "invalid duration '-5d'"; `--yes` → "Deleted 1 chunks"; empty wing →
  "Nothing to delete".
- Deployed recall 0.1.0; `recall forget --help` shows `--yes`.

## Resolution (2026-08-28)

cmd_forget counts impact + confirms (--yes skips, non-TTY refuses, empty wing short-circuits); parse_duration char-safe, rejects non-positive + overflow via checked_mul. Added --yes flag and store::count_wing.

### Verification
1. ✓ cargo test passes — "cargo test: 76 lib + 6 bin (parse_duration) + integration/contract/snapshot all pass"
2. ✓ forget prompts before deleting; negative/invalid durations rejected — "manual: non-TTY forget without --yes refuses (exit 1); --older-than=-5d rejected 'invalid duration'; --yes deletes; deployed recall 0.1.0 forget --help shows --yes"
