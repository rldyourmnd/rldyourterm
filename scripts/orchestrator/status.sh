#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"

for pid_file in "$root_dir"/scripts/orchestrator/pids/*.pid; do
  [[ -e "$pid_file" ]] || continue
  agent="$(basename "$pid_file" .pid)"
  pid="$(cat "$pid_file")"
  exit_file="$root_dir/scripts/orchestrator/outputs/${agent}.exit"
  if [[ -f "$exit_file" ]]; then
    code="$(cat "$exit_file")"
    echo "$agent done exit=$code"
  else
    if kill -0 "$pid" 2>/dev/null; then
      echo "$agent running pid=$pid"
    else
      echo "$agent stopped-no-exit pid=$pid"
    fi
  fi
done
