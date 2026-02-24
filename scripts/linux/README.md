# Linux Scripts

Canonical Linux implementation for Debian/Ubuntu.

## Flow

1. `install-foundation.sh`
2. `install-layer-1.sh`
3. `install-layer-2.sh`
4. `install-layer-3.sh`
5. `install-layer-4.sh`
6. `install-layer-5.sh`

## Entrypoints

- Full flow: `./scripts/install.sh`
- Dry-run: `./scripts/install.sh --dry-run`
- Health: `./scripts/health-check.sh --summary`

## Recovery Utilities

- NVML/NVIDIA recovery: `./scripts/linux/fix-nvidia-nvml.sh`
  - starts `nvidia-persistenced` if needed
  - can register boot pull-in for `multi-user.target`
