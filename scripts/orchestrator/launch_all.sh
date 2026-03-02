#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$root_dir/scripts/orchestrator/pids"

agents=(
  agent01
  agent02
  agent03
  agent04
  agent05
  agent06
  agent07
  agent08
  agent09
  agent10
)

for agent in "${agents[@]}"; do
  exit_file="$root_dir/scripts/orchestrator/outputs/${agent}.exit"
  pid_file="$root_dir/scripts/orchestrator/pids/${agent}.pid"
  launch_log="$root_dir/scripts/orchestrator/logs/${agent}.launcher.log"

  if [[ -f "$pid_file" ]]; then
    existing_pid="$(cat "$pid_file")"
    if kill -0 "$existing_pid" 2>/dev/null; then
      echo "skip $agent already running pid=$existing_pid"
      continue
    fi
  fi

  rm -f "$exit_file"
  nohup bash "$root_dir/scripts/orchestrator/run_one.sh" "$agent" >"$launch_log" 2>&1 &
  pid=$!
  echo "$pid" > "$pid_file"
  echo "launched $agent pid=$pid"
  sleep 1
done
