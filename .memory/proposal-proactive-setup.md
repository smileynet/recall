# Proposal: make import-all + scheduling proactive at setup

Status: proposal, **revised 2026-08-26** (orig 2026-08-19). Synthesis of the original
3 reviews plus a second pass: 3 web-research subagents (systemd user timers, ONNX RSS,
self-install prior art) and 3 review subagents (code, docs, skills) + a timer/config
reconciliation. Raw findings: `.scratch/review-{code,docs,skills}.md` (v1),
`.scratch/review2/{code,docs,skills,timers-config}.md` and `.scratch/research/*.md` (v2).

## ⚠️ Update summary — what changed since 2026-08-19

Verified against code (file:line), the live systemd units, and current docs/skills:

1. **LIVE OOM HAZARD — two enabled timers can overlap.** This session added
   `recall-ingest.timer` (30min, ingest-only, **no MemoryHigh, no timeout**) on top of the
   existing `recall-sync.timer` (6h, ingest+import, `MemoryHigh=45G`, `TimeoutStartSec=3h`).
   **Both are `enabled` + `active` right now.** `Type=oneshot` only blocks a unit from
   starting a second copy of *itself* — it does NOT stop `recall-ingest` and `recall-sync`
   (distinct units) from running together. Their schedules collide ~every 6h; the only
   cross-unit guard is recall's fs2 lock (`~/.recall/recall.lock`). `recall-ingest` is
   **fully redundant** (sync Phase 1 *is* ingest) and is the *unhardened* one. → New **P0-now**
   below. (Evidence: `.scratch/review2/timers-config.md`.)
2. **P3 is partly already done.** The `uv tool install` → `cargo install` fix and the
   staleness-contract reconciliation (`~/.recall/last_ingest` @24h, matches
   `ingest.rs` `write_last_ingest_marker` and `cli.rs` reader) are **already in the skills**.
   Remaining P3 work: the cli-reference **cron block** (still cron, still PAM-blocked) and
   **missing `import-all`/`import`/`health` rows** in the command table. (Evidence:
   `.scratch/review2/skills.md`.)
3. **ONNX RSS has a real lever the v1 proposal missed.** v1 correctly ruled out the thread
   env-vars but concluded nothing controls RSS. Research: the ~24GB is the **CPU memory
   arena (BFCArena)** pre-allocating and holding. Disabling it dropped 6GB→217MB in an
   upstream repro (ORT #11627). ort 2.0-rc SessionBuilder exposes
   `with_config_entry("session.disable_cpu_mem_arena","1")` + `with_memory_pattern(false)`.
   **Blocker:** `fastembed-rs InitOptions` exposes only `execution_providers` — no
   SessionOptions passthrough, so the arena fix needs driving `ort::Session` directly or a
   fastembed patch. → New **P2b** below. (Evidence: `.scratch/research/onnx-memory.md`.)
4. **Build-vs-buy for P0.** The `service-manager` crate (and timer-first `service-install`)
   already abstract systemd-user / launchd / schtasks with `current_exe()`-based install and
   user-level paths — the exact cross-platform detection P0 plans to hand-roll. Weigh against
   recall's prior dependency pain (ort). (Evidence: `.scratch/research/bootstrap-prior-art.md`.)
5. **Version drift.** Skills/docs now reference `recall 0.2.0` in places while AGENTS.md
   still says 0.1.0 — confirm the real version and make docs consistent during P4.

## P0-now — resolve the dual-timer OOM hazard (do before shipping anything)

Independent of the `recall setup` work below, the current machine has a real overlap risk.
Recommended (a one-time, reversible fix — get user confirmation before disabling a unit):
- **Disable `recall-ingest.timer`; keep `recall-sync.timer`** (sync already ingests, is
  memory-hardened, and eliminates the overlap window). If sub-6h freshness is genuinely
  wanted, instead add to `recall-ingest.service`: `Conflicts=recall-sync.service`,
  `MemoryHigh=45G`, `TimeoutStartSec=`, and stagger the calendars — but the simpler path is
  to drop it.
- Also consider adding a systemd-level `Conflicts=` between the two services so the fs2 lock
  is not the *sole* OOM defense.

## The problem this fixes

A fresh machine ends up with recall only half-working:
- Session ingest gets scheduled, but **project `.memory/` import (`import-all`) does not** —
  `recall health` sits at `0/N projects imported` until someone runs it by hand.
- There is **no one-command bootstrap**. Setup is scattered Windows PowerShell snippets
  across tickets #12/#13/#21/#31; every doc is stale for Linux.
- Scheduling guidance says **cron**, which is **PAM-blocked** on the Cloud Desktop.

The mechanism to fix (a) already exists: **`recall sync` = ingest + import-all in one
process, one embedder load** (ticket #25, done; `src/cli.rs:294`). It was built to replace
the ingest-only scheduled task, but that deployment step was deferred — which is exactly the gap.

## What was already done this session (the immediate fix)

- A systemd **user** timer for `recall sync` was created
  (`~/.config/systemd/user/recall-sync.{service,timer}`, every 6h, `Persistent=true`,
  `Linger=yes`, `MemoryHigh=45G`, `TimeoutStartSec=3h`). Every scheduled sync run also imports,
  closing the 0/N gap going forward.
- **Correction (2026-08-26):** the scheduler was NOT cleanly "repointed." A separate
  `recall-ingest.timer` (30min, ingest-only, unhardened) is *also* still enabled+active — see
  the P0-now hazard at the top. The two must be reconciled (disable ingest, or harden+`Conflicts`).
- Verified `$HOME/code` → `/local/home/sabiggin/code` has **46 `.memory/` projects**, so
  `sync`'s Phase 2 finds them all with no root change.

## The memory reality (must inform any bootstrap)

Discovered while deploying: a full ingest holds a **stable ~24GB RSS** on this 32-core box
(ONNX arena high-water mark). Findings, all verified this session:
- It does **not** grow unbounded across files — the per-file loop is bounded.
- **`ORT_INTRA_OP_NUM_THREADS` / `OMP_NUM_THREADS` / `RAYON_NUM_THREADS` are NOT honored**
  by ort 2.0-rc.9 in load-dynamic mode (RSS stayed 24GB with them set). Do not rely on them.
- **NEW (research 2026-08-26): the ~24GB is the CPU memory arena (BFCArena), and it IS
  controllable — just not via fastembed today.** Disabling the arena dropped 6GB→217MB in an
  upstream repro (ORT #11627). ort 2.0-rc SessionBuilder exposes
  `with_config_entry("session.disable_cpu_mem_arena","1")`, `with_memory_pattern(false)`, and
  `arena_extend_strategy=kSameAsRequested`. **Blocker:** `fastembed-rs InitOptions` exposes only
  `execution_providers` — no SessionOptions passthrough — so applying the arena fix means driving
  `ort::Session` directly (fastembed's embed logic is thin) or patching fastembed. Cheap
  no-code experiment worth one run first: `LD_PRELOAD` mimalloc + `MIMALLOC_PURGE_DELAY=0`.
  Full detail + citations: `.scratch/research/onnx-memory.md`. → tracked as **P2b** below.
- A single huge session file (44MB → ~55k chunks) previously blew past 20GB in one
  `model.embed` call. **Fixed** in `embed_batch` (sub-batch 256/call, commit 3860967) so
  peak is flat regardless of file size.
- The real failure mode is **concurrency**: two ~24GB runs = ~48GB → OOM (swapless host).
  Protection: `Type=oneshot` won't start a second run while one is active, and recall's
  fs2 lock (`~/.recall/recall.lock`) backs it up. **Any bootstrap MUST NOT launch an ingest
  while the timer might also fire one.**

## Recommendation (priority order)

### P0 — ship a product `recall setup` subcommand (one-command bootstrap)
Add `Commands::Setup` in `src/cli.rs` (confirmed absent today — enum has no
setup/install/schedule; `.scratch/review2/code.md`). On a fresh machine it should, idempotently:
1. Detect init system. **Linux → write + enable a systemd `--user` timer** calling
   `recall sync` (cron is PAM-blocked — do not emit cron on Linux). macOS → launchd plist.
   Windows → `schtasks` (matches the note at `cli.rs:295`).
   **Build-vs-buy:** the `service-manager` crate abstracts systemd-user/launchd/sc.exe behind
   one trait with auto-detection + user-level install; `service-install` is timer-first. Either
   removes the hand-rolled 3-template branch — weigh against recall's prior ort dep-pain. If
   hand-rolling, keep templates tiny and content-compare before writing.
2. Use `std::env::current_exe()` (canonicalized to defeat symlinks) for `ExecStart` so the unit
   survives `cargo install` moves. current_exe() can shift on upgrade → have `update`/deploy
   re-run `setup`, or make setup safely re-runnable.
3. Run **one** initial `recall sync` (populates sessions + all `.memory/` projects) — but
   only if no run is already active (respect the fs2 lock; never double-launch given the
   24GB footprint).
4. Flags: `--dry-run` (print units + target paths, write nothing), `--uninstall` (symmetric,
   same install context), `--interval 6h`.
5. Verify: end by printing `recall health` and asserting sessions ingested AND projects
   imported > 0 (the acceptance gate).

**Researched unit shape (`.scratch/research/systemd-timers.md`):** prefer anchored
`OnCalendar=*-*-* 00/6:00:00` + `Persistent=true` over `OnUnitActiveSec` — the latter drifts
(measures from last activation) and `Persistent=` only catches up missed runs with `OnCalendar`.
(The currently deployed timers use `OnUnitActiveSec` — switch during this work.) On the service:
`MemoryHigh` (soft throttle) + `MemoryMax` (hard, OOM-kills the *cgroup* not the host),
`RuntimeMaxSec`/`TimeoutStartSec` ≈ 5h30m so a hung run can't overlap the next, optional
`Conflicts=` the ingest unit. `loginctl enable-linger` is load-bearing (already set) — setup
should verify it and warn if absent.

This makes both legs proactive from a single command and encodes the systemd/lock knowledge
in the product instead of tribal shell steps.

### P1 — de-hardcode import roots (promote ticket #37)
Root discovery is **triplicated** (`cmd_import_all` cli.rs:278, `cmd_sync` Phase 2 cli.rs:363,
`discover_project_coverage` cli.rs:662 — line numbers refreshed 2026-08-26; the dead `D:/code`
literal persists at cli.rs:296/368/673) and hardcodes `$HOME/code` + `D:/code`.
- Collapse into one `fn discover_memory_projects() -> Vec<(mem_path, wing)>`.
- Read `import.roots` / `import.exclude` from `~/.recall/config.toml` (#37 design), default
  `[$HOME/code]`, drop literal `D:/code`. Optional `sync.import_interval` so import can run
  every Nth sync if the 6h import cost is unwanted.

### P2 — add a `last_import` marker + surface in health
Mirror `write_last_ingest_marker` (ingest.rs:78) at the end of sync Phase 2; show
"Last import: Nh ago" in `health`. Today import staleness is invisible (only coverage count).
Confirmed still not implemented (`.scratch/review2/code.md`); the 24h staleness check also
covers *sessions only* because the marker is written on ingest, not import.

### P2b — bound ONNX RSS by disabling the CPU memory arena (removes the OOM root cause)
The dual-timer OOM (P0-now) is a symptom; the root cause is the ~24GB arena. Landing the arena
fix shrinks each run's footprint and makes overlap far less dangerous. Path (see the memory
section + `.scratch/research/onnx-memory.md`): drive `ort::Session` directly for the embed path
(or patch fastembed) and set `session.disable_cpu_mem_arena=1` + `with_memory_pattern(false)`.
First, cheaply confirm the arena hypothesis with an `LD_PRELOAD` mimalloc run — no code change.
Verify against the pinned ort rev (methods confirmed in rc.13; confirm in rc.9). Self-contained
but higher-risk than P1/P2 (touches the embed hot path) — gate on a measured before/after RSS.

### P3 — fix skills + steering (partly already done — 2026-08-26)
- **DONE:** the `uv tool install` → `cargo install` fix and the staleness-contract
  reconciliation (`~/.recall/last_ingest` @24h, now consistent across skill + steering, and
  matches `ingest.rs`/`cli.rs`). Verified in `.scratch/review2/skills.md` — do not redo.
- **REMAINING:** `~/.kiro/skills/recall/references/cli-reference.md` still shows a **cron block**
  for Linux (PAM-blocked here) — replace with the systemd `--user` timer running `recall sync`;
  and the command table still omits **`import-all` / `import` / `health`** (stops at `status`)
  — add those rows. (`sync` is already present.)
- Residual: the 24h staleness check covers sessions only (see P2) — either write the marker on
  import too, or note the limitation in the steering.

### P4 — reconcile stale docs (Linux migration)
AGENTS.md/CONTEXT.md/HANDOFF describe a **Windows** deploy (`.exe`, Task Scheduler, `D:/code`,
38/38 coverage). Update to systemd/Linux paths, `recall sync` cadence, and mark the Windows
Task Scheduler snippets in #12/#13/#18/#31 as legacy/Windows-only.
Specifics found (`.scratch/review2/docs.md`), all verified stale:
- AGENTS.md "Deployment" is entirely Windows (`recall.exe`, `RecallIngest` Task Scheduler
  "every 30 min", `onnxruntime.dll`) and its CLI list shows `import-all … from D:/code`.
- **`recall sync` appears in NO doc** (AGENTS.md CLI list, README usage, Deployment) — the
  single biggest doc gap, since it's the live scheduled action.
- **Internal contradiction:** AGENTS.md says `cargo install --path .` is broken (#049) while
  README "Installation" leads with exactly that command — resolve which is true.
- **Version drift:** skills/docs reference `recall 0.2.0` in places, AGENTS.md says 0.1.0 —
  confirm the real version and make consistent.
- Replace hardcoded coverage counts (AGENTS.md `~44K chunks, 69 wings, 47/47`) with a pointer
  to `recall status`/`health`; `.memory/CONTEXT.md` Environment/Gotchas block is fully Windows
  and needs a cron-PAM-blocked gotcha added. `deploy-local.sh` still prints a cron suggestion
  (line ~85) despite cron being unusable here.

## Effort / sequencing
- **P0-now (dual-timer OOM) is do-first** — it's a live hazard on the current machine, one
  reversible `systemctl --user disable` (get confirmation). No code.
- P0 (`recall setup`) is the high-value build item (~half a day, less if using
  `service-manager`: one subcommand + unit templates + tests).
- P1/P2 are small, self-contained follow-ups that also improve maintainability.
- **P2b (ONNX arena)** removes the OOM *root cause* but touches the embed hot path — medium
  risk; do the LD_PRELOAD probe first, gate the code change on measured before/after RSS.
- P3/P4 are doc/skill edits, no code risk — and **P3 is ~half done already** (see above).
- The scheduling change already deployed closes the reported 0/N gap today, BUT left the
  dual-timer hazard (P0-now); P0 makes the whole setup reproducible on the next machine.

## Reusable artifacts that already exist (no new features needed for the core)
- `recall sync` (ingest+import in one process, one embedder load) — the whole a+b mechanism
  (#25; `Commands::Sync` cli.rs:93, `cmd_sync` cli.rs:332, embedder loaded once cli.rs:348).
- `recall import-all` standalone (cli.rs:278).
- `guard.rs` `ProcessGuard` fs2 lock (`recall-process.lock`) + `install_timeout` watchdog —
  the single-instance primitive `setup` should rely on (note: the ingest path takes a *second*
  lock `~/.recall/recall.lock`, ingest.rs:143 — redundant, worth collapsing).
- `service-manager` / `service-install` crates — cross-platform install if not hand-rolling P0.
- #18 profile-hook *decision* (hybrid staleness check + background non-blocking import) —
  logic ports to a `~/.recall/profile-hook.sh` for bash if faster-than-6h feedback is wanted;
  lower priority once the timer covers import.
- #37 config-options design — the right home for `import.roots`.
