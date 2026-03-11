#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/live-display-controlled-report.json}"
baseline_path="${2:-terminal_benchmark/baselines/live-display.controlled.json}"
comparison_mode="${TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE:-advisory}"

case "$comparison_mode" in
  advisory|enforced) ;;
  *)
    echo "invalid TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE: $comparison_mode" >&2
    exit 2
    ;;
esac

bash scripts/ci/run_terminal_display_benchmark_controlled.sh "$report_path"

python3 scripts/ci/refresh_terminal_benchmark_baseline.py \
  "$report_path" \
  "$baseline_path" \
  --comparison-mode "$comparison_mode" \
  --environment-scope controlled-display-session

threshold_args=(
  python3 scripts/ci/validate_terminal_benchmark_thresholds.py
  "$report_path"
  "$baseline_path"
)
if [[ "$comparison_mode" == "advisory" ]]; then
  threshold_args+=(--allow-advisory)
fi

"${threshold_args[@]}"

echo "live display benchmark calibration ok: report=$report_path baseline=$baseline_path"
