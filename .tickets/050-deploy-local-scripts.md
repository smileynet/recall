---
id: "050"
title: "Add deploy-local scripts (Windows + Unix) for single-command local deployment"
status: open
blocked_by: []
---

# Add deploy-local scripts (Windows + Unix) for single-command local deployment

## Problem

No codified deployment workflow exists. After building, you have to remember to copy
the binary, verify it works, and check the scheduled task. Agents and humans repeat
these steps manually every time.

## What to build

Two scripts that handle the full local deployment:
- `scripts/deploy-local.ps1` — Windows (PowerShell)
- `scripts/deploy-local.sh` — macOS/Linux (bash)

Both do: build → copy → verify → smoke test.

---

## Proposed: `scripts/deploy-local.ps1`

```powershell
#!/usr/bin/env pwsh
# deploy-local.ps1 — Build and deploy recall locally (Windows)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path $PSScriptRoot -Parent
$BinDir = if ($env:CARGO_HOME) { "$env:CARGO_HOME\bin" } else { "$env:USERPROFILE\.cargo\bin" }
$Target = "$BinDir\recall.exe"

Write-Host "Building release..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
} finally { Pop-Location }

# Copy binary
Write-Host "Deploying to $Target" -ForegroundColor Cyan
Copy-Item "$RepoRoot\target\release\recall.exe" -Destination $Target -Force

# Verify
$version = & $Target --version
if ($LASTEXITCODE -ne 0) { throw "Binary verification failed" }
Write-Host "Installed: $version" -ForegroundColor Green

# Smoke test
Write-Host "Running health check..." -ForegroundColor Cyan
& $Target health
if ($LASTEXITCODE -ne 0) {
    Write-Host "WARNING: health check returned non-zero" -ForegroundColor Yellow
}

# Check scheduled task
$task = Get-ScheduledTask -TaskName "RecallIngest" -ErrorAction SilentlyContinue
if ($task) {
    $info = $task | Get-ScheduledTaskInfo
    Write-Host "Scheduled task: $($task.State), last run: $($info.LastRunTime)" -ForegroundColor Green
} else {
    Write-Host "WARNING: RecallIngest scheduled task not found" -ForegroundColor Yellow
}

Write-Host "`nDone." -ForegroundColor Green
```

---

## Proposed: `scripts/deploy-local.sh`

```bash
#!/usr/bin/env bash
# deploy-local.sh — Build and deploy recall locally (macOS/Linux)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
TARGET="$BIN_DIR/recall"

echo "Building release..."
(cd "$REPO_ROOT" && cargo build --release)

# Copy binary
echo "Deploying to $TARGET"
cp "$REPO_ROOT/target/release/recall" "$TARGET"
chmod +x "$TARGET"

# Verify
version=$("$TARGET" --version)
echo "Installed: $version"

# Smoke test
echo "Running health check..."
"$TARGET" health || echo "WARNING: health check returned non-zero"

# Check for cron/systemd/launchd schedule
if [[ "$(uname)" == "Darwin" ]]; then
    # macOS: check launchd
    plist="$HOME/Library/LaunchAgents/com.recall.ingest.plist"
    if [[ -f "$plist" ]]; then
        echo "LaunchAgent: $(launchctl list | grep recall || echo 'not loaded')"
    else
        echo "NOTE: No LaunchAgent found. To schedule ingestion:"
        echo "  Create $plist with ProgramArguments = [\"$TARGET\", \"ingest\"]"
        echo "  StartInterval = 1800 (every 30 min)"
    fi
else
    # Linux: check systemd timer or cron
    if systemctl --user is-active recall-ingest.timer &>/dev/null; then
        echo "Systemd timer: active"
        systemctl --user status recall-ingest.timer --no-pager | head -5
    elif crontab -l 2>/dev/null | grep -q recall; then
        echo "Cron entry found:"
        crontab -l | grep recall
    else
        echo "NOTE: No scheduled ingestion found. Options:"
        echo "  Systemd: create ~/.config/systemd/user/recall-ingest.{service,timer}"
        echo "  Cron: */30 * * * * $TARGET ingest"
    fi
fi

echo "Done."
```

---

## Design notes

- Scripts are idempotent — safe to run repeatedly
- No `cargo install` (broken per ticket #049) — uses build + copy
- Binary path respects `CARGO_HOME` override (for relocated dev envs)
- Smoke test is advisory (non-zero doesn't fail the script) since health
  may report warnings for stale data without that being a deploy failure
- Scheduled task check is informational only — doesn't modify the task
- macOS/Linux script detects the scheduling mechanism and provides setup
  guidance if none exists

## Acceptance criteria

- [ ] `scripts/deploy-local.ps1` exists and deploys successfully on Windows
- [ ] `scripts/deploy-local.sh` exists and deploys successfully on macOS/Linux
- [ ] Both scripts: build, copy, verify version, run health check
- [ ] Both scripts: check for and report on scheduled ingestion
- [ ] Scripts fail early on build failure (don't copy stale binary)
- [ ] AGENTS.md updated with deploy command reference
- [ ] README updated with deployment section
