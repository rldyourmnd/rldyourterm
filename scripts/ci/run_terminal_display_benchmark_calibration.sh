#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/live-display-controlled-report.json}"
baseline_path="${2:-terminal_benchmark/baselines/live-display.controlled.json}"
calibration_report_path="${3:-${report_path%.json}.calibration.json}"
runner_readiness_report_path="${4:-}"
comparison_mode="${TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE:-advisory}"
required_session_type="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_SESSION_TYPE:-}"
required_display_server_hint="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_DISPLAY_SERVER_HINT:-}"

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

emit_args=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance calibration emit
  --report "$calibration_report_path"
  --benchmark-report "$report_path"
  --baseline "$baseline_path"
  --comparison-mode "$comparison_mode"
)
if [[ -n "$required_session_type" ]]; then
  emit_args+=(--required-session-type "$required_session_type")
fi
if [[ -n "$required_display_server_hint" ]]; then
  emit_args+=(--required-display-server-hint "$required_display_server_hint")
fi
if [[ -n "$runner_readiness_report_path" ]]; then
  emit_args+=(--runner-readiness-report "$runner_readiness_report_path")
fi

"${emit_args[@]}"

validation_args=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance calibration validate
  --report "$calibration_report_path"
  --benchmark-report "$report_path"
  --baseline "$baseline_path"
  --comparison-mode "$comparison_mode"
)
if [[ -n "$required_session_type" ]]; then
  validation_args+=(--required-session-type "$required_session_type")
fi
if [[ -n "$required_display_server_hint" ]]; then
  validation_args+=(--required-display-server-hint "$required_display_server_hint")
fi
if [[ -n "$runner_readiness_report_path" ]]; then
  validation_args+=(--runner-readiness-report "$runner_readiness_report_path")
fi

"${validation_args[@]}"

echo "live display benchmark calibration ok: report=$report_path baseline=$baseline_path calibration=$calibration_report_path"
