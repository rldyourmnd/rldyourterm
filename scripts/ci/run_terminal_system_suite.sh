#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_terminal_system_suite.sh [report-path] [--governance-mode <ci|release>] [--benchmark-report <path>] [--benchmark-baseline <path>] [--with-live-display <smoke|full>] [--live-display-report <path>] [--live-display-baseline <path>]
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
  ""|smoke|full) ;;
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

quality_gates=(
  "cargo fmt --all -- --check"
  "cargo check --workspace --all-targets --locked"
  "cargo test --workspace --locked"
  "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings"
  "cargo +1.92.0 check --workspace --all-targets --locked"
  "cargo check --manifest-path fuzz/Cargo.toml --locked"
  "bash scripts/ci/run_terminal_benchmark_smoke.sh"
  "bash scripts/ci/run_terminal_benchmark_full.sh $benchmark_report_path"
  "bash scripts/ci/run_e2e_governance.sh --mode $governance_mode"
)

if [[ -n "$live_display_mode" ]]; then
  quality_gates+=("bash scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh $live_display_report_path")
fi
if [[ -n "$benchmark_baseline_path" ]]; then
  quality_gates+=("benchmark-thresholds headless $benchmark_baseline_path")
fi
if [[ -n "$live_display_baseline_path" ]]; then
  quality_gates+=("benchmark-thresholds live-display $live_display_baseline_path")
fi

cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.92.0 check --workspace --all-targets --locked
cargo check --manifest-path fuzz/Cargo.toml --locked
bash scripts/ci/run_terminal_benchmark_smoke.sh
TERMINAL_BENCHMARK_BASELINE="$benchmark_baseline_path" \
  bash scripts/ci/run_terminal_benchmark_full.sh "$benchmark_report_path"
bash scripts/ci/run_e2e_governance.sh --mode "$governance_mode"
if [[ -n "$live_display_mode" ]]; then
  TERMINAL_DISPLAY_BENCHMARK_BASELINE="$live_display_baseline_path" \
    bash "scripts/ci/run_terminal_display_benchmark_${live_display_mode}.sh" "$live_display_report_path"
fi

python3 - "$report_path" "$benchmark_report_path" "$governance_mode" "$benchmark_baseline_path" "$live_display_mode" "$live_display_report_path" "$live_display_baseline_path" "${quality_gates[@]}" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

report_path = pathlib.Path(sys.argv[1])
benchmark_report_path = pathlib.Path(sys.argv[2])
governance_mode = sys.argv[3]
benchmark_baseline_path = sys.argv[4] or None
live_display_mode = sys.argv[5] or None
live_display_report_path = sys.argv[6] or None
live_display_baseline_path = sys.argv[7] or None
quality_gates = sys.argv[8:]

payload = {
    "system_tool": "terminal-system-suite",
    "status": "pass",
    "generated_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    "governance_mode": governance_mode,
    "benchmark_report": str(benchmark_report_path),
    "benchmark_baseline": benchmark_baseline_path,
    "live_display": None if live_display_mode is None else {
        "mode": live_display_mode,
        "report": live_display_report_path,
        "baseline": live_display_baseline_path,
    },
    "quality_gates": quality_gates,
}

report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY

validation_args=(
  python3 scripts/ci/validate_terminal_system_suite_report.py
  "$report_path"
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
