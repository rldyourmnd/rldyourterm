Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS LAYER 1: FILE OPERATIONS'
Write-Host 'bat, fd, rg, sd, jq, yq, eza' -ForegroundColor DarkGray

Install-WingetPackage -DisplayName 'bat' -CommandName 'bat' -PackageIds @('sharkdp.bat')
Install-WingetPackage -DisplayName 'fd' -CommandName 'fd' -PackageIds @('sharkdp.fd')
Install-WingetPackage -DisplayName 'ripgrep' -CommandName 'rg' -PackageIds @('BurntSushi.ripgrep.MSVC')
Install-WingetPackage -DisplayName 'sd' -CommandName 'sd' -PackageIds @('chmln.sd')
Install-WingetPackage -DisplayName 'jq' -CommandName 'jq' -PackageIds @('jqlang.jq')
Install-WingetPackage -DisplayName 'yq' -CommandName 'yq' -PackageIds @('MikeFarah.yq')
Install-WingetPackage -DisplayName 'eza' -CommandName 'eza' -PackageIds @('eza-community.eza')

Write-Section 'VERIFICATION'

$checks = @(
    @{ Name = 'bat'; Args = @('--version') },
    @{ Name = 'fd'; Args = @('--version') },
    @{ Name = 'rg'; Args = @('--version') },
    @{ Name = 'sd'; Args = @('--version') },
    @{ Name = 'jq'; Args = @('--version') },
    @{ Name = 'yq'; Args = @('--version') },
    @{ Name = 'eza'; Args = @('--version') }
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
Write-Host '[SUCCESS] Layer 1 complete.' -ForegroundColor Green
