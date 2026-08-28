---
id: "061"
title: "Test coverage for forget confirmation (close 052 validation gaps)"
status: in_progress
blocked_by: ["052"]
priority: high
validation_criteria:
  - "decide() matrix unit-tested"
  - "assert_cmd tests cover non-tty refuse, --yes, empty wing, bad duration"
---

# Test coverage for forget confirmation (close 052 validation gaps)

## Context

052 fixed `forget`/`parse_duration` and shipped, but its confirmation path was
validated by manual observation, not automated tests. Three gaps remained:
1. The exit-1-on-bad-input claim was hand-waved (`exit: 0` artifact dismissed).
2. Non-TTY refusal / `--yes` bypass / empty-wing short-circuit had no automated test.
3. The interactive `[y/N]` branch had zero coverage.

Grounded by research + code review 2026-08-28 (`.scratch/research/assert-cmd-testing.md`,
`.scratch/research/testable-prompts.md`, `.scratch/review/cli-errors-harness.md`,
`.scratch/review/forget-structure.md`).

## Key finding

`assert_cmd` stdin is ALWAYS a pipe → `is_terminal()` is `false` in the child, so
the interactive y/N branch is **untestable via assert_cmd** (would need a PTY /
rexpect). Therefore the y/N decision logic must be extracted into a pure function
to get coverage. This makes the refactor a requirement, not a nice-to-have.

## What to build

### A. Testable refactor (`src/cli.rs`)
- [ ] Add `enum Decision { Proceed, Abort, RefuseNonInteractive, NeedsPrompt }`
      and `fn decide(assume_yes: bool, is_tty: bool, answer: Option<&str>) -> Decision`
      (pure). Rules: yes→Proceed; !yes&&!tty→RefuseNonInteractive; !yes&&tty→
      NeedsPrompt; with an answer, "y"/"yes"→Proceed else Abort.
- [ ] Route `cmd_forget`'s branch (currently entangled at cli.rs L753-771) through
      `decide`. `stdin_is_tty()` resolved once at the edge, passed as bool
      (`IsTerminal` is a sealed trait — can't mock). `parse_duration` already pure — unchanged.
- [ ] Unit-test the `decide` matrix in `#[cfg(test)] mod tests` (no stdin/terminal):
      proceed-on-yes, refuse-on-nontty, proceed-on-"y"/"yes", abort-on-"n",
      abort-on-empty/EOF, force-supersedes if applicable.

### B. `assert_cmd` integration tests (`tests/cli_errors.rs`)
Seed model-free (`store::insert_chunk_atomic` takes an arbitrary `&[f32]`, e.g.
`&[0.1f32; 768]`), share the DB by pointing an in-process `Connection` at the
SAME path passed to `cmd.env("RECALL_DB", …)`, and DROP the connection before
spawning (WAL flush; avoid `set_var` process-global race).

- [ ] `forget_non_tty_refuses_without_yes`: seed → `forget --wing X` → `.failure()`
      + stderr contains "refusing to delete"  (pins the exit-1 I hand-waved)
- [ ] `forget_yes_deletes`: seed → `forget --wing X --yes` → `.success()` + "Deleted 1"
- [ ] `forget_empty_wing_no_prompt`: `forget --wing none --yes` → `.success()` + "Nothing to delete"
- [ ] `forget_negative_duration_rejected`: seed → `forget --wing X --older-than=-5d --yes`
      → `.failure()` + stderr "invalid duration"

## Accepted residual gap

The `read_yes_no` I/O shim (real `stdin().read_line`) stays untested; after the
refactor its only logic (`matches!("y"|"yes")`) moves into `decide` and IS tested.
The remaining `read_line` call is a trivial shim — documented, not pretended-covered.

## Acceptance criteria

- [ ] `decide` extracted and unit-tested (full matrix)
- [ ] 4 `assert_cmd` tests added and passing; non-TTY refusal asserts `.failure()`
- [ ] `cargo test` passes; `cargo clippy` clean; `cargo fmt` applied
- [ ] Behavior unchanged (same CLI output/exit codes as 052 shipped)

## Validation criteria

- `cargo test --bin recall` → decide matrix tests pass
- `cargo test --test cli_errors` → 4 new tests pass
- Negative-duration test asserts `.failure()` (replaces the dismissed `exit: 0`)
