#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/run_profile.sh" claude "${1:-4}" \
  single-window:1 \
  recoverable:pty-read \
  tick \
  cadence:60000
