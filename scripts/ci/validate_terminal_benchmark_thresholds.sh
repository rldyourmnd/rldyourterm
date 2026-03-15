#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 <report> <baseline> [--allow-advisory]" >&2
  exit 2
}

if [[ $# -lt 2 ]]; then
  usage
fi

report="$1"
baseline="$2"
shift 2

allow_advisory=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --allow-advisory) allow_advisory=true; shift ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

if [[ ! -f "$report" ]]; then
  echo "report file not found: $report" >&2
  exit 1
fi

if [[ ! -f "$baseline" ]]; then
  echo "baseline file not found: $baseline" >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

args=(
  cargo run -q --locked -p rldyourterm-terminal-benchmark --
  governance threshold validate
  --report "$report"
  --baseline "$baseline"
)

if [[ "$allow_advisory" == "true" ]]; then
  args+=(--allow-advisory)
fi

cd "$REPO_ROOT"
"${args[@]}"
