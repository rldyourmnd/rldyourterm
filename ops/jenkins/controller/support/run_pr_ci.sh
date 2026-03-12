#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_pr_ci.sh <ci|codeql|scorecard> [report-root]
USAGE
}

mode="${1:-}"
report_root="${2:-target/terminal-benchmark/jenkins}"

if [[ -z "$mode" ]]; then
  usage
  exit 2
fi

root_dir="$(pwd)"
report_root="${report_root%/}"

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "required command not found: $name" >&2
    exit 1
  fi
}

pr_title="${JENKINS_PR_TITLE:-}"
repo_full_name="${JENKINS_REPO_FULL_NAME:-rldyourmnd/rldyourterm}"
pr_head_sha="${JENKINS_PR_HEAD_SHA:-}"
fuzz_toolchain="${JENKINS_RUST_FUZZ_TOOLCHAIN:-nightly-2026-03-11}"
fuzz_seconds="${JENKINS_PR_FUZZ_SECONDS:-300}"
codeql_max_extracted_with_errors="${JENKINS_CODEQL_MAX_EXTRACTED_WITH_ERRORS:-5}"

validate_semantic_pr_title() {
  local title="$1"
  local semantic_report="$2"
  local subject=""

  if [[ -z "$title" ]]; then
    echo "JENKINS_PR_TITLE is required for semantic PR validation" >&2
    exit 1
  fi

  if [[ ! "$title" =~ ^(feat|fix|refactor|docs|test|chore|ci|build|perf|style|revert)(\([A-Za-z0-9._/-]+\))?(!)?:[[:space:]]+(.+)$ ]]; then
    echo "semantic PR title validation failed: '$title'" >&2
    exit 1
  fi

  subject="${BASH_REMATCH[4]}"
  if [[ "$subject" =~ ^[A-Z] ]]; then
    echo "semantic PR title subject must not start with uppercase: '$title'" >&2
    exit 1
  fi

  printf 'title=%s\nsubject=%s\n' "$title" "$subject" >"$semantic_report"
}

run_ci_suite() {
  local root="$1"
  local ci_root="$root/ci"
  local system_suite_report="$ci_root/jenkins-system-suite.json"
  local benchmark_report="$ci_root/jenkins-benchmark-report.json"
  local -a unittest_modules=(
    scripts.ci.test_terminal_benchmark_environment
    scripts.ci.test_validate_terminal_benchmark_report
  )

  mkdir -p "$ci_root"

  validate_semantic_pr_title "$pr_title" "$ci_root/semantic-pr-title.txt"

  bash scripts/ci/validate_cflite_toolchain_pin.sh

  if [[ -f "scripts/ci/test_terminal_display_governance.py" ]]; then
    unittest_modules+=(scripts.ci.test_terminal_display_governance)
  fi

  if [[ -f "scripts/ci/test_terminal_system_suite_governance.py" ]]; then
    unittest_modules+=(scripts.ci.test_terminal_system_suite_governance)
  fi

  python3 -m unittest "${unittest_modules[@]}"

  bash scripts/ci/run_terminal_system_suite.sh \
    "$system_suite_report" \
    --benchmark-report "$benchmark_report" \
    --governance-mode ci

  cargo audit
  cargo deny check bans licenses advisories sources

  bash scripts/mvp/run_matrix.sh 3

  cargo +"$fuzz_toolchain" fuzz run parser_feed -- -max_total_time="$fuzz_seconds"
}

run_codeql_suite() {
  local root="$1"
  local codeql_root="$root/codeql"
  local database_root="$codeql_root/database"
  local diagnostics_root="$codeql_root/diagnostics"
  local sarif_path="$codeql_root/results.sarif"
  local status_file=""
  local errors=""
  local actionable=""
  local extracted_with_errors_metric=""
  local extracted_with_errors_budget=""

  require_command codeql
  require_command cargo

  mkdir -p "$codeql_root"
  rm -rf "$database_root" "$diagnostics_root" "$sarif_path"

  codeql database create "$database_root" \
    --language=rust \
    --source-root "$root_dir" \
    --command "cargo build --workspace --locked"

  codeql database analyze "$database_root" \
    codeql/rust-queries:codeql-suites/rust-security-and-quality.qls \
    --format=sarifv2.1.0 \
    --output="$sarif_path"

  CODEQL_DATABASE_ROOT="$database_root" \
    bash scripts/ci/codeql_collect_rust_diagnostics.sh "$diagnostics_root"

  status_file="$diagnostics_root/status.env"
  if [[ ! -f "$status_file" ]]; then
    echo "CodeQL diagnostics status file is missing: $status_file" >&2
    exit 1
  fi

  # shellcheck source=/dev/null
  source "$status_file"

  errors="${CODEQL_EXTRACTION_ERROR_ROWS:-0}"
  actionable="${CODEQL_ACTIONABLE_EXTRACTION_WARNING_ROWS:-0}"
  extracted_with_errors_metric="${CODEQL_EXTRACTED_WITH_ERRORS_METRIC:-0}"
  extracted_with_errors_budget="$codeql_max_extracted_with_errors"

  case "$errors" in
    ''|*[!0-9]*)
      echo "invalid CODEQL_EXTRACTION_ERROR_ROWS='$errors' in $status_file" >&2
      exit 1
      ;;
  esac
  case "$actionable" in
    ''|*[!0-9]*)
      echo "invalid CODEQL_ACTIONABLE_EXTRACTION_WARNING_ROWS='$actionable' in $status_file" >&2
      exit 1
      ;;
  esac
  case "$extracted_with_errors_metric" in
    ''|*[!0-9]*)
      echo "invalid CODEQL_EXTRACTED_WITH_ERRORS_METRIC='$extracted_with_errors_metric' in $status_file" >&2
      exit 1
      ;;
  esac
  case "$extracted_with_errors_budget" in
    ''|*[!0-9]*)
      echo "invalid extracted-with-errors budget '$extracted_with_errors_budget'" >&2
      exit 1
      ;;
  esac

  echo "CodeQL extraction diagnostics enforcement: errors=$errors actionable_warnings=$actionable extracted_with_errors_metric=$extracted_with_errors_metric extracted_with_errors_budget=$extracted_with_errors_budget"

  if [[ "$errors" -gt 0 || "$actionable" -gt 0 ]]; then
    echo "actionable CodeQL extraction diagnostics detected" >&2
    exit 1
  fi

  if [[ "$extracted_with_errors_metric" -gt "$extracted_with_errors_budget" ]]; then
    echo "CodeQL extracted-with-errors metric ($extracted_with_errors_metric) exceeded budget ($extracted_with_errors_budget)" >&2
    exit 1
  fi
}

run_scorecard_suite() {
  local root="$1"
  local scorecard_root="$root/scorecard"
  local commit_ref="${pr_head_sha:-HEAD}"

  require_command scorecard

  mkdir -p "$scorecard_root"
  GITHUB_AUTH_TOKEN="${GITHUB_TOKEN:-}" \
    scorecard \
      --local="$root_dir" \
      --commit="$commit_ref" \
      --format json \
      --show-details \
      --output "$scorecard_root/results.json"
}

cd "$root_dir"

case "$mode" in
  ci)
    run_ci_suite "$report_root"
    ;;
  codeql)
    run_codeql_suite "$report_root"
    ;;
  scorecard)
    run_scorecard_suite "$report_root"
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    echo "unknown mode: $mode" >&2
    usage
    exit 2
    ;;
esac
