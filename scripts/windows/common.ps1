Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$script:LocalBin = Join-Path $HOME '.local\bin'

function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[SUCCESS] $Message" -ForegroundColor Green
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-ErrorLine {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

function Write-Section {
    param([string]$Title)
    Write-Host ''
    Write-Host '============================================================' -ForegroundColor DarkCyan
    Write-Host "  $Title" -ForegroundColor DarkCyan
    Write-Host '============================================================' -ForegroundColor DarkCyan
}

function Assert-Windows {
    if (-not $IsWindows -and $env:OS -ne 'Windows_NT') {
        throw 'This script is supported only on Windows.'
    }
}

function Test-CommandAvailable {
    param([Parameter(Mandatory = $true)][string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function New-DirectoryIfMissing {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Refresh-SessionPath {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()

    if (-not [string]::IsNullOrWhiteSpace($machinePath)) {
        $parts += $machinePath
    }

    if (-not [string]::IsNullOrWhiteSpace($userPath)) {
        $parts += $userPath
    }

    $env:Path = ($parts -join ';')
}

function Test-PathInEnvPath {
    param(
        [Parameter(Mandatory = $true)][string]$PathEntry,
        [Parameter(Mandatory = $true)][string]$PathValue
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $normalizedEntry = [System.IO.Path]::GetFullPath($PathEntry).TrimEnd('\\')
    foreach ($segment in ($PathValue -split ';')) {
        if ([string]::IsNullOrWhiteSpace($segment)) {
            continue
        }

        $candidate = $segment.Trim().TrimEnd('\\')
        try {
            $candidate = [System.IO.Path]::GetFullPath($candidate).TrimEnd('\\')
        } catch {
            # Keep original candidate if it cannot be normalized.
        }

        if ($candidate -ieq $normalizedEntry) {
            return $true
        }
    }

    return $false
}

function Ensure-UserPathEntry {
    param([Parameter(Mandatory = $true)][string]$PathEntry)

    New-DirectoryIfMissing -Path $PathEntry

    $currentUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (Test-PathInEnvPath -PathEntry $PathEntry -PathValue $currentUserPath)) {
        $updated = if ([string]::IsNullOrWhiteSpace($currentUserPath)) {
            $PathEntry
        } else {
            "$currentUserPath;$PathEntry"
        }

        [Environment]::SetEnvironmentVariable('Path', $updated, 'User')
        Write-Success "Added to user PATH: $PathEntry"
    }

    Refresh-SessionPath
}

function Ensure-LocalBinInPath {
    Ensure-UserPathEntry -PathEntry $script:LocalBin
}

function Ensure-ExecutionPolicyRemoteSigned {
    try {
        $currentPolicy = Get-ExecutionPolicy -Scope CurrentUser
        if ($currentPolicy -in @('Undefined', 'Restricted', 'AllSigned')) {
            Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force
            Write-Success 'ExecutionPolicy for CurrentUser set to RemoteSigned'
        }
    } catch {
        Write-Warn "Could not set ExecutionPolicy automatically: $($_.Exception.Message)"
    }
}

function Ensure-ProfileFile {
    if (-not (Test-Path -LiteralPath $PROFILE)) {
        New-DirectoryIfMissing -Path (Split-Path -Parent $PROFILE)
        New-Item -ItemType File -Path $PROFILE -Force | Out-Null
        Write-Success "Created PowerShell profile: $PROFILE"
    }
}

function Add-ProfileLineIfMissing {
    param(
        [Parameter(Mandatory = $true)][string]$Line,
        [string]$Comment = ''
    )

    Ensure-ProfileFile
    $profileContent = Get-Content -Path $PROFILE -Raw -ErrorAction SilentlyContinue

    if ($profileContent -notmatch [regex]::Escape($Line)) {
        Add-Content -Path $PROFILE -Value "`n$Comment"
        Add-Content -Path $PROFILE -Value $Line
        Write-Success "Updated profile with: $Line"
    }
}

function Ensure-PowerShellProfileIntegrations {
    Add-ProfileLineIfMissing -Comment '# starship' -Line 'Invoke-Expression (&starship init powershell)'
    Add-ProfileLineIfMissing -Comment '# zoxide' -Line 'Invoke-Expression (& { (zoxide init powershell | Out-String) })'
}

function Assert-WingetAvailable {
    if (-not (Test-CommandAvailable -Name winget)) {
        throw 'winget is required but not found. Install App Installer from Microsoft Store and retry.'
    }

    $version = (& winget --version 2>$null)
    if (-not $version) {
        throw 'winget command is present but not operational.'
    }

    Write-Info "winget version: $version"
}

function Invoke-WingetInstallById {
    param(
        [Parameter(Mandatory = $true)][string]$PackageId,
        [ValidateSet('user', 'machine')][string]$Scope = 'user'
    )

    $arguments = @(
        'install',
        '--id', $PackageId,
        '--exact',
        '--source', 'winget',
        '--scope', $Scope,
        '--accept-package-agreements',
        '--accept-source-agreements',
        '--silent',
        '--disable-interactivity'
    )

    & winget @arguments
    return $LASTEXITCODE
}

function Install-WingetPackage {
    param(
        [Parameter(Mandatory = $true)][string]$DisplayName,
        [Parameter(Mandatory = $true)][string]$CommandName,
        [Parameter(Mandatory = $true)][string[]]$PackageIds,
        [scriptblock]$Fallback,
        [bool]$Required = $true
    )

    if (Test-CommandAvailable -Name $CommandName) {
        Write-Info "$DisplayName already installed"
        return $true
    }

    Assert-WingetAvailable

    foreach ($packageId in $PackageIds) {
        foreach ($scope in @('user', 'machine')) {
            Write-Info "Installing $DisplayName via winget id $packageId (scope: $scope)"
            try {
                $code = Invoke-WingetInstallById -PackageId $packageId -Scope $scope
            } catch {
                $code = 1
            }

            Refresh-SessionPath

            if ($code -eq 0 -and (Test-CommandAvailable -Name $CommandName)) {
                Write-Success "$DisplayName installed via winget ($scope scope)"
                return $true
            }

            Write-Warn "winget id $packageId failed in scope '$scope' or command '$CommandName' still unavailable"
        }
    }

    if ($Fallback) {
        Write-Warn "Trying fallback installer for $DisplayName"
        & $Fallback
        Refresh-SessionPath

        if (Test-CommandAvailable -Name $CommandName) {
            Write-Success "$DisplayName installed via fallback"
            return $true
        }
    }

    if ($Required) {
        throw "Failed to install required tool: $DisplayName"
    }

    Write-Warn "Optional tool not installed: $DisplayName"
    return $false
}

function Install-WingetPackageById {
    param(
        [Parameter(Mandatory = $true)][string]$PackageId,
        [bool]$Required = $true
    )

    Assert-WingetAvailable
    Write-Info "Installing winget package id $PackageId"

    foreach ($scope in @('user', 'machine')) {
        try {
            $code = Invoke-WingetInstallById -PackageId $PackageId -Scope $scope
        } catch {
            $code = 1
        }

        if ($code -eq 0) {
            Write-Success "winget package installed: $PackageId ($scope scope)"
            return $true
        }
    }

    if ($Required) {
        throw "Failed to install winget package id: $PackageId"
    }

    Write-Warn "Optional winget package install failed: $PackageId"
    return $false
}

function Ensure-RustToolchain {
    if (Test-CommandAvailable -Name cargo) {
        return
    }

    Install-WingetPackage -DisplayName 'Rustup' -CommandName 'rustup' -PackageIds @('Rustlang.Rustup')

    Ensure-UserPathEntry -PathEntry (Join-Path $HOME '.cargo\bin')
    Refresh-SessionPath

    if (-not (Test-CommandAvailable -Name cargo)) {
        throw 'cargo is required but unavailable after rustup installation.'
    }

    Write-Success 'Rust toolchain is ready'
}

function Install-CargoBinary {
    param(
        [Parameter(Mandatory = $true)][string]$BinaryName,
        [Parameter(Mandatory = $true)][string]$CrateName,
        [string[]]$InstallArgs = @('--locked')
    )

    if (Test-CommandAvailable -Name $BinaryName) {
        Write-Info "$BinaryName already installed"
        return
    }

    Ensure-RustToolchain

    Write-Info "Installing $CrateName via cargo"
    & cargo install $CrateName @InstallArgs

    Ensure-UserPathEntry -PathEntry (Join-Path $HOME '.cargo\bin')
    Refresh-SessionPath

    if (-not (Test-CommandAvailable -Name $BinaryName)) {
        throw "cargo install succeeded but $BinaryName is still unavailable"
    }

    Write-Success "$BinaryName installed via cargo"
}

function Install-UvViaOfficialScript {
    Write-Info 'Installing uv via official PowerShell installer'
    $scriptText = Invoke-RestMethod -Uri 'https://astral.sh/uv/install.ps1'
    Invoke-Expression $scriptText

    Ensure-UserPathEntry -PathEntry (Join-Path $HOME '.local\bin')
    Refresh-SessionPath
}

function Install-BunViaOfficialScript {
    Write-Info 'Installing bun via official PowerShell installer'
    $scriptText = Invoke-RestMethod -Uri 'https://bun.sh/install.ps1'
    Invoke-Expression $scriptText

    Ensure-UserPathEntry -PathEntry (Join-Path $HOME '.bun\bin')
    Refresh-SessionPath
}

function Install-UvTool {
    param(
        [Parameter(Mandatory = $true)][string]$CommandName,
        [Parameter(Mandatory = $true)][string]$PackageName
    )

    if (Test-CommandAvailable -Name $CommandName) {
        Write-Info "$CommandName already installed"
        return
    }

    if (-not (Test-CommandAvailable -Name uv)) {
        throw 'uv is required for uv tool install.'
    }

    Write-Info "Installing Python tool via uv: $PackageName"
    & uv tool install $PackageName

    Ensure-UserPathEntry -PathEntry (Join-Path $HOME '.local\bin')
    Refresh-SessionPath

    if (-not (Test-CommandAvailable -Name $CommandName)) {
        throw "uv tool install did not expose command: $CommandName"
    }

    Write-Success "$CommandName installed via uv tool"
}

function Ensure-NpmGlobalBinInPath {
    $npmBin = Join-Path $env:APPDATA 'npm'
    Ensure-UserPathEntry -PathEntry $npmBin
}

function Install-NpmGlobalPackage {
    param(
        [Parameter(Mandatory = $true)][string]$CommandName,
        [Parameter(Mandatory = $true)][string]$PackageName
    )

    if (Test-CommandAvailable -Name $CommandName) {
        Write-Info "$CommandName already installed"
        return
    }

    if (-not (Test-CommandAvailable -Name npm)) {
        throw 'npm is required for global package installation.'
    }

    Ensure-NpmGlobalBinInPath

    Write-Info "Installing npm package: $PackageName"
    & npm install -g $PackageName

    Refresh-SessionPath

    if (-not (Test-CommandAvailable -Name $CommandName)) {
        throw "$PackageName installed but command '$CommandName' is unavailable"
    }

    Write-Success "$CommandName installed via npm"
}

function Copy-FileWithBackup {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (-not (Test-Path -LiteralPath $Source)) {
        throw "Source file not found: $Source"
    }

    $destinationDir = Split-Path -Parent $Destination
    if (-not [string]::IsNullOrWhiteSpace($destinationDir)) {
        New-DirectoryIfMissing -Path $destinationDir
    }

    $copyRequired = $true

    if (Test-Path -LiteralPath $Destination) {
        $srcHash = (Get-FileHash -Path $Source).Hash
        $dstHash = (Get-FileHash -Path $Destination).Hash
        if ($srcHash -eq $dstHash) {
            $copyRequired = $false
        } else {
            $timestamp = Get-Date -Format 'yyyyMMdd_HHmmss'
            Copy-Item -Path $Destination -Destination "$Destination.bak.$timestamp" -Force
            Write-Warn "Backed up existing file: $Destination.bak.$timestamp"
        }
    }

    if ($copyRequired) {
        Copy-Item -Path $Source -Destination $Destination -Force
        Write-Success "Copied $(Split-Path -Leaf $Source) -> $Destination"
    } else {
        Write-Info "Already up to date: $Destination"
    }
}

function Install-GrepaiWindowsBinary {
    if (Test-CommandAvailable -Name grepai) {
        Write-Info 'grepai already installed'
        return
    }

    Ensure-LocalBinInPath

    Write-Info 'Installing grepai from latest GitHub release'
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/yoanbernabeu/grepai/releases/latest'

    $arch = if ($env:PROCESSOR_ARCHITECTURE -match 'ARM64') { 'arm64' } else { 'amd64' }

    $asset = $release.assets |
        Where-Object { $_.name -match "windows_${arch}\\.zip$" } |
        Select-Object -First 1

    if (-not $asset) {
        $asset = $release.assets |
            Where-Object { $_.name -match 'windows_.*\\.zip$' } |
            Select-Object -First 1
    }

    if (-not $asset) {
        throw 'No compatible grepai Windows zip asset found in latest release.'
    }

    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("grepai-win-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

    try {
        $zipPath = Join-Path $tempRoot $asset.name
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath

        $extractDir = Join-Path $tempRoot 'extract'
        Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force

        $binary = Get-ChildItem -Path $extractDir -Recurse -Filter 'grepai.exe' | Select-Object -First 1
        if (-not $binary) {
            throw 'grepai.exe not found inside downloaded archive.'
        }

        $destination = Join-Path $script:LocalBin 'grepai.exe'
        Copy-Item -Path $binary.FullName -Destination $destination -Force
        Write-Success "Installed grepai to $destination"
    } finally {
        Remove-Item -Path $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }

    Refresh-SessionPath

    if (-not (Test-CommandAvailable -Name grepai)) {
        throw 'grepai installation completed but command is still unavailable.'
    }
}

function Get-CommandVersion {
    param(
        [Parameter(Mandatory = $true)][string]$CommandName,
        [Parameter(Mandatory = $true)][string[]]$VersionArgs
    )

    if (-not (Test-CommandAvailable -Name $CommandName)) {
        return $null
    }

    try {
        $result = & $CommandName @VersionArgs 2>$null | Select-Object -First 1
        if ([string]::IsNullOrWhiteSpace($result)) {
            return 'installed'
        }

        return $result.ToString().Trim()
    } catch {
        return 'installed'
    }
}

function Configure-GitDefaults {
    if (-not (Test-CommandAvailable -Name git)) {
        Write-Warn 'git is not available; skipping git configuration defaults'
        return
    }

    git config --global core.pager 'delta'
    git config --global delta.line-numbers true
    git config --global delta.side-by-side true
    git config --global delta.navigate true
    git config --global delta.dark true
    git config --global delta.syntax-theme 'Catppuccin Mocha'
    git config --global delta.hyperlinks true
    git config --global delta.hyperlinks-commit-link-format 'https://github.com/{host}/{org}/{repo}/commit/{commit}'
    git config --global interactive.diffFilter 'delta --color-only'
    git config --global merge.conflictstyle diff3
    git config --global diff.colorMoved default

    git config --global alias.co checkout
    git config --global alias.br branch
    git config --global alias.ci commit
    git config --global alias.st status
    git config --global alias.unstage 'reset HEAD --'
    git config --global alias.last 'log -1 HEAD'
    git config --global alias.visual '!lazygit'
    git config --global alias.lg 'log --oneline --graph --all'

    Write-Success 'Applied git + delta defaults'
}

function Test-WSLAvailable {
    return (Test-CommandAvailable -Name wsl)
}
