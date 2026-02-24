Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS LAYER 3: GITHUB & GIT'
Write-Host 'gh, lazygit, delta' -ForegroundColor DarkGray

Install-WingetPackage -DisplayName 'GitHub CLI' -CommandName 'gh' -PackageIds @('GitHub.cli')
Install-WingetPackage -DisplayName 'lazygit' -CommandName 'lazygit' -PackageIds @('JesseDuffield.lazygit')
Install-WingetPackage -DisplayName 'delta' -CommandName 'delta' -PackageIds @('dandavison.delta')

Configure-GitDefaults

Write-Section 'VERIFICATION'

$checks = @(
    @{ Name = 'gh'; Args = @('--version') },
    @{ Name = 'lazygit'; Args = @('--version') },
    @{ Name = 'delta'; Args = @('--version') },
    @{ Name = 'git'; Args = @('--version') }
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
Write-Host "[INFO] git core.pager: $(git config --global core.pager 2>$null)" -ForegroundColor Cyan
Write-Host "[INFO] git delta.side-by-side: $(git config --global delta.side-by-side 2>$null)" -ForegroundColor Cyan

Write-Host ''
Write-Host '[SUCCESS] Layer 3 complete.' -ForegroundColor Green
