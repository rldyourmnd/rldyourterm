# Platform Guide

## Status Matrix

| Platform | Status | Install | Dry-Run | Health |
| --- | --- | --- | --- | --- |
| Linux (Debian/Ubuntu) | Production | `./scripts/install.sh` | `./scripts/install.sh --dry-run` | `./scripts/health-check.sh --summary` |
| macOS | Production | `./scripts/install.sh` or `./scripts/install-macos.sh` | `./scripts/macos/install.sh --dry-run` | `./scripts/health-check-macos.sh --summary` |
| Windows | Production | `.\scripts\install-windows.ps1` | `.\scripts\install-windows.ps1 -DryRun` | `.\scripts\health-check-windows.ps1 -Summary` |

## Linux Notes

- Non-interactive APT installs are used in scripts.
- Rust-based tools use `cargo install --locked` where applicable.
- Flow smoke available via `./scripts/install.sh --dry-run`.

## macOS Notes

- Homebrew-based provisioning.
- Shared layered model with Linux/Windows.
- Dedicated dry-run in `scripts/macos/install.sh --dry-run`.

## Windows Notes

- WinGet-first strategy with fallback installers.
- PowerShell profile integration for Starship/zoxide.
- Wrapper scripts support `-Help`.

## Platform Docs

- Linux: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/platforms/linux/README.md>
- macOS: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/platforms/macos/README.md>
- Windows: <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/platforms/windows/README.md>
