# Scripts Layout

This directory is organized by operating system.

## Canonical Entry Points

Use these first:

- Install: `scripts/install.sh` (OS dispatcher)
- Health-check: `scripts/health-check.sh` (OS dispatcher)
- Release validation: `./scripts/release-check.sh --tag <vMAJOR.MINOR.PATCH>`

Platform wrappers (`install-macos.sh`, `install-windows.ps1`, `health-check-macos.sh`, `health-check-windows.ps1`) are convenience entrypoints.

## Canonical Implementations

- Linux: `scripts/linux/`
- macOS: `scripts/macos/`
- Windows: `scripts/windows/`

## Dispatchers

- `scripts/install.sh` dispatches by OS.
- `scripts/health-check.sh` dispatches by OS.
- `scripts/install-macos.sh` and `scripts/health-check-macos.sh` are macOS wrappers.
- `scripts/install-windows.ps1` and `scripts/health-check-windows.ps1` are Windows wrappers.

## Shared Helpers

- `scripts/shared/starship/switch-profile.sh`
- `scripts/publish-wiki.sh`

## Release Support

- `scripts/release-check.sh` validates release metadata consistency before tag publish.

## Linux Runtime Recovery

- `scripts/linux/fix-nvidia-nvml.sh` repairs common NVML/NVIDIA runtime issues (`nvidia-smi` unknown error).
