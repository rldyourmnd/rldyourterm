#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/full-report.json}"
scale="${TERMINAL_BENCHMARK_SCALE:-standard}"
warmup_iterations="${TERMINAL_BENCHMARK_WARMUP_ITERATIONS:-1}"
iterations="${TERMINAL_BENCHMARK_ITERATIONS:-3}"

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale "$scale" \
  --warmup-iterations "$warmup_iterations" \
  --iterations "$iterations" \
  --format json \
  --output "$report_path" \
  >/dev/null

python3 scripts/ci/validate_terminal_benchmark_report.py \
  "$report_path" \
  --require-full-suite

echo "benchmark full ok: $report_path"
