Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS LAYER 2: PRODUCTIVITY'
Write-Host 'fzf, zoxide, atuin, uv, bun, watchexec, glow, btm, hyperfine' -ForegroundColor DarkGray

Install-WingetPackage -DisplayName 'fzf' -CommandName 'fzf' -PackageIds @('junegunn.fzf')
Install-WingetPackage -DisplayName 'zoxide' -CommandName 'zoxide' -PackageIds @('ajeetdsouza.zoxide')
Install-WingetPackage -DisplayName 'atuin' -CommandName 'atuin' -PackageIds @('Atuinsh.Atuin')

Install-WingetPackage -DisplayName 'uv' -CommandName 'uv' -PackageIds @('astral-sh.uv') -Fallback { Install-UvViaOfficialScript }
Install-WingetPackage -DisplayName 'bun' -CommandName 'bun' -PackageIds @('Oven-sh.Bun') -Fallback { Install-BunViaOfficialScript }

# watchexec currently has uneven winget coverage; cargo path is reliable.
Install-CargoBinary -BinaryName 'watchexec' -CrateName 'watchexec-cli'

Install-WingetPackage -DisplayName 'glow' -CommandName 'glow' -PackageIds @('charmbracelet.glow')
Install-WingetPackage -DisplayName 'bottom' -CommandName 'btm' -PackageIds @('Clement.bottom')
Install-WingetPackage -DisplayName 'hyperfine' -CommandName 'hyperfine' -PackageIds @('sharkdp.hyperfine')

Write-Section 'VERIFICATION'

$checks = @(
    @{ Name = 'fzf'; Args = @('--version') },
    @{ Name = 'zoxide'; Args = @('--version') },
    @{ Name = 'atuin'; Args = @('--version') },
    @{ Name = 'uv'; Args = @('--version') },
    @{ Name = 'bun'; Args = @('--version') },
    @{ Name = 'watchexec'; Args = @('--version') },
    @{ Name = 'glow'; Args = @('--version') },
    @{ Name = 'btm'; Args = @('--version') },
    @{ Name = 'hyperfine'; Args = @('--version') }
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
Write-Host '[SUCCESS] Layer 2 complete.' -ForegroundColor Green
