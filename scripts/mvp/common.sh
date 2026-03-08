#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/output"
REPO_HEAD_SHA="$(cd "$ROOT_DIR" && git rev-parse HEAD)"

ensure_output_dir() {
  mkdir -p "$OUTPUT_DIR"
}

render_command_line() {
  local rendered=""
  printf -v rendered '%q ' "$@"
  echo "${rendered% }"
}

run_app_capture() {
  local profile="$1"
  local repeat="$2"
  local log_file="$3"
  shift 3
  local started_at_utc
  started_at_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  local -a cmd=(
    cargo run -q -p rldyourterm-app --
    --mode auto
    --shell fish
    --window-count 1
    --refresh-rate-millihz 60000
    --mvp-profile "$profile"
    --mvp-repeat "$repeat"
    --mvp-command single-window:1
  )

  for extra_command in "$@"; do
    cmd+=(--mvp-command "$extra_command")
  done

  local command_line
  command_line="$(render_command_line "${cmd[@]}")"

  (
    cd "$ROOT_DIR"
    echo "MVP_HARNESS profile=$profile repeat=$repeat started_at_utc=$started_at_utc repo_head_sha=$REPO_HEAD_SHA single_window_required=1 release_governance=manual-only"
    echo "MVP_HARNESS_CMD $command_line"
    "${cmd[@]}"
  ) | tee "$log_file"
}

extract_result_line() {
  local log_file="$1"
  grep '^MVP_RESULT ' "$log_file" | tail -n 1 || true
}

result_field() {
  local result_line="$1"
  local key="$2"
  local token

  for token in $result_line; do
    case "$token" in
      "$key="*)
        echo "${token#*=}"
        return 0
        ;;
    esac
  done

  return 1
}

assert_result_line() {
  local profile="$1"
  local log_file="$2"

  local result_line
  result_line="$(extract_result_line "$log_file")"

  if [[ -z "$result_line" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=missing-result log=$log_file" >&2
    return 1
  fi

  local result_profile
  result_profile="$(result_field "$result_line" profile || true)"
  if [[ "$result_profile" != "$profile" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=profile-mismatch actual_profile=${result_profile:-missing} log=$log_file" >&2
    return 1
  fi

  local result_windows
  result_windows="$(result_field "$result_line" windows || true)"
  if [[ "$result_windows" != "1" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=single-window-violation windows=${result_windows:-missing} log=$log_file" >&2
    return 1
  fi

  local result_state
  result_state="$(result_field "$result_line" state || true)"
  if [[ "$result_state" != "running" && "$result_state" != "degraded" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=invalid-state state=${result_state:-missing} log=$log_file" >&2
    return 1
  fi

  local result_single_window_required
  result_single_window_required="$(result_field "$result_line" single_window_required || true)"
  if [[ "$result_single_window_required" != "1" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=single-window-required-missing value=${result_single_window_required:-missing} log=$log_file" >&2
    return 1
  fi

  local result_single_window_enforced
  result_single_window_enforced="$(result_field "$result_line" single_window_enforced || true)"
  if [[ "$result_single_window_enforced" != "yes" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=single-window-enforcement-missing value=${result_single_window_enforced:-missing} log=$log_file" >&2
    return 1
  fi

  local result_release_governance
  result_release_governance="$(result_field "$result_line" release_governance || true)"
  if [[ "$result_release_governance" != "manual-only" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=release-governance-mismatch value=${result_release_governance:-missing} log=$log_file" >&2
    return 1
  fi

  local running_step_observed
  running_step_observed="$(result_field "$result_line" running_step_observed || true)"
  if [[ "$running_step_observed" != "yes" ]]; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=running-step-missing value=${running_step_observed:-missing} log=$log_file" >&2
    return 1
  fi

  local recoverable_observed
  recoverable_observed="$(result_field "$result_line" recoverable_observed || true)"
  local cadence_resync_observed
  cadence_resync_observed="$(result_field "$result_line" cadence_resync_observed || true)"
  local gpu_retry_observed
  gpu_retry_observed="$(result_field "$result_line" gpu_retry_observed || true)"
  local fallback_observed
  fallback_observed="$(result_field "$result_line" fallback_observed || true)"

  case "$profile" in
    claude)
      if [[ "$recoverable_observed" != "yes" ]]; then
        echo "MVP_GATE_RESULT profile=$profile status=fail reason=recoverable-evidence-missing value=${recoverable_observed:-missing} log=$log_file" >&2
        return 1
      fi
      ;;
    codex)
      if [[ "$gpu_retry_observed" != "yes" ]]; then
        echo "MVP_GATE_RESULT profile=$profile status=fail reason=gpu-retry-evidence-missing value=${gpu_retry_observed:-missing} log=$log_file" >&2
        return 1
      fi
      if [[ "$fallback_observed" != "yes" ]]; then
        echo "MVP_GATE_RESULT profile=$profile status=fail reason=gpu-fallback-evidence-missing value=${fallback_observed:-missing} log=$log_file" >&2
        return 1
      fi
      ;;
    gemini)
      if [[ "$cadence_resync_observed" != "yes" ]]; then
        echo "MVP_GATE_RESULT profile=$profile status=fail reason=cadence-resync-evidence-missing value=${cadence_resync_observed:-missing} log=$log_file" >&2
        return 1
      fi
      ;;
  esac

  if ! grep -q '^MVP_STEP .*command=single-window:1 ' "$log_file"; then
    echo "MVP_GATE_RESULT profile=$profile status=fail reason=single-window-step-missing log=$log_file" >&2
    return 1
  fi

  echo "MVP_GATE_RESULT profile=$profile status=pass state=$result_state windows=$result_windows single_window_required=$result_single_window_required single_window_enforced=$result_single_window_enforced release_governance=$result_release_governance recoverable_observed=$recoverable_observed cadence_resync_observed=$cadence_resync_observed gpu_retry_observed=$gpu_retry_observed fallback_observed=$fallback_observed running_step_observed=$running_step_observed log=$log_file"
}
