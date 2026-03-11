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

report_path="${1:-$(mktemp -t rldyourterm-terminal-display-benchmark.XXXXXX.json)}"
scenario="${TERMINAL_DISPLAY_BENCHMARK_SCENARIO:-all}"
scale="${TERMINAL_DISPLAY_BENCHMARK_SCALE:-quick}"
warmup_iterations="${TERMINAL_DISPLAY_BENCHMARK_WARMUP_ITERATIONS:-0}"
iterations="${TERMINAL_DISPLAY_BENCHMARK_ITERATIONS:-1}"

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

echo "live display benchmark smoke ok: $report_path"
