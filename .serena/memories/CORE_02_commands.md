<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: c6615cc docs(benchmark): sync calibration workflow guidance
Scope: project commands, .github/workflows/, scripts/ci/, scripts/mvp/
Area: CORE
-->

# Suggested Commands

## Build and Check
```bash
cargo check --workspace --all-targets --locked
cargo check --locked -p rldyourterm-app
cargo check --locked -p rldyourterm-ui
cargo build --workspace --locked
cargo build --locked -p rldyourterm-app
```

## Run
```bash
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1
cargo run -q -p rldyourterm-app -- --mode auto --shell fish --window-count 1 --tty
./target/debug/rldyourterm-app --mode auto --shell fish --window-count 1
```

## Test and Lint
```bash
cargo test --workspace --locked
cargo test --locked -p rldyourterm-core
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.92.0 check --workspace --all-targets --locked
```

## Governance and Benchmark
```bash
bash scripts/ci/run_e2e_governance.sh --mode ci
bash scripts/ci/validate_authority_docs.sh
bash scripts/ci/validate_vsa_dependency_graph.sh
bash scripts/ci/run_terminal_benchmark_smoke.sh
bash scripts/ci/run_terminal_benchmark_full.sh
bash scripts/ci/run_terminal_display_benchmark_smoke.sh
bash scripts/ci/run_terminal_display_benchmark_full.sh
bash scripts/ci/run_terminal_display_benchmark_controlled.sh
bash scripts/ci/run_terminal_display_benchmark_calibration.sh
TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE=enforced bash scripts/ci/run_terminal_display_benchmark_calibration.sh
bash scripts/ci/run_terminal_system_suite.sh
bash scripts/ci/run_terminal_system_suite.sh --with-live-display smoke
bash scripts/ci/run_terminal_system_suite.sh --with-live-display controlled
bash scripts/ci/run_terminal_system_suite.sh --benchmark-baseline terminal_benchmark/baselines/canonical-headless.standard.json --with-live-display smoke --live-display-baseline terminal_benchmark/baselines/live-display.quick.json
python3 scripts/ci/validate_terminal_system_suite_report.py target/terminal-benchmark/system-suite-report.json --benchmark-report target/terminal-benchmark/system-suite-report.benchmark.json --governance-mode ci
python3 scripts/ci/validate_terminal_display_environment.py /tmp/report.json --require-monitor-cadence --require-monitor-scale-factor
python3 scripts/ci/refresh_terminal_benchmark_baseline.py /tmp/report.json terminal_benchmark/baselines/custom.json
```

## Compatibility Harness
```bash
bash scripts/mvp/run_matrix.sh 3
bash scripts/mvp/run_matrix.sh 5
bash scripts/mvp/run_profile.sh codex 3 recoverable:pty-write tick mode:cpu
```

## CI/CD Surface
- `.github/workflows/ci.yml` triggers on push and pull_request to `main`
- CI jobs: check, test, coverage, clippy, benchmark-smoke, fmt, msrv, audit, deny, ci-gate
- Additional PR-visible workflows: CodeQL, ClusterFuzzLite PR fuzzing, Scorecard, Semantic PR, PR Automation
- Release workflow: `.github/workflows/release.yml` via `workflow_dispatch`, with `scripts/ci/run_terminal_system_suite.sh --governance-mode release` as the canonical pre-security validation lane
- Live-display benchmark lanes are local/manual only; they are not required PR CI gates
- Controlled live-display validation is also local/manual; it is intended for monitor-aware calibration environments rather than generic developer sessions
- `.github/workflows/display-benchmark.yml` is the manual self-hosted workflow for controlled display calibration and artifact collection; it requires runner labels `self-hosted`, `display-benchmark`, and the selected OS label
