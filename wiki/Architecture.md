# Architecture

## Design Goals

- Deterministic provisioning
- Clear platform boundaries
- Re-runnable layer scripts
- Reproducible validation before/after changes

## Topology

```text
Dispatcher: scripts/install.sh
  -> Linux pipeline (bash)
  -> macOS pipeline (bash/Homebrew)
  -> Windows pipeline (PowerShell/WinGet)

Per-platform flow:
  Foundation -> Layer1 -> Layer2 -> Layer3 -> Layer4 -> Layer5
```

## Key Boundaries

- `scripts/`: install and health-check behavior
- `configs/`: canonical template configs
- `docs/`: deep reference and runbooks
- `wiki/`: operational quick guides

## Validation Boundary

- Linux: `scripts/health-check.sh`
- macOS: `scripts/macos/health-check.sh`
- Windows: `scripts/windows/health-check.ps1`

## Operational Contract

- Script changes require doc updates.
- Behavior changes require changelog updates.
- Wiki should remain concise and linked to canonical docs.
