#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/full-report.json}"
scale="${TERMINAL_BENCHMARK_SCALE:-standard}"
warmup_iterations="${TERMINAL_BENCHMARK_WARMUP_ITERATIONS:-1}"
iterations="${TERMINAL_BENCHMARK_ITERATIONS:-3}"
baseline_path="${TERMINAL_BENCHMARK_BASELINE:-}"

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale "$scale" \
  --warmup-iterations "$warmup_iterations" \
  --iterations "$iterations" \
  --format json \
  --output "$report_path" \
  >/dev/null

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  validate \
  --suite canonical-headless \
  --report "$report_path" \
  --require-full-suite

if [[ -n "$baseline_path" ]]; then
  cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
    governance threshold validate \
    --report "$report_path" \
    --baseline "$baseline_path"
fi

echo "benchmark full ok: $report_path"
