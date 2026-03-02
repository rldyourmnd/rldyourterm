# rldyourterm

Crash-intolerant AI terminal runtime with v1.0 priorities locked as:
1. stability,
2. AI CLI compatibility (`Claude Code`, `Codex`, `Gemini CLI`),
3. speed.

## Source of truth

- `AGENTS.md`
- `planning/`

## MVP runtime status

This repository now includes a Rust workspace with an MVP bootstrap path:
- `rldyourterm-app`: CLI bootstrap + shell resolution + diagnostics events.
- `rldyourterm-ui`: single-window runtime scaffold with command-driven control hooks.
- `scripts/mvp/*`: executable compatibility harness for Claude/Codex/Gemini scenarios.

Single-window baseline is explicit and mandatory in v1.0 (`window_count=1`).

## Session start

```bash
bash planning/system/validate_planning.sh
```

## Quick checks

```bash
cargo check -p rldyourterm-ui
cargo check -p rldyourterm-app
```

## MVP compatibility harness

Required manual release gate (covers `R-11`, `R-12`, `R-13`):

```bash
bash scripts/mvp/run_matrix.sh 1
```

Extended compatibility soak:

```bash
bash scripts/mvp/run_matrix.sh 3
```

Run a single profile with optional extra commands:

```bash
bash scripts/mvp/run_profile.sh codex 2 recoverable:pty-write tick mode:cpu
```

Harness logs are written to `scripts/mvp/output/`.
Each required gate run must produce these evidence lines:
- `MVP_PROFILE_START ... single_window_required=1 release_governance=manual-only ...`
- `MVP_HARNESS ... single_window_required=1 release_governance=manual-only`
- `MVP_HARNESS_CMD ... --window-count 1 ...`
- `MVP_RESULT ... windows=1 ... single_window_required=1 single_window_enforced=yes ... release_governance=manual-only ...`
- `MVP_GATE_RESULT ... status=pass ...`
- `MVP_PROFILE_RESULT ... status=pass ...`
- `MVP_MATRIX_RESULT ... status=pass ...`
