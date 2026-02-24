# In-Terminal Settings UX (Command Palette)

## Requirement
- No external config-only UX.
- Settings must be editable from terminal command surface.

## UX Baseline
- `Ctrl+Shift+P` opens command palette.
- Minimal modal command mode for commands like:
  - `theme set cuberpunk`
  - `mode cpu` / `mode gpu` / `mode auto`
  - `render fps target 120`
  - `shell fish` `shell zsh` `shell auto-init on|off`

## Persistence
- All changes write to a typed config model in app profile with schema versioning.
- Changes are live-applied and reversible via history snapshots.

## Validation & Stability
- Commands rejected by schema should show explicit reason and keep previous valid state.
- All palette actions emit `settings.apply` trace events.

## Scope and v1.0 commands
- Baseline commands: theme, render mode, shell target, fps target, auto-init toggle.
- Invalid commands must not affect running session state.
- Every command path must emit `settings.apply` with event-id, previous state, and new state.
- Debug mode command and profile commands are allowed through a dedicated safe command namespace.
