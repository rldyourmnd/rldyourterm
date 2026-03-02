#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
"$SCRIPT_DIR/run_profile.sh" codex "${1:-4}" \
  single-window:1 \
  gpu-failure:surface-error:1000 \
  gpu-failure:submit-error:1500 \
  gpu-failure:swapchain-out-of-date:2000 \
  tick \
  mode:auto
