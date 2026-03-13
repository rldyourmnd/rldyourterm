#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
SUPPORT_SCRIPT="$ROOT_DIR/ops/jenkins/controller/support/run_pr_ci.sh"

if [[ ! -f "$SUPPORT_SCRIPT" ]]; then
  echo "missing Jenkins support script: $SUPPORT_SCRIPT" >&2
  exit 1
fi

exec bash "$SUPPORT_SCRIPT" "$@"
