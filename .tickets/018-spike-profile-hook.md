---
id: 18
title: "Spike: profile hook — what should it do with Rust binary?"
status: open
priority: normal
type: spike
blocked_by: [11]
estimate: 20min
---

# Spike: Profile Hook Design

## Question

The Python recall has a `profile-hook.ps1` that runs on every shell open:
1. Checks `~/.recall/last_ingest` staleness (> 4 hours = stale)
2. If stale: runs `recall import` for cwd project + `recall ingest` in background

With the Rust binary (which is much faster to start but slow to embed), what should the hook do?

## Options to evaluate

A. **Same pattern** — check staleness, launch background ingest
B. **Simpler** — just check staleness and warn (let scheduled task handle ingest)
C. **Hybrid** — check staleness, do fast import of cwd .memory/ (no embedding needed for hash-gate check), skip full ingest
D. **Nothing** — rely entirely on scheduled task (30 min interval)

## Considerations

- Rust binary cold start: ~500ms (model load) — too slow to block shell open
- Background launch: `Start-Process -NoNewWindow` works but spawns a visible process
- The 20-minute full ingest would be unacceptable on shell open
- Import with hash-gate is instant when nothing changed

## What to do

1. Time `recall import .memory/ --wing X` when nothing changed (hash-gate skip)
2. Time `recall import .memory/ --wing X` when one file changed
3. Decide: is fast-path import acceptable on shell open, or should we only warn?

## Success criteria

- [ ] Decision documented: what the profile hook should do
- [ ] Timing data for fast-path operations
