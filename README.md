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

Canonical headless benchmark suite lives in `terminal_benchmark/`.

Quick run:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- --scenario all --scale standard
```

Structured JSON output:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale stress \
  --format json \
  --output /tmp/rldyourterm-terminal-benchmark.json
```

CI-parity smoke run:

```bash
bash scripts/ci/run_terminal_benchmark_smoke.sh
```

## CI/CD profile

- Current PR-visible check suite includes `CI`, `CodeQL`, `ClusterFuzzLite PR fuzzing`, `Scorecard`, `Semantic PR`, and `PR Automation`.
- `CI` gate is strict for critical fan-out jobs (`check/test/clippy/fmt/msrv/audit/deny` must be `success`); only `coverage` may be `skipped` when disabled.
- `CodeQL` publishes Rust extraction diagnostics artifact and fails closed when diagnostics telemetry is missing/invalid; actionable diagnostics (`ExtractionErrors > 0` or non-benign extraction warnings) block merges, while known generated build-output warnings remain observable but non-blocking.
- Release governance remains manual-only (`workflow_dispatch`) with enforced preflight (`authority-doc validation`, `locked quality gates`, `security gates`, `AI CLI compatibility matrix`).
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
