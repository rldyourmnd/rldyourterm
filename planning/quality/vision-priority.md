# Vision Priorities (Core Quality Contract)

## 2026-02-24 Priority Order

1. `СТАБИЛЬНОСТЬ`
- No session drops in both CPU and GPU modes.
- Deterministic core state machine with explicit error boundaries.
- GPU crash must auto-fallback to CPU without terminating the shell session.

2. `BEST PRACTICES FOR AI TOOLS`
- Command-line ergonomics tuned for: CodeX, OpenCode, Claude Code, Gemini CLI.
- Fast shell round-trip, low-latency copy/paste, predictable command completion behavior.
- Minimal startup friction for terminal automation workflows.

3. `СКОРОСТЬ`
- Baseline responsiveness over raw feature count.
- Two render modes:
  - `--render-mode cpu`
  - `--render-mode gpu`
  - `--render-mode auto` (GPU if healthy, fallback CPU on failure)
- Explicit RAM/perf budget discipline and frame pacing strategy.

## Non-Goals (v1)
- No full multiplexer on v1 (single-terminal baseline first).
- No external config-file-only UX; settings are first-class in-terminal controls.

Quality gates mapping:
- `planning/quality/v1.0.0-quality-gates.md`
- `planning/quality/v1.0.0-acceptance-matrix.md`

## Feature Baseline
- OS: Linux (Ubuntu/deb) and macOS.
- Shell baseline: fish + Starship by default with opt-in auto-init.
- Shell fallback path: zsh fallback is allowed; bash is not a default v1.0 target on Linux/macOS.
- Compatibility contract: ANSI/terminal baseline behavior first (colors, cursor, clear/redraw, scroll/paste stability), with controlled degradation for rare legacy extensions.
- UI target: modern visual language, cube-like/cuberpunk theme baseline.
