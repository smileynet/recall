---
id: "072"
title: "Guard import --force against silent whole-wing wipe"
status: open
blocked_by: []
---

# Guard import --force against silent whole-wing wipe

## Context

`recall import <PATH> --wing W --force` deletes ALL of the wing's imported chunks
(`src/ingest.rs`, force branch: `delete_chunks_by_source_prefix("import:{wing}:")`
+ clears the manifest), then reimports only the files under `<PATH>`. So running
`--force` on a SUBDIRECTORY silently destroys everything else in the wing.

**Incident (2026-08-17, godot-helper):**
`recall import .memory/api-capabilities/ --wing godot_api_reference --force`
wiped 11,409 drawers → 68 (just the capability pages). 60-minute full reimport to recover.

Filed from crew-research ticket 110 (documented the gotcha in the recall skill's
CLI reference; this ticket is the tool-side guard, which is recall's to own).

## What to build

A **coarse guard** — the EASY fix that closes the actual data-loss hole without a
schema change (true "PATH is a subdirectory of the original import root" detection
is HARD because the import root is stored nowhere; only root-stripped relative
paths are persisted).

1. When `--force` would delete a NON-EMPTY wing, print the blast radius (existing
   chunk count for the wing) and require confirmation before deleting.
2. Add a `--yes` flag that bypasses the prompt (distinct from `--force`: `--force`
   = "wipe + reimport" intent; `--yes` = "skip the confirmation"). Prior art
   (terraform/rsync/git/npm) keeps skip-guard and skip-prompt as separate flags.
3. **Thread `--yes` through `sync --force` / `import-all --force`** and any non-TTY
   path — a bare `read` prompt hangs the scheduled RecallIngest task. Non-interactive
   contexts must require `--yes` rather than blocking.
4. Mirror the existing `Forget` command's confirmation pattern (machinery already
   in `src/cli.rs`).

Later enhancement (optional, separate ticket): record `import_root` per source so a
true subdirectory-of-root guard can refuse the exact incident command.

## Acceptance criteria

- [ ] `import --force` on a non-empty wing prompts with the chunk count before deleting
- [ ] `--yes` bypasses the prompt; `sync`/`import-all` force paths pass `--yes` and never hang non-TTY
- [ ] Test covers: force on non-empty wing without --yes refuses/prompts; with --yes proceeds
- [ ] recall skill CLI reference (crew-research) updated if flag surface changes
