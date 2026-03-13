#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-target/terminal-benchmark/live-display-runner-readiness.json}"
required_session_type="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_SESSION_TYPE:-}"
required_display_server_hint="${TERMINAL_DISPLAY_BENCHMARK_REQUIRED_DISPLAY_SERVER_HINT:-}"
command=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance runner-readiness check
  --report "$report_path"
  --require-pass
)
if [[ -n "$required_session_type" ]]; then
  command+=(--require-session-type "$required_session_type")
fi
if [[ -n "$required_display_server_hint" ]]; then
  command+=(--require-display-server-hint "$required_display_server_hint")
fi

"${command[@]}"
