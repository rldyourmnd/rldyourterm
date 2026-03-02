#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <agent-id>" >&2
  exit 2
fi

agent="$1"
root_dir="$(cd "$(dirname "$0")/../.." && pwd)"
prompt_file="$root_dir/scripts/orchestrator/prompts/${agent}.txt"
log_file="$root_dir/scripts/orchestrator/logs/${agent}.log"
final_file="$root_dir/scripts/orchestrator/outputs/${agent}.final.txt"
exit_file="$root_dir/scripts/orchestrator/outputs/${agent}.exit"

if [[ ! -f "$prompt_file" ]]; then
  echo "missing prompt file: $prompt_file" >&2
  exit 3
fi

if codex exec --skip-git-repo-check -C "$root_dir" --output-last-message "$final_file" - < "$prompt_file" > "$log_file" 2>&1; then
  echo "0" > "$exit_file"
else
  code=$?
  echo "$code" > "$exit_file"
  exit "$code"
fi
