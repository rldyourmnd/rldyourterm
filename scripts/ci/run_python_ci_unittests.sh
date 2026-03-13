#!/usr/bin/env bash
set -euo pipefail

modules=(
  scripts.ci.test_terminal_benchmark_environment
  scripts.ci.test_validate_terminal_benchmark_report
)

append_if_present() {
  local path="$1"
  local module="$2"
  if [[ -f "$path" ]]; then
    modules+=("$module")
  fi
}

append_if_present "scripts/ci/test_terminal_display_governance.py" "scripts.ci.test_terminal_display_governance"
append_if_present "scripts/ci/test_terminal_system_suite_governance.py" "scripts.ci.test_terminal_system_suite_governance"

python3 -m unittest "${modules[@]}"
