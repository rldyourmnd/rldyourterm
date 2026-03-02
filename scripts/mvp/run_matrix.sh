#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
repeat="${1:-3}"
single_window_required="1"
release_governance="manual-only"

if ! [[ "$repeat" =~ ^[0-9]+$ ]] || [[ "$repeat" -lt 3 ]]; then
  echo "invalid repeat value: $repeat (expected integer >= 3 for sustained-run gate)" >&2
  exit 2
fi

profiles=(claude codex gemini)
profiles_csv="$(IFS=,; echo "${profiles[*]}")"
passed=0
failed=0
failed_profiles=()

echo "MVP_MATRIX_START repeat=$repeat profiles=$profiles_csv single_window_required=$single_window_required release_governance=$release_governance"

for profile in "${profiles[@]}"; do
  echo "MVP_MATRIX_PROFILE profile=$profile repeat=$repeat single_window_required=$single_window_required release_governance=$release_governance"
  if ! "$SCRIPT_DIR/scenario_${profile}.sh" "$repeat"; then
    failed_profiles+=("$profile")
    failed=$((failed + 1))
  else
    passed=$((passed + 1))
  fi
  echo
  sleep 1
done

if [[ "$failed" -gt 0 ]]; then
  failed_profiles_csv="$(IFS=,; echo "${failed_profiles[*]}")"
  echo "MVP_MATRIX_RESULT status=fail repeat=$repeat passed=$passed failed=$failed profiles=$profiles_csv failed_profiles=$failed_profiles_csv single_window_required=$single_window_required release_governance=$release_governance" >&2
  exit 1
fi

echo "MVP_MATRIX_RESULT status=pass repeat=$repeat passed=$passed failed=$failed profiles=$profiles_csv failed_profiles=none single_window_required=$single_window_required release_governance=$release_governance"
