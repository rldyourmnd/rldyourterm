Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir 'common.ps1')

Assert-Windows

Write-Section 'WINDOWS FOUNDATION: rldyourterm + PowerShell + Starship + Git'

Assert-WingetAvailable
Ensure-ExecutionPolicyRemoteSigned
Ensure-LocalBinInPath

Install-WingetPackage -DisplayName 'rldyourterm' -CommandName 'rldyourterm' -PackageIds @('rldyourterm.rldyourterm')
Install-WingetPackage -DisplayName 'PowerShell 7' -CommandName 'pwsh' -PackageIds @('Microsoft.PowerShell')
Install-WingetPackage -DisplayName 'Starship' -CommandName 'starship' -PackageIds @('Starship.Starship')
Install-WingetPackage -DisplayName 'Git' -CommandName 'git' -PackageIds @('Git.Git')

# Optional Nerd Font for better glyph support in terminal and prompt.
Install-WingetPackageById -PackageId 'DEVCOM.JetBrainsMonoNerdFont' -Required $false

$rldyourtermRepoConfig = Join-Path $script:ProjectRoot 'configs\rldyourterm\rldyourterm.lua'
$starshipRepoConfig = Join-Path $script:ProjectRoot 'configs\starship\starship.toml'
$starshipProfilesRepo = Join-Path $script:ProjectRoot 'configs\starship\profiles'

$rldyourtermUserConfig = Join-Path $HOME '.rldyourterm.lua'
$rldyourtermConfigDir = Join-Path $HOME '.config\rldyourterm'
$starshipConfigDir = Join-Path $HOME '.config\starship'
$starshipProfilesDir = Join-Path $starshipConfigDir 'profiles'
$starshipUserConfig = Join-Path $HOME '.config\starship.toml'

New-DirectoryIfMissing -Path $rldyourtermConfigDir
New-DirectoryIfMissing -Path $starshipConfigDir
New-DirectoryIfMissing -Path $starshipProfilesDir

Copy-FileWithBackup -Source $rldyourtermRepoConfig -Destination $rldyourtermUserConfig
Copy-FileWithBackup -Source $rldyourtermRepoConfig -Destination (Join-Path $rldyourtermConfigDir 'rldyourterm.lua')
Copy-FileWithBackup -Source $starshipRepoConfig -Destination $starshipUserConfig

Get-ChildItem -Path $starshipProfilesRepo -Filter '*.toml' | ForEach-Object {
    Copy-FileWithBackup -Source $_.FullName -Destination (Join-Path $starshipProfilesDir $_.Name)
}

Ensure-PowerShellProfileIntegrations
Ensure-NpmGlobalBinInPath

if (Test-WSLAvailable) {
    try {
        $distros = & wsl -l -q 2>$null
        if ($LASTEXITCODE -eq 0 -and $distros) {
            Write-Warn 'Fish on Windows should be run via WSL. Install fish inside your WSL distro if needed.'
        } else {
            Write-Warn 'WSL is available but no distro detected. Fish support requires a configured WSL distro.'
        }
    } catch {
        Write-Warn 'Unable to query WSL distro status.'
    }
} else {
    Write-Warn 'WSL is not installed. Native Windows flow uses PowerShell + Starship.'
}

Write-Section 'VERIFICATION'

$terminalCommand = 'rldyourterm'

$checks = @(
    @{ Name = $terminalCommand; Args = @('--version') },
    @{ Name = 'pwsh'; Args = @('--version') },
    @{ Name = 'starship'; Args = @('--version') },
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
Write-Host '[SUCCESS] Windows foundation completed.' -ForegroundColor Green
Write-Host 'Next: run scripts/windows/install-layer-1.ps1 .. install-layer-5.ps1' -ForegroundColor Green
