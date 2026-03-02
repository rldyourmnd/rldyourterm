#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/mvp/common.sh
source "$SCRIPT_DIR/common.sh"

single_window_required="1"
release_governance="manual-only"

usage() {
  cat <<USAGE
usage: $0 <claude|codex|gemini> [repeat] [extra-mvp-command ...]

examples:
  $0 claude
  $0 codex 3
  $0 gemini 4 cadence:144000 cadence:60000
USAGE
}

if [[ $# -lt 1 ]]; then
  usage
  exit 2
fi

profile="$1"
shift
case "$profile" in
  claude|codex|gemini) ;;
  *)
    echo "invalid profile: $profile" >&2
    usage
    exit 2
    ;;
esac

repeat="1"
if [[ $# -gt 0 && "$1" =~ ^[0-9]+$ ]]; then
  repeat="$1"
  shift
fi

if ! [[ "$repeat" =~ ^[0-9]+$ ]] || [[ "$repeat" -lt 1 ]]; then
  echo "invalid repeat value: $repeat (expected integer >= 1)" >&2
  exit 2
fi

ensure_output_dir
log_file="$OUTPUT_DIR/${profile}-$(date -u +%Y%m%dT%H%M%SZ).log"
extra_command_count="$#"

echo "MVP_PROFILE_START profile=$profile repeat=$repeat extra_commands=$extra_command_count single_window_required=$single_window_required release_governance=$release_governance log=$log_file"

if ! run_app_capture "$profile" "$repeat" "$log_file" "$@"; then
  echo "MVP_PROFILE_RESULT profile=$profile status=fail reason=app-run-failed repeat=$repeat extra_commands=$extra_command_count single_window_required=$single_window_required release_governance=$release_governance log=$log_file" >&2
  exit 1
fi

if ! assert_result_line "$profile" "$log_file"; then
  echo "MVP_PROFILE_RESULT profile=$profile status=fail reason=gate-assertion-failed repeat=$repeat extra_commands=$extra_command_count single_window_required=$single_window_required release_governance=$release_governance log=$log_file" >&2
  exit 1
fi

echo "MVP_PROFILE_RESULT profile=$profile status=pass reason=gate-pass repeat=$repeat extra_commands=$extra_command_count single_window_required=$single_window_required release_governance=$release_governance log=$log_file"
