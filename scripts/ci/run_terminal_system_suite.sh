#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_terminal_system_suite.sh [report-path] [--governance-mode <ci|release>] [--benchmark-report <path>] [--benchmark-baseline <path>] [--with-live-display <smoke|full|controlled>] [--live-display-report <path>] [--live-display-baseline <path>]
USAGE
}

report_path="target/terminal-benchmark/system-suite-report.json"
benchmark_report_path=""
governance_mode="ci"
benchmark_baseline_path=""
live_display_mode=""
live_display_report_path=""
live_display_baseline_path=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --governance-mode)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --governance-mode" >&2
        usage
        exit 2
      fi
      governance_mode="$2"
      shift 2
      ;;
    --benchmark-report)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --benchmark-report" >&2
        usage
        exit 2
      fi
      benchmark_report_path="$2"
      shift 2
      ;;
    --benchmark-baseline)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --benchmark-baseline" >&2
        usage
        exit 2
      fi
      benchmark_baseline_path="$2"
      shift 2
      ;;
    --with-live-display)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --with-live-display" >&2
        usage
        exit 2
      fi
      live_display_mode="$2"
      shift 2
      ;;
    --live-display-report)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --live-display-report" >&2
        usage
        exit 2
      fi
      live_display_report_path="$2"
      shift 2
      ;;
    --live-display-baseline)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --live-display-baseline" >&2
        usage
        exit 2
      fi
      live_display_baseline_path="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --*)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
    *)
      report_path="$1"
      shift
      ;;
  esac
done

case "$governance_mode" in
  ci|release) ;;
  *)
    echo "invalid governance mode: $governance_mode" >&2
    usage
    exit 2
    ;;
esac

case "$live_display_mode" in
  ""|smoke|full|controlled) ;;
  *)
    echo "invalid live display mode: $live_display_mode" >&2
    usage
    exit 2
    ;;
esac

if [[ -z "$benchmark_report_path" ]]; then
  benchmark_report_path="${report_path%.json}.benchmark.json"
fi

if [[ -n "$live_display_mode" && -z "$live_display_report_path" ]]; then
  live_display_report_path="${report_path%.json}.display.json"
fi

mkdir -p "$(dirname "$report_path")" "$(dirname "$benchmark_report_path")"
if [[ -n "$live_display_report_path" ]]; then
  mkdir -p "$(dirname "$live_display_report_path")"
fi

quality_gates=()

run_gate() {
  local label="$1"
  shift
  quality_gates+=("$label")
  "$@"
}

run_gate "cargo fmt --all -- --check" cargo fmt --all -- --check
run_gate "cargo check --workspace --all-targets --locked" cargo check --workspace --all-targets --locked
run_gate "cargo test --workspace --locked" cargo test --workspace --locked
run_gate "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings" cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
run_gate "cargo +1.92.0 check --workspace --all-targets --locked" cargo +1.92.0 check --workspace --all-targets --locked
run_gate "cargo check --manifest-path fuzz/Cargo.toml --locked" cargo check --manifest-path fuzz/Cargo.toml --locked
run_gate "bash scripts/ci/run_terminal_benchmark_smoke.sh" bash scripts/ci/run_terminal_benchmark_smoke.sh
if [[ -n "$benchmark_baseline_path" ]]; then
  run_gate "TERMINAL_BENCHMARK_BASELINE=$benchmark_baseline_path bash scripts/ci/run_terminal_benchmark_full.sh $benchmark_report_path" \
    env TERMINAL_BENCHMARK_BASELINE="$benchmark_baseline_path" \
    bash scripts/ci/run_terminal_benchmark_full.sh "$benchmark_report_path"
else
  run_gate "bash scripts/ci/run_terminal_benchmark_full.sh $benchmark_report_path" \
    bash scripts/ci/run_terminal_benchmark_full.sh "$benchmark_report_path"
fi
run_gate "bash scripts/ci/run_e2e_governance.sh --mode $governance_mode" bash scripts/ci/run_e2e_governance.sh --mode "$governance_mode"
if [[ -n "$live_display_mode" ]]; then
  if [[ -n "$live_display_baseline_path" ]]; then
    run_gate "TERMINAL_DISPLAY_BENCHMARK_BASELINE=$live_display_baseline_path bash scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh $live_display_report_path" \
      env TERMINAL_DISPLAY_BENCHMARK_BASELINE="$live_display_baseline_path" \
      bash "scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh" "$live_display_report_path"
  else
    run_gate "bash scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh $live_display_report_path" \
      bash "scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh" "$live_display_report_path"
  fi
fi

emit_args=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance system-suite emit
  --report "$report_path"
  --benchmark-report "$benchmark_report_path"
  --governance-mode "$governance_mode"
)
if [[ -n "$benchmark_baseline_path" ]]; then
  emit_args+=(--benchmark-baseline "$benchmark_baseline_path")
fi
if [[ -n "$live_display_mode" ]]; then
  emit_args+=(--live-display-mode "$live_display_mode")
fi
if [[ -n "$live_display_report_path" ]]; then
  emit_args+=(--live-display-report "$live_display_report_path")
fi
if [[ -n "$live_display_baseline_path" ]]; then
  emit_args+=(--live-display-baseline "$live_display_baseline_path")
fi
for gate in "${quality_gates[@]}"; do
  emit_args+=(--quality-gate "$gate")
done

"${emit_args[@]}"

validation_args=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance system-suite validate
  --report "$report_path"
  --benchmark-report "$benchmark_report_path"
  --governance-mode "$governance_mode"
)

if [[ -n "$benchmark_baseline_path" ]]; then
  validation_args+=(--benchmark-baseline "$benchmark_baseline_path")
fi
if [[ -n "$live_display_mode" ]]; then
  validation_args+=(--live-display-mode "$live_display_mode")
fi
if [[ -n "$live_display_report_path" ]]; then
  validation_args+=(--live-display-report "$live_display_report_path")
fi
if [[ -n "$live_display_baseline_path" ]]; then
  validation_args+=(--live-display-baseline "$live_display_baseline_path")
fi

"${validation_args[@]}"

echo "terminal system suite ok: $report_path"
