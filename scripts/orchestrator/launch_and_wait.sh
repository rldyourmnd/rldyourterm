#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "$0")/../.." && pwd)"

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

mkdir -p "$root_dir/scripts/orchestrator/pids" "$root_dir/scripts/orchestrator/logs" "$root_dir/scripts/orchestrator/outputs"

for agent in "${agents[@]}"; do
  rm -f "$root_dir/scripts/orchestrator/outputs/${agent}.exit"
  rm -f "$root_dir/scripts/orchestrator/outputs/${agent}.final.txt"
  : > "$root_dir/scripts/orchestrator/logs/${agent}.log"
  : > "$root_dir/scripts/orchestrator/logs/${agent}.launcher.log"
  bash "$root_dir/scripts/orchestrator/run_one.sh" "$agent" >"$root_dir/scripts/orchestrator/logs/${agent}.launcher.log" 2>&1 &
  pid=$!
  echo "$pid" > "$root_dir/scripts/orchestrator/pids/${agent}.pid"
  echo "launched $agent pid=$pid"
  sleep 1
done

while true; do
  remaining=0
  for agent in "${agents[@]}"; do
    exit_file="$root_dir/scripts/orchestrator/outputs/${agent}.exit"
    if [[ ! -f "$exit_file" ]]; then
      remaining=$((remaining + 1))
    fi
  done

  echo "remaining=$remaining"

  if [[ $remaining -eq 0 ]]; then
    break
  fi

  sleep 10
done

echo "all agents finished"
for agent in "${agents[@]}"; do
  code="$(cat "$root_dir/scripts/orchestrator/outputs/${agent}.exit")"
  echo "$agent exit=$code"
done
