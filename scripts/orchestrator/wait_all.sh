#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"

while true; do
  remaining=0
  for pid_file in "$root_dir"/scripts/orchestrator/pids/*.pid; do
    [[ -e "$pid_file" ]] || continue
    agent="$(basename "$pid_file" .pid)"
    exit_file="$root_dir/scripts/orchestrator/outputs/${agent}.exit"
    if [[ ! -f "$exit_file" ]]; then
      remaining=$((remaining + 1))
    fi
  done

  if [[ $remaining -eq 0 ]]; then
    break
  fi

  sleep 5
done

bash "$root_dir/scripts/orchestrator/status.sh"
