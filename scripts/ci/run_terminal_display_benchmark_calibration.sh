#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/live-display-controlled-report.json}"
baseline_path="${2:-terminal_benchmark/baselines/live-display.controlled.json}"
calibration_report_path="${3:-${report_path%.json}.calibration.json}"
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

python3 - "$calibration_report_path" "$report_path" "$baseline_path" "$comparison_mode" "$required_session_type" "$required_display_server_hint" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

report_path = pathlib.Path(sys.argv[2])
baseline_path = pathlib.Path(sys.argv[3])
payload = {
    "system_tool": "terminal-display-calibration",
    "status": "pass",
    "generated_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    "benchmark_report": str(report_path),
    "baseline": str(baseline_path),
    "comparison_mode": sys.argv[4],
    "required_session_type": sys.argv[5] or None,
    "required_display_server_hint": sys.argv[6] or None,
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY

python3 scripts/ci/validate_terminal_display_calibration_report.py \
  "$calibration_report_path" \
  --benchmark-report "$report_path" \
  --baseline "$baseline_path" \
  --comparison-mode "$comparison_mode"

echo "live display benchmark calibration ok: report=$report_path baseline=$baseline_path calibration=$calibration_report_path"
