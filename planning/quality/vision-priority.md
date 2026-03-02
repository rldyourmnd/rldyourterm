# Vision Priorities (Core Quality Contract)

## 2026-03-02 Priority Order

1. `СТАБИЛЬНОСТЬ (CRASH-INTOLERANT)`
- Runtime target: no known crash path in normal/degraded states.
- Recoverable GPU/PTY/runtime errors never terminate active shell sessions.
- Panic/crash is `Sev-0` and release blocking.

2. `AI CLI COMPATIBILITY`
- Core tool targets: `Claude Code`, `Codex`, `Gemini CLI`.
- Deterministic command loop behavior, low operational noise, stable long-run sessions.
- Prompt, copy/paste, and output behavior must stay predictable under load.

3. `СКОРОСТЬ (ULTRA-LOW LATENCY)`
- Prioritize minimal prompt-to-paint and command round-trip latency.
- Enforce bounded CPU/RAM growth.
- Use monitor-driven cadence with deterministic re-sync on monitor transfer.

## Non-Goals (v1)
- No multiplexer/multi-window parity.
- No heavy visual effects baseline.
- No config-file-first UX.

## Engineering Bias
- Self-authored runtime logic first.
- External dependencies only where integration boundaries require them.

Quality gates mapping:
- `planning/quality/v1.0.0-quality-gates.md`
- `planning/quality/v1.0.0-acceptance-matrix.md`
