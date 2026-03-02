#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/run_profile.sh" gemini "${1:-4}" \
  single-window:1 \
  transfer-cadence:144000 \
  transfer-cadence:60000 \
  tick
