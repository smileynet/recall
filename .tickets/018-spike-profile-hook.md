---
id: 18
title: "Spike: profile hook — what should it do with Rust binary?"
status: done
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

- [x] Decision documented: what the profile hook should do
- [x] Timing data for fast-path operations

## Timing Results (2026-08-01)

| Operation | Time | Notes |
|-----------|------|-------|
| `recall status` (no model) | ~160ms | Baseline: process start + DB open |
| `recall import` (no changes, warm) | ~550ms | Model load dominates even when skipping |
| `recall import` (one file changed) | ~1.0-1.1s | Model load + embed one file's chunks |
| Model cold start | ~1.3s | First run after process cache expires |

### Root cause of 550ms floor

`import_directory()` calls `Embedder::new()` unconditionally (line 175 of ingest.rs) before checking hashes. The embedder loads even when every file is skipped by hash-gate.

If embedder load were deferred until actually needed, the no-change path would be ~160ms (file reads + hash compares only).

## Decision: Option C (Hybrid) with deferred embedder

**Profile hook should:**
1. Check staleness of `~/.recall/last_ingest` (> 6 hours = stale)
2. If stale: run `recall import .memory/ --wing {cwd}` in background (non-blocking)
3. Never block shell open — always `Start-Process -NoNewWindow -WindowStyle Hidden`

**Optimization to unlock this (separate ticket):**
- Defer `Embedder::new()` in `import_directory` until at least one file needs embedding
- This makes the no-change path ~160ms (acceptable for background shell-open task)
- Even without this optimization, 550ms background is fine (user doesn't see it)

**Why not Option D (nothing):**
- Scheduled task runs every 6h but only does ingest (sessions), not import (.memory/)
- After #025 (`recall sync`), this becomes less important — sync covers both
- But hook still provides faster feedback when editing .memory/ docs

**Why not Option A (full ingest on shell open):**
- Full ingest can take minutes — unacceptable even in background (resource usage)
- Scheduled task handles ingest adequately

### Profile hook script (PowerShell)

```powershell
# ~/.recall/profile-hook.ps1
$marker = "$env:USERPROFILE\.recall\last_ingest"
$staleHours = 6

if (Test-Path $marker) {
    $age = (Get-Date) - (Get-Item $marker).LastWriteTime
    if ($age.TotalHours -lt $staleHours) { return }
}

# Find .memory/ in cwd or parent
$dir = Get-Location
while ($dir -and -not (Test-Path "$dir\.memory")) {
    $dir = Split-Path $dir -Parent
}
if (-not $dir) { return }

$wing = (Split-Path $dir -Leaf) -replace '-','_'
Start-Process -FilePath "$env:USERPROFILE\.cargo\bin\recall.exe" `
    -ArgumentList "import",".memory/","--wing",$wing `
    -WorkingDirectory $dir `
    -NoNewWindow -WindowStyle Hidden
```

**Add to PowerShell profile:**
```powershell
. "$env:USERPROFILE\.recall\profile-hook.ps1"
```
