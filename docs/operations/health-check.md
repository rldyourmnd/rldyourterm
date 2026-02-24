# Production Health Checks

Run this whenever the environment is provisioned, upgraded, or after changing
install scripts.

Linux uses `scripts/health-check.sh`.
macOS uses `scripts/macos/health-check.sh` (or wrapper `scripts/health-check-macos.sh`).
Windows uses `scripts/windows/health-check.ps1` (or wrapper `scripts/health-check-windows.ps1`).

## Quick Command

```bash
./scripts/health-check.sh
```

Linux flow smoke (no installs):

```bash
./scripts/install.sh --dry-run
```

Strict CI-style validation:

```bash
./scripts/health-check.sh --strict
```

PowerShell (Windows):

```powershell
.\scripts\health-check-windows.ps1 -Summary
.\scripts\health-check-windows.ps1 -Strict
```

macOS:

```bash
./scripts/health-check-macos.sh --summary
./scripts/health-check-macos.sh --strict
```

## What the Health Check Validates

- Bash script syntax (`bash -n scripts/*.sh`)
- PowerShell script parse validation (`*.ps1` under `scripts/`)
- Fish config syntax (`fish -n configs/fish/config.fish`)
- Mandatory config files and parity checks:
  - `~/.rldyourterm.lua` vs `configs/rldyourterm/rldyourterm.lua`
  - `~/.config/starship.toml` vs `configs/starship/starship.toml`
- Linux/macOS-only:
  - `~/.config/fish/config.fish` vs `configs/fish/config.fish`
- Required tools installation
- PATH integrity (`$HOME/.local/bin`)
- Linux `ast-grep` runtime identity (ensures `sg` is ast-grep, not util-linux `sg`)
- Known local runtime issues:
  - `semgrep` permission errors (often environment/sandbox-dependent)
  - `gemini` non-responsive invocation on this machine
  - recent compositor/runtime freeze signatures from user journal (`journalctl --user`)
    - GNOME/Mutter resize-path signals (`size change accounting`, `frame counter`, `MetaShapedTexture`)
    - rldyourterm runtime signals (`update-status` runtime error, mux broken pipe)
  - memory pressure + OOM risk indicators:
    - low `MemAvailable` snapshot from `/proc/meminfo`
    - high swap saturation percentage
    - kernel OOM signatures from `journalctl -k -b` (`oom-kill`, `Out of memory: Killed process`)
- Tool inventory contract drift (terminal+tool catalog updates)
- Installer script health (`git diff`-stable script files and executable bit)

If runtime freeze warnings appear, test these runtime overrides before escalating:

```bash
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_STABLE_RESIZE=1 rldyourterm-stable --mode stable
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_STABLE_RESIZE=1 rldyourterm-stable --mode stable
RLDYOURTERM_FORCE_X11=1 RLDYOURTERM_MINIMAL_UI=1 rldyourterm-stable --mode minimal
RLDYOURTERM_FORCE_WAYLAND=1 rldyourterm-stable --mode wayland
RLDYOURTERM_SAFE_RENDERER=1 rldyourterm-stable --mode software
```

Repo launcher shortcuts:

```bash
rldyourterm-stable --mode stable
rldyourterm-stable --mode minimal
rldyourterm-stable --mode wayland
rldyourterm-stable --mode software
```

Launcher command args:

- Safe default (shell only): `rldyourterm-stable --mode stable`
- To run a command in terminal: `rldyourterm-stable --mode stable -- <command>`
- Raw positional arguments are ignored by default unless `RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS=1` is set.

## Recommended Usage

- Run on a clean shell after changes:

```bash
./scripts/health-check.sh --summary
```

- If failures are reported, fix each item and rerun.
- Keep outputs in commit or incident notes for reproducibility.

## Summary Mode Output

`--summary` prints only the counts and a final health status line but still
performs all validation checks internally.

## Escalation

If the check reports multiple failures, prioritize:

1. Permission and environment issues (`PATH`, script syntax, command availability)
2. Config mismatch and tool availability
3. Optional/localized runtime issues (`semgrep`, `gemini`) listed in `troubleshooting.md`
4. GPU/NVML recovery via `scripts/linux/fix-nvidia-nvml.sh` when `nvidia-smi` is unhealthy
