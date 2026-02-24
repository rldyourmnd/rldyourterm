Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS LAYER 4: CODE INTELLIGENCE'
Write-Host 'grepai, ast-grep, probe, semgrep, ctags, tokei' -ForegroundColor DarkGray

Install-WingetPackage -DisplayName 'Universal Ctags' -CommandName 'ctags' -PackageIds @('UniversalCtags.Ctags')
Install-WingetPackage -DisplayName 'tokei' -CommandName 'tokei' -PackageIds @('XAMPPRocky.Tokei')
if (Test-CommandAvailable -Name 'sg' -or Test-CommandAvailable -Name 'ast-grep') {
    Write-Info 'ast-grep already installed'
} else {
    Install-WingetPackage -DisplayName 'ast-grep' -CommandName 'sg' -PackageIds @('ast-grep.ast-grep')
}

Install-CargoBinary -BinaryName 'probe' -CrateName 'probe-code'

if (-not (Test-CommandAvailable -Name uv)) {
    Install-UvViaOfficialScript
}
Install-UvTool -CommandName 'semgrep' -PackageName 'semgrep'

Install-GrepaiWindowsBinary

Write-Section 'VERIFICATION'

$checks = @(
    @{ Name = 'ctags'; Args = @('--version') },
    @{ Name = 'tokei'; Args = @('--version') },
    @{ Name = 'probe'; Args = @('--version') },
    @{ Name = 'semgrep'; Args = @('--version') },
    @{ Name = 'grepai'; Args = @('version') }
)

foreach ($check in $checks) {
    $version = Get-CommandVersion -CommandName $check.Name -VersionArgs $check.Args
    if ($version) {
        Write-Host "[OK] $($check.Name): $version" -ForegroundColor Green
    } else {
        Write-Host "[FAIL] $($check.Name): NOT INSTALLED" -ForegroundColor Red
    }
}

if (Test-CommandAvailable -Name 'sg') {
    $version = Get-CommandVersion -CommandName 'sg' -VersionArgs @('--version')
    Write-Host "[OK] sg: $version" -ForegroundColor Green
} elseif (Test-CommandAvailable -Name 'ast-grep') {
    $version = Get-CommandVersion -CommandName 'ast-grep' -VersionArgs @('--version')
    Write-Host "[OK] ast-grep: $version" -ForegroundColor Green
} else {
    Write-Host "[FAIL] ast-grep/sg: NOT INSTALLED" -ForegroundColor Red
}

Write-Host ''
Write-Host '[SUCCESS] Layer 4 complete.' -ForegroundColor Green
