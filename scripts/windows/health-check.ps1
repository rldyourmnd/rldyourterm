param(
    [Alias('summary')][switch]$Summary,
    [Alias('strict')][switch]$Strict,
    [Alias('h')][switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

if ($Help) {
    Write-Host 'Usage: .\scripts\health-check-windows.ps1 [-Summary] [-Strict]'
    Write-Host '  -Summary  Print only counters and final status'
    Write-Host '  -Strict   Fail on local-vs-repo config drift'
    exit 0
}

$verboseMode = -not $Summary
$passed = 0
$warnings = 0
$failures = 0

function Add-Pass {
    param([string]$Message)
    $script:passed++
    if ($verboseMode) {
        Write-Host "[OK] $Message" -ForegroundColor Green
    }
}

function Add-Warn {
    param([string]$Message)
    $script:warnings++
    if ($verboseMode) {
        Write-Host "[WARN] $Message" -ForegroundColor Yellow
    }
}

function Add-Fail {
    param([string]$Message)
    $script:failures++
    if ($verboseMode) {
        Write-Host "[FAIL] $Message" -ForegroundColor Red
    }
}

function Check-Command {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$VersionArgs,
        [bool]$Optional = $false
    )

    if (Test-CommandAvailable -Name $Name) {
        $version = Get-CommandVersion -CommandName $Name -VersionArgs $VersionArgs
        Add-Pass ("{0}: {1}" -f $Name, $version)
        return
    }

    if ($Optional) {
        Add-Warn "$Name missing (optional)"
    } else {
        Add-Fail "$Name missing"
    }
}

function Check-FileParity {
    param(
        [Parameter(Mandatory = $true)][string]$LocalPath,
        [Parameter(Mandatory = $true)][string]$RepoPath,
        [Parameter(Mandatory = $true)][string]$Label
    )

    if (-not (Test-Path -LiteralPath $RepoPath)) {
        Add-Fail "$Label repo file missing: $RepoPath"
        return
    }

    if (-not (Test-Path -LiteralPath $LocalPath)) {
        if ($Strict) {
            Add-Fail "$Label missing locally: $LocalPath"
        } else {
            Add-Warn "$Label missing locally: $LocalPath"
        }
        return
    }

    $repoHash = (Get-FileHash -Path $RepoPath).Hash
    $localHash = (Get-FileHash -Path $LocalPath).Hash

    if ($repoHash -eq $localHash) {
        Add-Pass "$Label parity OK"
    } else {
        if ($Strict) {
            Add-Fail "$Label parity mismatch"
        } else {
            Add-Warn "$Label parity mismatch"
        }
    }
}

function Check-ProfileLine {
    param(
        [Parameter(Mandatory = $true)][string]$Line,
        [Parameter(Mandatory = $true)][string]$Label
    )

    if (-not (Test-Path -LiteralPath $PROFILE)) {
        if ($Strict) {
            Add-Fail "PowerShell profile missing: $PROFILE"
        } else {
            Add-Warn "PowerShell profile missing: $PROFILE"
        }
        return
    }

    $content = Get-Content -Path $PROFILE -Raw -ErrorAction SilentlyContinue
    if ($content -match [regex]::Escape($Line)) {
        Add-Pass "$Label in profile"
    } else {
        if ($Strict) {
            Add-Fail "$Label missing in profile"
        } else {
            Add-Warn "$Label missing in profile"
        }
    }
}

if ($verboseMode) {
    Write-Section 'WINDOWS HEALTH CHECK'
    Write-Host (Get-Date -Format 'yyyy-MM-ddTHH:mm:ssK') -ForegroundColor DarkCyan
}

# Parse all PowerShell scripts in repository scripts directory.
$allPsScripts = Get-ChildItem -Path (Join-Path $script:ProjectRoot 'scripts') -Filter '*.ps1' -Recurse | Sort-Object FullName
foreach ($scriptFile in $allPsScripts) {
    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile($scriptFile.FullName, [ref]$tokens, [ref]$errors) | Out-Null

    if ($errors.Count -eq 0) {
        Add-Pass "Parse: $($scriptFile.FullName)"
    } else {
        Add-Fail "Parse failed: $($scriptFile.FullName)"
    }
}

# Foundation
Check-Command -Name 'winget' -VersionArgs @('--version')
if (Test-CommandAvailable -Name 'rldyourterm') {
  Check-Command -Name 'rldyourterm' -VersionArgs @('--version')
} else {
  Add-Fail 'rldyourterm command missing'
}
Check-Command -Name 'pwsh' -VersionArgs @('--version')
Check-Command -Name 'starship' -VersionArgs @('--version')
Check-Command -Name 'git' -VersionArgs @('--version')

# Layer 1
Check-Command -Name 'bat' -VersionArgs @('--version')
Check-Command -Name 'fd' -VersionArgs @('--version')
Check-Command -Name 'rg' -VersionArgs @('--version')
Check-Command -Name 'sd' -VersionArgs @('--version')
Check-Command -Name 'jq' -VersionArgs @('--version')
Check-Command -Name 'yq' -VersionArgs @('--version')
Check-Command -Name 'eza' -VersionArgs @('--version')

# Layer 2
Check-Command -Name 'fzf' -VersionArgs @('--version')
Check-Command -Name 'zoxide' -VersionArgs @('--version')
Check-Command -Name 'atuin' -VersionArgs @('--version')
Check-Command -Name 'uv' -VersionArgs @('--version')
Check-Command -Name 'bun' -VersionArgs @('--version')
Check-Command -Name 'watchexec' -VersionArgs @('--version')
Check-Command -Name 'glow' -VersionArgs @('--version')
Check-Command -Name 'btm' -VersionArgs @('--version')
Check-Command -Name 'hyperfine' -VersionArgs @('--version')

# Layer 3
Check-Command -Name 'gh' -VersionArgs @('--version')
Check-Command -Name 'lazygit' -VersionArgs @('--version')
Check-Command -Name 'delta' -VersionArgs @('--version')

# Layer 4
Check-Command -Name 'ctags' -VersionArgs @('--version')
Check-Command -Name 'tokei' -VersionArgs @('--version')
if (Test-CommandAvailable -Name 'sg') {
    Check-Command -Name 'sg' -VersionArgs @('--version')
} elseif (Test-CommandAvailable -Name 'ast-grep') {
    Check-Command -Name 'ast-grep' -VersionArgs @('--version')
} else {
    Add-Fail 'ast-grep/sg missing'
}
Check-Command -Name 'probe' -VersionArgs @('--version')
Check-Command -Name 'semgrep' -VersionArgs @('--version')
Check-Command -Name 'grepai' -VersionArgs @('version')

# Layer 5
Check-Command -Name 'node' -VersionArgs @('--version')
Check-Command -Name 'npm' -VersionArgs @('--version')
Check-Command -Name 'claude' -VersionArgs @('--version')
Check-Command -Name 'gemini' -VersionArgs @('--version')
Check-Command -Name 'codex' -VersionArgs @('--version')

# Config parity and profile checks
Check-FileParity -LocalPath (Join-Path $HOME '.rldyourterm.lua') -RepoPath (Join-Path $script:ProjectRoot 'configs\rldyourterm\rldyourterm.lua') -Label 'rldyourterm config'

Check-FileParity -LocalPath (Join-Path $HOME '.config\starship.toml') -RepoPath (Join-Path $script:ProjectRoot 'configs\starship\starship.toml') -Label 'Starship config'

Check-ProfileLine -Line 'Invoke-Expression (&starship init powershell)' -Label 'Starship init'
Check-ProfileLine -Line 'Invoke-Expression (& { (zoxide init powershell | Out-String) })' -Label 'zoxide init'

$localBin = Join-Path $HOME '.local\bin'
if (Test-PathInEnvPath -PathEntry $localBin -PathValue $env:Path) {
    Add-Pass "PATH contains $localBin"
} else {
    if ($Strict) {
        Add-Fail "PATH missing $localBin"
    } else {
        Add-Warn "PATH missing $localBin"
    }
}

if (Test-WSLAvailable) {
    Add-Pass 'WSL command available'
} else {
    Add-Warn 'WSL not available (fish via WSL is optional)'
}

Write-Host ''
Write-Host '============================================================' -ForegroundColor DarkCyan
Write-Host '  Health Check Summary' -ForegroundColor DarkCyan
Write-Host "  Passed:   $passed" -ForegroundColor DarkCyan
Write-Host "  Warnings: $warnings" -ForegroundColor DarkCyan
Write-Host "  Failures: $failures" -ForegroundColor DarkCyan
Write-Host '============================================================' -ForegroundColor DarkCyan

if ($failures -gt 0) {
    Write-Host 'Health status: FAIL' -ForegroundColor Red
    exit 1
}

if ($Strict -and $warnings -gt 0) {
    Write-Host 'Health status: FAIL (strict mode treats warnings as failures)' -ForegroundColor Red
    exit 1
}

Write-Host 'Health status: PASS' -ForegroundColor Green
