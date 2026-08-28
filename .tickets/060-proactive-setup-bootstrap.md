---
id: "060"
title: "Add recall setup subcommand — one-command proactive bootstrap (systemd/launchd/schtasks + sync)"
status: open
blocked_by: ["37"]
priority: high
---

# Add recall setup subcommand — one-command proactive bootstrap (systemd/launchd/schtasks + sync)

## Problem

A fresh machine ends up with recall only half-working: session `ingest` gets scheduled but
project `.memory/` `import-all` does not, so `recall health` sits at `0/N projects imported`
until run by hand. There is no one-command bootstrap — setup is scattered Windows PowerShell
snippets across tickets #12/#13/#21/#31, and scheduling guidance still says cron (PAM-blocked
on the Cloud Desktop). The `recall sync` mechanism (ingest + import-all, one embedder load,
#25 done) exists but was never wired into a product-level installer.

Source: `.memory/proposal-proactive-setup.md` (revised 2026-08-28), backed by 2 research/review
passes. Raw findings in `.scratch/research/*.md` and `.scratch/review2/*.md`.

## Research summary (2026-08-26/28, 7 subagents)

- **systemd user timers** (`.scratch/research/systemd-timers.md`): prefer anchored
  `OnCalendar=*-*-* 00/6:00:00` + `Persistent=true` over drifting `OnUnitActiveSec`; cap with
  `MemoryHigh`/`MemoryMax` (OOM-kills the cgroup, not the host) + `RuntimeMaxSec`≈5h30m;
  `loginctl enable-linger` is the load-bearing prerequisite; systemd's single-active-instance
  guarantee only covers a unit vs itself — cross-unit needs `Conflicts=` or a lock.
- **ONNX RSS** (`.scratch/research/onnx-memory.md`): the ~24GB is the CPU memory arena
  (BFCArena), controllable via `session.disable_cpu_mem_arena` + `with_memory_pattern(false)`
  in ort 2.0-rc — but `fastembed-rs InitOptions` exposes only `execution_providers`, so the
  fix needs driving `ort::Session` directly. Tracked as its own sub-item (P2b).
- **bootstrap prior art** (`.scratch/research/bootstrap-prior-art.md`): `service-manager` /
  `service-install` crates abstract systemd-user/launchd/schtasks behind one trait with
  `current_exe()`-based install; weigh against recall's prior ort dep-pain vs hand-rolling
  three tiny templates.
- **code review** (`.scratch/review2/code.md`): `Commands::Setup` absent today; `Sync` present
  and shares one embedder (cli.rs:93/332/348); import-root discovery triplicated + hardcoded
  `$HOME/code`+`D:/code` (cli.rs:278/363/662); no `last_import` marker; single-instance via
  `guard.rs` fs2 lock (+ a redundant second lock in the ingest path, ingest.rs:143).

## What to build

`Commands::Setup` in `src/cli.rs`, idempotent:
1. Detect init system → Linux: systemd `--user` timer running `recall sync` (never emit cron);
   macOS: launchd plist; Windows: schtasks. Consider the `service-manager` crate vs hand-rolled
   templates (content-compare before writing).
2. `std::env::current_exe()` (canonicalized) for `ExecStart` so units survive `cargo install`
   moves; re-runnable so `update`/deploy can refresh.
3. Run one initial `recall sync` only if no run is active (respect the fs2 lock — never
   double-launch given the ~24GB footprint).
4. Flags: `--dry-run` (print units + paths, write nothing), `--uninstall` (symmetric),
   `--interval 6h`.
5. Verify: print `recall health`, assert sessions ingested AND projects imported > 0.

Unit shape: anchored `OnCalendar` + `Persistent=true`, `MemoryHigh`/`MemoryMax`,
`RuntimeMaxSec`≈5h30m, verify `enable-linger`.

## Related follow-ups (separate tickets, not blockers)

- **P0-now (done in this session):** disabled redundant `recall-ingest.timer` (30min,
  unhardened) that could overlap `recall-sync.timer` → OOM. See Resolution.
- **P1 / #37:** de-hardcode + de-triplicate import roots into `discover_memory_projects()`
  reading `import.roots` from config. (This ticket is blocked_by #37.)
- **P2:** `last_import` marker + surface in `health`.
- **P2b:** disable ONNX CPU arena to bound RSS (touches embed hot path; LD_PRELOAD mimalloc
  probe first) — file as its own ticket.
- **P3/P4:** skills + docs (P3 ~half done: uv→cargo and staleness contract already fixed;
  remaining = cron block + missing command-table rows; docs still Windows-only, `recall sync`
  undocumented, cargo-install contradiction, 0.1.0 vs 0.2.0 drift).

## Acceptance criteria

- [ ] `recall setup` exists; on Linux writes + enables a systemd `--user` timer running `recall sync`
- [ ] `ExecStart` uses `current_exe()`; unit survives a `cargo install`/binary move
- [ ] `--dry-run` writes nothing and prints the rendered unit + target path
- [ ] `--uninstall` cleanly removes what `setup` installed (symmetric)
- [ ] Initial sync respects the fs2 lock (no launch if a run is active)
- [ ] Ends by printing `recall health` and asserts sessions + projects imported > 0
- [ ] systemd unit includes memory caps + `RuntimeMaxSec` and verifies `enable-linger`
- [ ] Does NOT emit cron on Linux
- [ ] Tests cover `--dry-run` output and idempotent re-run

## Resolution

**P0-now hazard mitigated (2026-08-28).** Disabled the redundant `recall-ingest.timer`
(30min, ingest-only, unhardened) that could overlap the hardened `recall-sync.timer` (6h,
ingest+import, `MemoryHigh=45G`, `TimeoutStartSec=3h`) → risk of two ~24GB runs = OOM on the
swapless host. `recall-sync` already does ingest as Phase 1, so ingest was fully redundant.

Command: `systemctl --user disable --now recall-ingest.timer`. Verified:
`recall-ingest.timer` disabled+inactive; `recall-sync.timer` enabled+active and the sole
scheduler (`list-timers` shows 1 timer, next run scheduled); no run in progress. Unit files
left on disk — reversible via `systemctl --user enable --now recall-ingest.timer`.

The main deliverable (`recall setup` subcommand) remains **open** — this ticket tracks it.
Blocked by #37 (config-driven import roots). Do not close until the setup subcommand + its
acceptance criteria are met.
