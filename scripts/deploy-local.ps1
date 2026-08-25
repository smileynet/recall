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
    Write-Host "  See AGENTS.md ## Deployment for setup instructions" -ForegroundColor Yellow
}

# Cleanup
Remove-Item "$Target.old" -ErrorAction SilentlyContinue
Write-Host "`nDone." -ForegroundColor Green
