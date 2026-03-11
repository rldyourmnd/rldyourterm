# rldyourterm

[![CI](https://github.com/rldyourmnd/rldyourterm/actions/workflows/ci.yml/badge.svg)](https://github.com/rldyourmnd/rldyourterm/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/rldyourmnd/rldyourterm/branch/main/graph/badge.svg)](https://codecov.io/gh/rldyourmnd/rldyourterm)
[![Security](https://github.com/rldyourmnd/rldyourterm/actions/workflows/ci.yml/badge.svg?branch=main&event=audit)](https://github.com/rldyourmnd/rldyourterm/security/overview)
[![CodeQL](https://github.com/rldyourmnd/rldyourterm/actions/workflows/codeql.yml/badge.svg)](https://github.com/rldyourmnd/rldyourterm/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/rldyourmnd/rldyourterm/badge)](https://securityscorecards.dev/viewer/?uri=github.com/rldyourmnd/rldyourterm)
[![License](https://img.shields.io/badge/license-GPL--3.0--only-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg)](https://www.rust-lang.org/)
[![crates.io](https://img.shields.io/crates/v/rldyourterm-app.svg)](https://crates.io/crates/rldyourterm-app)

Crash-intolerant AI terminal runtime with v1.0 priorities locked as:
1. stability,
2. AI CLI compatibility (`Claude Code`, `Codex`, `Gemini CLI`),
3. speed.

## Source of truth

- `AGENTS.md`
- `CLAUDE.md`
- `.serena/memories/`

## MVP runtime status

This repository now includes a Rust workspace with an MVP bootstrap path:
- `rldyourterm-app`: CLI bootstrap + shell resolution + diagnostics events.
- `rldyourterm-ui`: single-window runtime scaffold with command-driven control hooks.
- `scripts/mvp/*`: executable compatibility harness for Claude/Codex/Gemini scenarios.

Single-window baseline is explicit and mandatory in v1.0 (`window_count=1`).

## Quick checks

```bash
cargo check --locked -p rldyourterm-ui
cargo check --locked -p rldyourterm-app
```

## Benchmarking

`terminal_benchmark/` now provides two benchmark suites:
- `canonical-headless` for deterministic CI-safe coverage across `core`, `services/session`, `ui`, `features/settings`, `features/shell-integration`, `features/font`, `features/render-gpu` policy helpers, and `features/render-cpu`
- `live-display` for real `winit` window plus `wgpu`/`softbuffer` presentation timing on a live display session

The canonical headless JSON report still marks `app`, `foundation`, `foundation-platform`, and `features/diagnostics` as correctness-only layers. The live-display suite is intentionally local/manual and does not run as a required PR CI gate.

Quick run:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- --scenario all --scale standard
```

Structured JSON output:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --suite canonical-headless \
  --scenario all \
  --scale stress \
  --format json \
  --output /tmp/rldyourterm-terminal-benchmark.json
```

Headless CI-parity smoke run:

```bash
bash scripts/ci/run_terminal_benchmark_smoke.sh
```

Full benchmark suite:

```bash
bash scripts/ci/run_terminal_benchmark_full.sh
```

Live-display local smoke run:

```bash
bash scripts/ci/run_terminal_display_benchmark_smoke.sh
```

Live-display local full run:

```bash
bash scripts/ci/run_terminal_display_benchmark_full.sh
```

Controlled live-display calibration run:

```bash
bash scripts/ci/run_terminal_display_benchmark_controlled.sh
```

Controlled live-display calibration run with baseline refresh:

```bash
bash scripts/ci/run_terminal_display_benchmark_calibration.sh \
  target/terminal-benchmark/live-display-controlled-report.json \
  terminal_benchmark/baselines/live-display.controlled.json \
  target/terminal-benchmark/live-display-controlled-calibration.json
```

Use `TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE=enforced` only when you intentionally want the calibrated baseline to become a hard gate.

There is also a manual self-hosted GitHub workflow for the same controlled calibration flow:
- `.github/workflows/display-benchmark.yml`
- runner labels required: `self-hosted`, `display-benchmark`, and the selected OS label
- it uploads four artifacts: runner readiness report, controlled report, controlled baseline, calibration report

Optional threshold validation against versioned baselines:

```bash
TERMINAL_BENCHMARK_BASELINE=terminal_benchmark/baselines/canonical-headless.standard.json \
  bash scripts/ci/run_terminal_benchmark_full.sh

TERMINAL_DISPLAY_BENCHMARK_BASELINE=terminal_benchmark/baselines/live-display.quick.json \
  bash scripts/ci/run_terminal_display_benchmark_smoke.sh
```

Headless baselines are enforced. Live-display baselines are advisory by default: regressions are reported to stderr but do not turn the command into a hard failure unless you deliberately promote that policy.

Benchmark baseline scope is fail-closed:
- `portable-headless` baselines apply only to canonical headless reports
- `local-display-session` baselines apply only to generic local live-display reports
- `controlled-display-session` baselines can only be refreshed from, and applied to, monitor-aware controlled live-display reports
- controlled baselines now also carry exact environment requirements for the calibrated host profile, including display server hint and per-scenario CPU monitor metadata

Canonical local system suite:

```bash
bash scripts/ci/run_terminal_system_suite.sh
```

Canonical local system suite with optional live-display coverage:

```bash
bash scripts/ci/run_terminal_system_suite.sh --with-live-display smoke
```

Canonical local system suite with controlled live-display validation:

```bash
bash scripts/ci/run_terminal_system_suite.sh --with-live-display controlled
```

Canonical local system suite with baselines:

```bash
bash scripts/ci/run_terminal_system_suite.sh \
  --benchmark-baseline terminal_benchmark/baselines/canonical-headless.standard.json \
  --with-live-display smoke \
  --live-display-baseline terminal_benchmark/baselines/live-display.quick.json
```

The system suite emits a machine-readable JSON report and validates the referenced full benchmark report and, when requested, the live-display benchmark report and benchmark baselines before returning success.

CPU live-display reports now include:
- suite-level environment metadata: `session_type` and `display_server_hint`
- phase-level timing for `redraw_dispatch`, which captures the wait between `request_redraw` and `RedrawRequested`
- phase-level timing for `buffer_acquire`, `raster`, and `present`
- `cpu_buffer_age_counts`, which shows how often the softbuffer framebuffer arrives with `age=0`, `age=1`, `age=2`, or `age>=3`
- `pacing_mode` and `monitor_refresh_rate_millihz`, which show whether the live run used monitor-driven cadence or the same `event-driven` fallback that production uses when monitor timing is unavailable
- `monitor_name` and `monitor_scale_factor`, which show whether `winit` exposed concrete monitor metadata for the run

Controlled live-display validation adds monitor-aware requirements:
- `steady-redraw-cpu` and `resize-cycle-cpu` must use `pacing_mode=monitor-cadence`
- monitor refresh and scale metadata must be present for those scenarios
- optional session/display-server expectations can be provided through `TERMINAL_DISPLAY_BENCHMARK_REQUIRED_SESSION_TYPE` and `TERMINAL_DISPLAY_BENCHMARK_REQUIRED_DISPLAY_SERVER_HINT`

That keeps local display regressions explainable before touching production render code. The current CPU display path is explicitly age-aware and replays a two-frame damage history when `softbuffer` returns `age=2`.

## CI/CD profile

- Current PR-visible check suite includes `CI`, `CodeQL`, `ClusterFuzzLite PR fuzzing`, `Scorecard`, `Semantic PR`, and `PR Automation`.
- `CI` gate is strict for critical fan-out jobs (`check/test/clippy/fmt/msrv/audit/deny` must be `success`); only `coverage` may be `skipped` when disabled.
- `CodeQL` publishes Rust extraction diagnostics artifact and fails closed when diagnostics telemetry is missing/invalid; actionable diagnostics (`ExtractionErrors > 0` or non-benign extraction warnings) block merges, while known generated build-output warnings remain observable but non-blocking.
- Release governance remains manual-only (`workflow_dispatch`) with enforced preflight (`terminal system suite`, `security gates`, `AI CLI compatibility matrix`).
- Weekly non-blocking soak lane: `.github/workflows/soak.yml` runs `scripts/mvp/run_matrix.sh` and publishes artifacts for long-run compatibility evidence.

## Fuzzing

The repository includes `cargo-fuzz` targets and ClusterFuzzLite workflows.

Local quick run:

```bash
cargo install cargo-fuzz
cargo fuzz run parser_feed -- -max_total_time=30
```

## Interactive Run

Run the binary directly as the terminal runtime (outside the MVP harness):

```bash
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1
```

Default non-harness runtime now launches the GUI window terminal path (single-window MVP baseline).
If GUI initialization fails, runtime emits a warning and falls back to TTY mode.
To force TTY mode explicitly:

```bash
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1 --tty
```

Or build once and run the binary directly:

```bash
cargo build -p rldyourterm-app
./target/debug/rldyourterm-app --mode auto --shell fish --window-count 1
```

## MVP compatibility harness

MVP harness scenarios continue to run from `scripts/mvp/*` and do not replace interactive mode.

Required manual release gate (covers `R-11`, `R-12`, `R-13`):

```bash
bash scripts/mvp/run_matrix.sh 3
```

Extended compatibility soak:

```bash
bash scripts/mvp/run_matrix.sh 5
```

Run a single profile with optional extra commands:

```bash
bash scripts/mvp/run_profile.sh codex 3 recoverable:pty-write tick mode:cpu
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
