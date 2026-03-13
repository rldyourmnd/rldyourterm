#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEFAULT_CONTRACT_PATH="$ROOT_DIR/.github/branch-protection/main.required-status-checks.json"

usage() {
  cat <<'USAGE'
usage: sync_main_branch_required_checks.sh [--mode <check|apply>] [--repo <owner/name>] [--branch <name>] [--contract <path>]

Options:
  --mode      check (default) verifies branch protection required checks against contract.
              apply updates required checks to match the contract, then verifies the result.
  --repo      GitHub repository in owner/name form. Default: rldyourmnd/rldyourterm
  --branch    Branch to validate. Default: main
  --contract  Path to required-status-checks contract JSON.
USAGE
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "required command not found: $name" >&2
    exit 1
  fi
}

normalize_contract() {
  local path="$1"
  jq -cS '
    {
      strict: (.strict // false),
      checks: (
        (.checks // [])
        | map({
            context: .context,
            app_id: (.app_id // -1)
          })
        | sort_by(.context)
      )
    }
  ' "$path"
}

normalize_remote() {
  jq -cS '
    {
      strict: (.strict // false),
      checks: (
        (
          if ((.checks // []) | length) > 0 then
            .checks
          else
            ((.contexts // []) | map({context: ., app_id: -1}))
          end
        )
        | map({
            context: .context,
            app_id: (.app_id // -1)
          })
        | sort_by(.context)
      )
    }
  '
}

assert_unique_contexts() {
  local normalized_json="$1"
  local source_label="$2"

  local total_count unique_count
  total_count="$(jq '.checks | length' <<<"$normalized_json")"
  unique_count="$(jq '[.checks[].context] | unique | length' <<<"$normalized_json")"

  if [[ "$total_count" != "$unique_count" ]]; then
    echo "${source_label} contains duplicate check contexts" >&2
    exit 1
  fi
}

api_get() {
  local path="$1"
  local token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  token="${token//$'\r'/}"
  token="${token//$'\n'/}"
  if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    gh api "$path"
    return 0
  fi

  require_command curl
  if [[ -z "$token" ]]; then
    echo "GH_TOKEN or GITHUB_TOKEN is required when gh auth is unavailable" >&2
    exit 1
  fi

  curl -fsSL \
    -H 'Accept: application/vnd.github+json' \
    -H "Authorization: Bearer ${token}" \
    -H 'X-GitHub-Api-Version: 2022-11-28' \
    "https://api.github.com/${path}"
}

api_patch() {
  local path="$1"
  local payload="$2"
  local token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  token="${token//$'\r'/}"
  token="${token//$'\n'/}"
  if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    gh api --method PATCH "$path" --input - <<<"$payload" >/dev/null
    return 0
  fi

  require_command curl
  if [[ -z "$token" ]]; then
    echo "GH_TOKEN or GITHUB_TOKEN is required when gh auth is unavailable" >&2
    exit 1
  fi

  curl -fsSL -X PATCH \
    -H 'Accept: application/vnd.github+json' \
    -H "Authorization: Bearer ${token}" \
    -H 'X-GitHub-Api-Version: 2022-11-28' \
    --data "$payload" \
    "https://api.github.com/${path}" >/dev/null
}

mode="check"
repo="rldyourmnd/rldyourterm"
branch="main"
contract_path="$DEFAULT_CONTRACT_PATH"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --mode" >&2
        usage
        exit 2
      fi
      mode="$2"
      shift 2
      ;;
    --repo)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --repo" >&2
        usage
        exit 2
      fi
      repo="$2"
      shift 2
      ;;
    --branch)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --branch" >&2
        usage
        exit 2
      fi
      branch="$2"
      shift 2
      ;;
    --contract)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --contract" >&2
        usage
        exit 2
      fi
      contract_path="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$mode" in
  check|apply) ;;
  *)
    echo "invalid --mode value: $mode (expected check|apply)" >&2
    exit 2
    ;;
esac

if [[ ! -f "$contract_path" ]]; then
  echo "missing branch protection contract: $contract_path" >&2
  exit 1
fi

require_command jq

desired_json="$(normalize_contract "$contract_path")"
assert_unique_contexts "$desired_json" "contract"

remote_json="$(
  api_get \
    "repos/${repo}/branches/${branch}/protection/required_status_checks" \
  | normalize_remote
)"
assert_unique_contexts "$remote_json" "remote required checks"

if [[ "$mode" == "apply" ]]; then
  printf '%s\n' "applying required status checks contract to ${repo}:${branch}"
  api_patch "repos/${repo}/branches/${branch}/protection/required_status_checks" "$desired_json"

  remote_json="$(
    api_get \
      "repos/${repo}/branches/${branch}/protection/required_status_checks" \
    | normalize_remote
  )"
  assert_unique_contexts "$remote_json" "remote required checks"
fi

if [[ "$desired_json" != "$remote_json" ]]; then
  echo "branch protection required checks drift detected for ${repo}:${branch}" >&2
  echo "expected:" >&2
  jq -S '.' <<<"$desired_json" >&2
  echo "actual:" >&2
  jq -S '.' <<<"$remote_json" >&2
  exit 1
fi

printf '%s\n' "branch protection required checks match contract for ${repo}:${branch}"
