#!/usr/bin/env bash
set -euo pipefail

require_display_session() {
  case "$(uname -s)" in
    Linux)
      if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
        echo "live display benchmark requires DISPLAY or WAYLAND_DISPLAY on Linux" >&2
        exit 2
      fi
      ;;
    Darwin)
      ;;
    *)
      ;;
  esac
}

report_path="${1:-target/terminal-benchmark/live-display-controlled-report.json}"
scenario="${TERMINAL_DISPLAY_BENCHMARK_SCENARIO:-all}"
scale="${TERMINAL_DISPLAY_BENCHMARK_SCALE:-standard}"
warmup_iterations="${TERMINAL_DISPLAY_BENCHMARK_WARMUP_ITERATIONS:-1}"
iterations="${TERMINAL_DISPLAY_BENCHMARK_ITERATIONS:-3}"
baseline_path="${TERMINAL_DISPLAY_BENCHMARK_BASELINE:-}"
required_session_type="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_SESSION_TYPE:-}"
required_display_server_hint="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_DISPLAY_SERVER_HINT:-}"

require_display_session

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  --suite live-display \
  --scenario "$scenario" \
  --scale "$scale" \
  --warmup-iterations "$warmup_iterations" \
  --iterations "$iterations" \
  --format json \
  --output "$report_path" \
  >/dev/null

validator_args=()
if [[ "$scenario" == "all" ]]; then
  validator_args+=(--require-full-suite)
else
  validator_args+=(--require-scenario "$scenario")
fi

python3 scripts/ci/validate_terminal_display_benchmark_report.py \
  "$report_path" \
  "${validator_args[@]}"

environment_args=(
  "$report_path"
  --require-monitor-cadence
  --require-monitor-scale-factor
)
if [[ -n "$required_session_type" ]]; then
  environment_args+=(--require-session-type "$required_session_type")
fi
if [[ -n "$required_display_server_hint" ]]; then
  environment_args+=(--require-display-server-hint "$required_display_server_hint")
fi

python3 scripts/ci/validate_terminal_display_environment.py \
  "${environment_args[@]}"

if [[ -n "$baseline_path" ]]; then
  python3 scripts/ci/validate_terminal_benchmark_thresholds.py \
    "$report_path" \
    "$baseline_path"
fi

echo "live display benchmark controlled ok: $report_path"
