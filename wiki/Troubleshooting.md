# Troubleshooting

## Fast Recovery Flow

1. Identify failing platform + layer.
2. Fix prerequisites (network, package manager, PATH, permissions).
3. Rerun only the failing layer.
4. Re-run platform health-check.

## Platform Health Commands

- Linux: `./scripts/health-check.sh --summary`
- macOS: `./scripts/health-check-macos.sh --summary`
- Windows: `.\scripts\health-check-windows.ps1 -Summary`

## Common Failures

### `command not found` after install

- Ensure user-local bin path exists and is exported:
  - Linux/macOS: `$HOME/.local/bin`
  - Windows: `$HOME\.local\bin` and `%APPDATA%\npm`

### Config parity mismatch

- Re-copy templates from `configs/` to user dotfiles.
- Re-run strict health-check.

### Terminal freeze on move/resize (multi-monitor)

- Start with repo defaults from config:
  - `cp configs/rldyourterm/rldyourterm.lua ~/.rldyourterm.lua`
- For move/resize crashes, test X11 + stable resize profile first:
  - `rldyourterm-stable --mode stable`
- If needed, test low-overhead X11 mode:
  - `rldyourterm-stable --mode minimal`
- If needed, test native Wayland explicitly:
  - `rldyourterm-stable --mode wayland`
- Last-resort renderer fallback:
  - `rldyourterm-stable --mode software`
- Inspect last logs:
  - `journalctl --user -b --since '20 minutes ago' | rg -n "size change accounting|frame counter but no frame drawn time|MetaShapedTexture|update-status event: runtime error"`

### Unexpected `Unable to spawn <arg>` in rldyourterm logs

- This usually means a positional value was passed to `rldyourterm-stable --mode <mode>` as a command name.
- Use wrapper mode for normal sessions:
  - `rldyourterm-stable --mode stable`
- Run commands explicitly:
  - `rldyourterm-stable --mode stable -- <command>`
- If you intentionally need raw positional arguments, set:
  - `RLDYOURTERM_STABLE_ALLOW_COMMAND_ARGS=1`

### Linux `sg` is not `ast-grep`

- Check `sg --version` output.
- If needed, run `~/.cargo/bin/sg --version` and reinstall ast-grep.

### Global npm permission errors

- Configure user prefix:
  - `npm config set prefix "$HOME/.local"`
  - add `$HOME/.local/bin` to PATH

## Escalation

When unresolved, provide:

- health-check output,
- failing command output,
- platform + shell + package-manager details,
- exact layer/script invoked.

## Canonical Guide

- <https://github.com/rldyourmnd/rldyourterm/blob/main/docs/operations/troubleshooting.md>
