# In-Terminal Settings UX (Command Palette)

## Current v1.0 scope (implementation sync: 2026-03-04)

- Primary trigger: `Ctrl+Shift+P` (Linux/TTY) or `Cmd+Shift+P` (macOS GUI/TTY).
- Palette UI is keyboard-shortcuts-first.
- Interactive runtime palette actions:
  - `1` -> `mode cpu`
  - `2` -> `mode gpu`
  - `3` -> `mode auto`
  - `d` -> `debug on|off`
  - `i` -> runtime info line
  - `Esc` -> close palette

## Explicit non-scope

- No free-form command-line input inside GUI/TTY palette UI.
- No in-palette text prompt/editor in v1.0 baseline.

## Where text commands exist

`crates/features/settings` exposes strict parser/apply API (`parse_palette_command`, `apply_palette_command`) for command strings:

- `mode <cpu|gpu|auto>`
- `shell <fish|zsh|auto>`
- `shell auto-init <on|off>`
- `render cadence monitor-auto`
- `theme set <cuberpunk|aurora|monochrome>`
- `profile <balanced|throughput|stability>`
- `debug <on|off>`

These command strings are consumed by runtime dispatchers and CLI automation (`--palette-command`), not by free-form typing inside palette UI.

## Validation and stability invariants

- Invalid command text must not mutate runtime state.
- Palette command result must be explicit: `Applied`, `Noop`, or `Rejected`.
- Render mode and diagnostics toggles must remain observable in runtime output/events.
