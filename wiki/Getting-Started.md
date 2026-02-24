# Getting Started

Canonical quick-start is maintained in `README.md` (`Quick Start` section). This page mirrors operator-focused steps for wiki users.

## Prerequisites

- Git and curl available
- Internet access for package downloads
- Platform package manager access:
  - Linux: `sudo` + APT
  - macOS: Homebrew
  - Windows: WinGet + PowerShell profile write access

## Quick Start by Platform

### Linux

```bash
git clone https://github.com/rldyourmnd/rldyourterm.git
cd rldyourterm
./scripts/install.sh
```

### macOS

```bash
git clone https://github.com/rldyourmnd/rldyourterm.git
cd rldyourterm
./scripts/install.sh
```

### Windows (PowerShell)

```powershell
git clone https://github.com/rldyourmnd/rldyourterm.git
cd rldyourterm
.\scripts\install-windows.ps1
```

## Smoke Checks (No Installs)

- Linux: `./scripts/install.sh --dry-run`
- macOS: `./scripts/macos/install.sh --dry-run`
- Windows: `.\scripts\install-windows.ps1 -DryRun`

## Initial Verification

- Linux: `./scripts/health-check.sh --summary`
- macOS: `./scripts/health-check-macos.sh --summary`
- Windows: `.\scripts\health-check-windows.ps1 -Summary`

Expected result: PASS with zero failures.

## Next Steps

- [Installation and Layers](Installation-and-Layers)
- [Operations Runbook](Operations-Runbook)
- [Troubleshooting](Troubleshooting)
