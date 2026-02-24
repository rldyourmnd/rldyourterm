# rldyourterm Wiki

This wiki is the operational playbook for `rldyourterm`.

## Scope

- Linux (Debian/Ubuntu): production
- macOS: production
- Windows (PowerShell + WinGet): production

## Start Here

1. [Getting Started](Getting-Started)
2. [Platform Guide](Platform-Guide)
3. [Installation and Layers](Installation-and-Layers)
4. [Operations Runbook](Operations-Runbook)

## What Was Updated

- Linux installer hardening:
  - no direct installer `curl | sh` pattern in Linux scripts,
  - non-interactive APT behavior,
  - `cargo install --locked` for reproducible Rust CLI installs,
  - checksum-validation attempts for release archives where available.
- Linux flow smoke entrypoint: `./scripts/install.sh --dry-run`.
- Windows helper flags:
  - `./scripts/install.sh --help` in shell dispatch flow,
  - `-Help` for `install-windows.ps1` and `health-check-windows.ps1`.
- CI now includes Linux, macOS, and Windows smoke/parsing coverage.
- rldyourterm runtime profile is stability-first on Linux (X11 baseline) with explicit
  overrides (`RLDYOURTERM_FORCE_WAYLAND`, `RLDYOURTERM_FORCE_X11`, `RLDYOURTERM_AUTO_WAYLAND`,
  `RLDYOURTERM_SAFE_RENDERER`, `RLDYOURTERM_STABLE_RESIZE`, `RLDYOURTERM_MINIMAL_UI`).

## Canonical Sources

- README: <https://github.com/rldyourmnd/rldyourterm/blob/main/README.md>
- Platform docs: <https://github.com/rldyourmnd/rldyourterm/tree/main/docs/platforms>
- Operations docs: <https://github.com/rldyourmnd/rldyourterm/tree/main/docs/operations>
- Linux reference docs: <https://github.com/rldyourmnd/rldyourterm/tree/main/docs/platforms/linux/reference>

## Wiki Maintenance

- Wiki source lives in `wiki/` in this repository.
- Publish with `scripts/publish-wiki.sh`.
- Update wiki and docs in the same PR when operator behavior changes.
