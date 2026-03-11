#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/live-display-runner-readiness.json}"
required_session_type="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_SESSION_TYPE:-}"
required_display_server_hint="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_DISPLAY_SERVER_HINT:-}"

os_name="$(uname -s)"
session_type="${XDG_SESSION_TYPE:-}"
display_server_hint="unknown"
display_env_present="false"

case "$os_name" in
  Linux)
    if [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
      display_server_hint="wayland"
      display_env_present="true"
    elif [[ -n "${DISPLAY:-}" ]]; then
      display_server_hint="x11"
      display_env_present="true"
    fi
    ;;
  Darwin)
    display_server_hint="appkit"
    display_env_present="true"
    ;;
esac

errors=()
if [[ "$os_name" == "Linux" && "$display_env_present" != "true" ]]; then
  errors+=("linux self-hosted runner requires DISPLAY or WAYLAND_DISPLAY for display benchmark calibration")
fi
if [[ -n "$required_session_type" && "$session_type" != "$required_session_type" ]]; then
  errors+=("required session_type '$required_session_type' does not match detected '$session_type'")
fi
if [[ -n "$required_display_server_hint" && "$display_server_hint" != "$required_display_server_hint" ]]; then
  errors+=("required display_server_hint '$required_display_server_hint' does not match detected '$display_server_hint'")
fi

status="pass"
if (( ${#errors[@]} > 0 )); then
  status="fail"
fi

python3 - "$report_path" "$status" "$os_name" "$session_type" "$display_server_hint" "$display_env_present" "$required_session_type" "$required_display_server_hint" "${errors[@]}" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

report_path = pathlib.Path(sys.argv[1])
error_args = sys.argv[9:]
payload = {
    "system_tool": "terminal-display-runner-readiness",
    "status": sys.argv[2],
    "generated_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    "os": sys.argv[3],
    "session_type": sys.argv[4] or None,
    "display_server_hint": sys.argv[5],
    "display_env_present": sys.argv[6] == "true",
    "required_session_type": sys.argv[7] or None,
    "required_display_server_hint": sys.argv[8] or None,
    "errors": error_args,
}
report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY

python3 scripts/ci/validate_terminal_display_runner_readiness_report.py "$report_path"

if [[ "$status" != "pass" ]]; then
  printf 'display benchmark runner readiness failed: %s\n' "$report_path" >&2
  for error in "${errors[@]}"; do
    printf -- '- %s\n' "$error" >&2
  done
  exit 1
fi

echo "display benchmark runner readiness ok: $report_path"
