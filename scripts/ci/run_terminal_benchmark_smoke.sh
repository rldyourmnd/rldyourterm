#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-$(mktemp -t rldyourterm-terminal-benchmark.XXXXXX.json)}"

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale quick \
  --warmup-iterations 0 \
  --iterations 1 \
  --format json \
  --output "$report_path" \
  >/dev/null

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  validate \
  --suite canonical-headless \
  --report "$report_path" \
  --require-full-suite

echo "benchmark smoke ok: $report_path"
