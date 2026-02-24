param(
    [Alias('dry-run')][switch]$DryRun,
    [Alias('h')][switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

if ($Help) {
    Write-Host 'Usage: .\scripts\install-windows.ps1 [-DryRun]'
    Write-Host '  -DryRun   Validate the Windows flow without making changes'
    exit 0
}

Write-Section 'WINDOWS INSTALLATION PIPELINE: Foundation + Layers 1..5'

$steps = @(
    'install-foundation.ps1',
    'install-layer-1.ps1',
    'install-layer-2.ps1',
    'install-layer-3.ps1',
    'install-layer-4.ps1',
    'install-layer-5.ps1'
)

foreach ($step in $steps) {
    $path = Join-Path $scriptDir $step
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing script: $path"
    }

    Write-Info "Running $step"
    if ($DryRun) {
        Write-Info "[dry-run] would execute: $path"
    } else {
        & $path
        Write-Success "$step complete"
    }
}

Write-Section 'WINDOWS INSTALLATION COMPLETE'
if ($DryRun) {
    Write-Host '[SUCCESS] Dry-run flow validation PASS' -ForegroundColor Green
    exit 0
}

Write-Host 'Next steps:' -ForegroundColor Green
Write-Host '1. Open a new terminal session (or run: RefreshEnv in terminal that supports it).' -ForegroundColor Green
Write-Host '2. Run health-check: .\scripts\health-check-windows.ps1 -Summary' -ForegroundColor Green
Write-Host '3. Authenticate gh, claude, gemini, codex as needed.' -ForegroundColor Green
Write-Host '4. Optional fish experience: install fish inside WSL and set rldyourterm launcher profile to WSL.' -ForegroundColor Green
