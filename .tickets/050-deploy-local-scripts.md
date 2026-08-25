---
id: "050"
title: "Add deploy-local scripts (Windows + Unix) for single-command local deployment"
status: open
blocked_by: []
priority: medium
estimate: 2h
---

# Add deploy-local scripts (Windows + Unix) for single-command local deployment

## Problem

No codified deployment workflow exists. After building, you have to remember to copy
the binary, verify it works, and check the scheduled task. Agents and humans repeat
these steps manually every time. Ticket #29 documents a workaround; ticket #49
confirms `cargo install` is broken indefinitely.

## Research Summary

Research dispatched 2026-08-25 (8 subagents, all returned). Key findings:

**Windows file locking:** A running .exe cannot be overwritten (`SHARING_VIOLATION`)
but CAN be renamed on NTFS. For recall's timer-triggered task (runs every 30 min,
exits after), the binary is usually unlocked between runs. Strategy: check if running,
wait briefly or rename-swap.

**Unix (POSIX):** `rename(2)` is atomic — `mv new_binary old_path` works even while
the binary is running. No restart needed for timer/oneshot services.

**Best practices (from Rust CLI ecosystem):**
- `--locked` flag for reproducible builds (uses Cargo.lock exactly)
- Backup previous binary before overwriting
- Gate on tests passing before deploy
- Verify after deploy (`--version` + smoke test)
- Idempotent scripts (running twice produces same result)

**Cross-platform scripts:** Keep separate implementations (.ps1 + .sh), share
constants via common file if needed. Test both in CI matrix. Don't force a single
language across platforms.

**Prior art (starship, just, cargo-binstall, UBI):** curl|sh pattern is for
distribution, not local dev deploy. For dev workflows, build+copy with verification
is standard.

**Scheduled task considerations:** No restart needed for binary-only updates on
timer-triggered services. Task Scheduler resolves the exe path at invocation time.
Replacing between runs is inherently safe.

## What to build

Two scripts:
- `scripts/deploy-local.ps1` — Windows (PowerShell)
- `scripts/deploy-local.sh` — macOS/Linux (bash)

---

## Proposed: `scripts/deploy-local.ps1`

```powershell
#!/usr/bin/env pwsh
# deploy-local.ps1 — Build and deploy recall locally (Windows)
#
# Usage: .\scripts\deploy-local.ps1 [-SkipTests] [-Force]
#   -SkipTests  Skip cargo test before deploy
#   -Force      Deploy even if recall process is running (rename-swap)
param(
    [switch]$SkipTests,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path $PSScriptRoot -Parent
$BinDir = if ($env:CARGO_HOME) { "$env:CARGO_HOME\bin" } else { "$env:USERPROFILE\.cargo\bin" }
$Target = "$BinDir\recall.exe"
$Backup = "$Target.prev"

Write-Host "recall deploy-local (Windows)" -ForegroundColor Cyan
Write-Host "  repo: $RepoRoot"
Write-Host "  target: $Target"

# 1. Tests (gate)
if (-not $SkipTests) {
    Write-Host "`nRunning tests..." -ForegroundColor Cyan
    Push-Location $RepoRoot
    try {
        cargo test --lib
        if ($LASTEXITCODE -ne 0) { throw "Tests failed — aborting deploy" }
    } finally { Pop-Location }
}

# 2. Build
Write-Host "`nBuilding release (--locked)..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
} finally { Pop-Location }

# 3. Handle running process
$proc = Get-Process -Name "recall" -ErrorAction SilentlyContinue
if ($proc) {
    if ($Force) {
        Write-Host "`nrecall is running — using rename-swap" -ForegroundColor Yellow
        Remove-Item "$Target.old" -ErrorAction SilentlyContinue
        Rename-Item $Target "$Target.old"
    } else {
        Write-Host "`nrecall is running — waiting up to 120s..." -ForegroundColor Yellow
        $proc | Wait-Process -Timeout 120 -ErrorAction Stop
    }
}

# 4. Backup previous binary
if (Test-Path $Target) {
    Copy-Item $Target $Backup -Force
}

# 5. Deploy
Write-Host "`nDeploying..." -ForegroundColor Cyan
Copy-Item "$RepoRoot\target\release\recall.exe" -Destination $Target -Force

# 6. Verify
$version = & $Target --version
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Verification failed — rolling back" -ForegroundColor Red
    if (Test-Path $Backup) { Copy-Item $Backup $Target -Force }
    exit 1
}
Write-Host "Installed: $version" -ForegroundColor Green

# 7. Smoke test
Write-Host "`nHealth check..." -ForegroundColor Cyan
& $Target health
if ($LASTEXITCODE -ne 0) {
    Write-Host "WARNING: health check returned non-zero (may be stale data)" -ForegroundColor Yellow
}

# 8. Scheduled task status
$task = Get-ScheduledTask -TaskName "RecallIngest" -ErrorAction SilentlyContinue
if ($task) {
    $info = $task | Get-ScheduledTaskInfo
    Write-Host "`nScheduled task: $($task.State), last run: $($info.LastRunTime), result: $($info.LastTaskResult)" -ForegroundColor Green
} else {
    Write-Host "`nWARNING: RecallIngest scheduled task not found" -ForegroundColor Yellow
    Write-Host "  Run with admin to create:" -ForegroundColor Yellow
    Write-Host "  See AGENTS.md ## Deployment for schtasks command" -ForegroundColor Yellow
}

# Cleanup
Remove-Item "$Target.old" -ErrorAction SilentlyContinue
Write-Host "`nDone." -ForegroundColor Green
```

---

## Proposed: `scripts/deploy-local.sh`

```bash
#!/usr/bin/env bash
# deploy-local.sh — Build and deploy recall locally (macOS/Linux)
#
# Usage: ./scripts/deploy-local.sh [--skip-tests] [--force]
set -euo pipefail

SKIP_TESTS=false
FORCE=false
for arg in "$@"; do
    case "$arg" in
        --skip-tests) SKIP_TESTS=true ;;
        --force) FORCE=true ;;
        *) echo "Unknown arg: $arg"; exit 1 ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
TARGET="$BIN_DIR/recall"
BACKUP="$TARGET.prev"

echo "recall deploy-local ($(uname -s))"
echo "  repo: $REPO_ROOT"
echo "  target: $TARGET"

# 1. Tests (gate)
if [[ "$SKIP_TESTS" != "true" ]]; then
    echo -e "\nRunning tests..."
    (cd "$REPO_ROOT" && cargo test --lib)
fi

# 2. Build
echo -e "\nBuilding release (--locked)..."
(cd "$REPO_ROOT" && cargo build --release --locked)

# 3. Backup previous binary
if [[ -f "$TARGET" ]]; then
    cp "$TARGET" "$BACKUP"
fi

# 4. Deploy (atomic on same filesystem via mv)
echo -e "\nDeploying..."
cp "$REPO_ROOT/target/release/recall" "$TARGET.new"
chmod +x "$TARGET.new"
mv "$TARGET.new" "$TARGET"  # atomic rename

# 5. Verify
version=$("$TARGET" --version)
if [[ $? -ne 0 ]]; then
    echo "ERROR: Verification failed — rolling back"
    if [[ -f "$BACKUP" ]]; then mv "$BACKUP" "$TARGET"; fi
    exit 1
fi
echo "Installed: $version"

# 6. Smoke test
echo -e "\nHealth check..."
"$TARGET" health || echo "WARNING: health check returned non-zero (may be stale data)"

# 7. Check scheduled ingestion
echo ""
if [[ "$(uname)" == "Darwin" ]]; then
    plist="$HOME/Library/LaunchAgents/com.recall.ingest.plist"
    if [[ -f "$plist" ]]; then
        loaded=$(launchctl list 2>/dev/null | grep recall || true)
        if [[ -n "$loaded" ]]; then
            echo "LaunchAgent: loaded"
        else
            echo "LaunchAgent: plist exists but not loaded"
            echo "  launchctl load $plist"
        fi
    else
        echo "NOTE: No LaunchAgent configured. To schedule ingestion:"
        echo "  Create $plist with:"
        echo "    ProgramArguments: [\"$TARGET\", \"ingest\"]"
        echo "    StartInterval: 1800  (every 30 min)"
    fi
else
    if systemctl --user is-active recall-ingest.timer &>/dev/null; then
        echo "Systemd timer: active"
        systemctl --user status recall-ingest.timer --no-pager 2>/dev/null | head -3
    elif crontab -l 2>/dev/null | grep -q recall; then
        echo "Cron entry:"
        crontab -l 2>/dev/null | grep recall
    else
        echo "NOTE: No scheduled ingestion found. Options:"
        echo "  Cron:    */30 * * * * $TARGET ingest"
        echo "  Systemd: ~/.config/systemd/user/recall-ingest.{service,timer}"
    fi
fi

echo -e "\nDone."
```

---

## Design decisions (research-informed)

| Decision | Rationale | Source |
|----------|-----------|--------|
| `--locked` flag on build | Ensures Cargo.lock is respected exactly — the ONLY thing keeping builds working given ort incompatibility | Tickets #29, #49; Cargo docs |
| Unit tests only (`--lib`) as gate | Full test suite needs model (~8s), unit tests are <1s and catch regressions | AGENTS.md commands section |
| Backup before overwrite | Enables rollback if new binary fails verification | Rust CLI deploy best practices |
| Windows: check process + rename-swap | Running exe can't be overwritten on NTFS, but can be renamed | SO research, MS docs |
| Unix: cp + mv (atomic) | `mv` on same filesystem is `rename(2)` — atomic, safe even if process is running | POSIX semantics research |
| Rollback on verification failure | If `--version` fails, restore backup automatically | Prior art (starship, UBI) |
| Scheduled task check is informational only | Don't modify the task — just report status | Review: task settings are complex, shouldn't be recreated casually |
| Separate .ps1 and .sh files | Cross-platform script research: separate implementations > polyglot hacks | Cross-platform scripts research |
| `--skip-tests` flag | Allow fast deploy when tests already passed (agent workflow) | Script authoring best practices |

## Out of scope (deliberate)

- **Creating/modifying the scheduled task** — complex enough to remain manual (see AGENTS.md)
- **Version bumping** — separate concern (release-protocol skill)
- **ONNX Runtime DLL** — already handled by load-dynamic; cached at `~/.recall/lib/`
- **Cross-compilation** — deploy to current platform only

## Acceptance criteria

- [ ] `scripts/deploy-local.ps1` exists and deploys successfully on Windows
- [ ] `scripts/deploy-local.sh` exists and deploys successfully on macOS/Linux
- [ ] Both: build with `--locked`, copy, verify version, health check
- [ ] Both: check for and report on scheduled ingestion status
- [ ] Both: fail early on build/test failure (don't copy stale binary)
- [ ] Both: rollback on verification failure
- [ ] Both: handle `--skip-tests` flag
- [ ] Windows: handle running process (wait or rename-swap with `--force`)
- [ ] Unix: atomic replacement via rename
- [ ] AGENTS.md Deployment section updated with `scripts/deploy-local.ps1` reference
