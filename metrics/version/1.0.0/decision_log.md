# v1.0.0 Decision Log

Date: 2026-02-24

## Scope Lock for v1.0.0
- Platform: Ubuntu LTS + Debian-flavored ecosystems, macOS.
- Core-first delivery: single terminal baseline, затем расширения.
- Mandatory dual rendering: cpu, gpu, auto.
- Priorities: Stability > AI Tool Best Practices > Speed.
- UI profile: `cuberpunk` baseline.

## Stack Decision
- Rust workspace with strict VSA layering.
- PTY: `portable-pty` adapter in foundation.
- Window/input: `winit` adapter in foundation.
- Render: `wgpu` backend with CPU fallback.
- Text/render internals: собственный path, оптимизированный для baseline.

## Product Decisions
- Shell auto-init: `fish + starship` default, `zsh` fallback.
- Settings UX: in-terminal command palette, not external-config-first.
- Scrollback: default hard cap 50_000 lines.
- Debug/observability: enabled by runtime mode and structured events.

## Governance Decisions
- No CI for v1.0; all validation is manual.
- Release packaging path: GitHub artifacts + documented installer plan for apt/homebrew.
- Windows support is architectural scaffold only in v1.0.

## Contract Lock
- ADR baseline for v1.0.0:
  - `planning/architecture/adr-0001-layer-boundaries.md`
  - `planning/adr/0002-pty-strategy.md`
  - `planning/adr/0003-render-fallback.md`
  - `planning/adr/0004-settings-command-palette.md`
  - `planning/discovery/v1.0.0-answer-lock.md`
  - `planning/quality/v1.0.0-acceptance-matrix.md`
