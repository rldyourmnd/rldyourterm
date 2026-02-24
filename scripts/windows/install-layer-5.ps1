Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS LAYER 5: AI ORCHESTRATION'
Write-Host 'Claude Code, Gemini CLI, Codex CLI' -ForegroundColor DarkGray

Install-WingetPackage -DisplayName 'Node.js LTS' -CommandName 'node' -PackageIds @('OpenJS.NodeJS.LTS')

if (-not (Test-CommandAvailable -Name npm)) {
    throw 'npm is missing after Node.js installation.'
}

Ensure-NpmGlobalBinInPath

Install-NpmGlobalPackage -CommandName 'claude' -PackageName '@anthropic-ai/claude-code'
Install-NpmGlobalPackage -CommandName 'gemini' -PackageName '@google/gemini-cli'
Install-NpmGlobalPackage -CommandName 'codex' -PackageName '@openai/codex'

Write-Section 'VERIFICATION'

$checks = @(
    @{ Name = 'node'; Args = @('--version') },
    @{ Name = 'npm'; Args = @('--version') },
    @{ Name = 'claude'; Args = @('--version') },
    @{ Name = 'gemini'; Args = @('--version') },
    @{ Name = 'codex'; Args = @('--version') }
)

foreach ($check in $checks) {
    $version = Get-CommandVersion -CommandName $check.Name -VersionArgs $check.Args
    if ($version) {
        Write-Host "[OK] $($check.Name): $version" -ForegroundColor Green
    } else {
        Write-Host "[FAIL] $($check.Name): NOT INSTALLED" -ForegroundColor Red
    }
}

Write-Host ''
Write-Host 'Authentication:' -ForegroundColor Cyan
Write-Host '- claude  -> run claude (browser/device auth)' -ForegroundColor Cyan
Write-Host '- gemini  -> run gemini (OAuth) or set GEMINI_API_KEY' -ForegroundColor Cyan
Write-Host '- codex   -> set OPENAI_API_KEY and run codex' -ForegroundColor Cyan

Write-Host ''
Write-Host '[SUCCESS] Layer 5 complete.' -ForegroundColor Green
