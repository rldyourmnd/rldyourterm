#!/usr/bin/env bash
set -euo pipefail

modules=(
  scripts.ci.test_validate_terminal_benchmark_thresholds
)

python3 -m unittest "${modules[@]}"
