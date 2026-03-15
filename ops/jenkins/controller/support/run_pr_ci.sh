#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_pr_ci.sh <ci|codeql|scorecard|extended> [report-root]
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

emit_runtime_diagnostics() {
  echo "Jenkins runtime diagnostics:"
  echo "workdir: $(pwd)"
  echo "uid: $(id -u)/$(id -g), user: $(id -un)"
  echo "PATH: $PATH"
  echo "CARGO_HOME: ${CARGO_HOME:-<unset>}"
  echo "RUSTUP_HOME: ${RUSTUP_HOME:-<unset>}"
  echo "CARGO_TARGET_DIR: ${CARGO_TARGET_DIR:-<unset>}"
  echo "command -v cargo: $(command -v cargo || echo '<missing>')"
  echo "command -v rustup: $(command -v rustup || echo '<missing>')"
  echo "command -v rustc: $(command -v rustc || echo '<missing>')"
  ls -la /home/jenkins/.cache/rust 2>/dev/null | sed 's/^/  /' || true
}

ensure_cargo_on_path() {
  local candidate=""
  local home_base="${HOME:-/home/jenkins}"

  if command -v cargo >/dev/null 2>&1; then
    return 0
  fi

  for candidate in \
    "${CARGO_HOME:-$home_base/.cache/rust/cargo}" \
    "$home_base/.cache/rust/cargo" \
    "$home_base/.cache/cargo" \
    "$home_base/.cargo" \
    "/usr/local/cargo"; do
    if [[ -n "$candidate" && -x "$candidate/bin/cargo" ]]; then
      export CARGO_HOME="$candidate"
      if [[ ":$PATH:" != *":$candidate/bin:"* ]]; then
        export PATH="$candidate/bin:$PATH"
      fi
      return 0
    fi
  done

  if [[ -f "${CARGO_HOME:-$home_base/.cache/rust/cargo}/env" ]]; then
    # shellcheck source=/dev/null
    source "${CARGO_HOME:-$home_base/.cache/rust/cargo}/env"
  fi

  if command -v cargo >/dev/null 2>&1; then
    return 0
  fi

  echo "required command not found: cargo" >&2
  emit_runtime_diagnostics
  exit 127
}

ensure_cargo_on_path

default_target_dir() {
  local mode="$1"
  local explicit_dir="${JENKINS_CARGO_TARGET_DIR:-${CARGO_TARGET_DIR:-}}"

  if [[ -n "$explicit_dir" ]]; then
    printf '%s\n' "$explicit_dir"
    return 0
  fi

  printf '%s\n' "${root_dir}/.jenkins-target/${mode}"
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "required command not found: $name" >&2
    emit_runtime_diagnostics
    exit 1
  fi
}

prepare_clean_codeql_workspace() {
  local target_dir="$1"

  # CodeQL must analyze the checked-out source tree, not leftover cargo and fuzz
  # outputs from the earlier CI contour in the same Jenkins workspace.
  if [[ -n "$target_dir" && -d "$target_dir" ]]; then
    rm -rf "$target_dir"
  fi
}

pr_title="${JENKINS_PR_TITLE:-}"
pr_head_sha="${JENKINS_PR_HEAD_SHA:-}"
pr_checkout_sha="${JENKINS_PR_CHECKOUT_SHA:-}"
fuzz_toolchain="${JENKINS_RUST_FUZZ_TOOLCHAIN:-nightly-2026-03-11}"
fuzz_seconds="${JENKINS_PR_FUZZ_SECONDS:-300}"
codeql_max_extracted_with_errors="${JENKINS_CODEQL_MAX_EXTRACTED_WITH_ERRORS:-5}"
extended_benchmark_scale="${JENKINS_EXTENDED_BENCHMARK_SCALE:-stress}"
extended_matrix_repeat="${JENKINS_EXTENDED_MATRIX_REPEAT:-5}"
extended_fuzz_seconds="${JENKINS_EXTENDED_FUZZ_SECONDS:-600}"
cargo_machete_version="${JENKINS_CARGO_MACHETE_VERSION:-0.9.1}"
cargo_udeps_version="${JENKINS_CARGO_UDEPS_VERSION:-0.1.60}"
validate_branch_protection_contract="${JENKINS_VALIDATE_BRANCH_PROTECTION_CONTRACT:-1}"

ensure_cargo_tool() {
  local binary="$1"
  local crate_name="$2"
  local version="$3"
  local toolchain="${4:-1.94.0}"

  if command -v "$binary" >/dev/null 2>&1; then
    return 0
  fi

  cargo +"$toolchain" install --locked "$crate_name" --version "$version"
}

verify_main_branch_protection_contract() {
  if [[ "$validate_branch_protection_contract" != "1" ]]; then
    echo "Skipping branch protection contract verification (JENKINS_VALIDATE_BRANCH_PROTECTION_CONTRACT=$validate_branch_protection_contract)"
    return 0
  fi

  if [[ -z "${GITHUB_TOKEN:-}" ]]; then
    echo "GITHUB_TOKEN is required to verify branch protection contract in Jenkins" >&2
    exit 1
  fi

  GH_TOKEN="$GITHUB_TOKEN" bash scripts/ci/sync_main_branch_required_checks.sh --mode check
}

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
  local target_dir="${2:-$(default_target_dir ci)}"
  local ci_root="$root/ci"
  local system_suite_report="$ci_root/jenkins-system-suite.json"
  local benchmark_report="$ci_root/jenkins-benchmark-report.json"

  mkdir -p "$ci_root"

  verify_main_branch_protection_contract

  validate_semantic_pr_title "$pr_title" "$ci_root/semantic-pr-title.txt"

  bash -n ops/jenkins/deploy_remote.sh
  bash -n ops/jenkins/controller/support/run_pr_ci.sh
  bash -n scripts/ci/run_jenkins_pr_ci.sh
  python3 -m py_compile ops/jenkins/router/router.py
  python3 -m json.tool < ops/jenkins/router/repositories.json >/dev/null

  if compgen -G "ops/jenkins/router/test_*.py" >/dev/null; then
    python3 -m unittest discover -s ops/jenkins/router -p 'test_*.py'
  fi

  bash scripts/ci/validate_cflite_toolchain_pin.sh

  CARGO_TARGET_DIR="$target_dir" cargo fmt --all -- --check
  CARGO_TARGET_DIR="$target_dir" cargo check --workspace --all-targets --locked
  CARGO_TARGET_DIR="$target_dir" cargo test --workspace --locked
  CARGO_TARGET_DIR="$target_dir" cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  CARGO_TARGET_DIR="$target_dir" cargo +1.92.0 check --workspace --all-targets --locked

  CARGO_TARGET_DIR="$target_dir" \
    bash scripts/ci/run_terminal_system_suite.sh \
    "$system_suite_report" \
    --benchmark-report "$benchmark_report" \
    --governance-mode ci

  CARGO_TARGET_DIR="$target_dir" cargo audit
  CARGO_TARGET_DIR="$target_dir" cargo deny check bans licenses advisories sources

  CARGO_TARGET_DIR="$target_dir" bash scripts/mvp/run_matrix.sh 3

  CARGO_TARGET_DIR="$target_dir" \
    cargo +"$fuzz_toolchain" fuzz run parser_feed -- -max_total_time="$fuzz_seconds"
}

run_codeql_suite() {
  local root="$1"
  local target_dir="${2:-$(default_target_dir codeql)}"
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

  prepare_clean_codeql_workspace "$target_dir"

  mkdir -p "$codeql_root"
  rm -rf "$database_root" "$diagnostics_root" "$sarif_path"

  codeql database create "$database_root" \
    --language=rust \
    --source-root "$root_dir" \
    --command "CARGO_TARGET_DIR=$target_dir cargo build --workspace --locked"

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

run_extended_suite() {
  local root="$1"
  local target_dir="${2:-$(default_target_dir extended)}"
  local extended_root="$root/extended"
  local benchmark_report="$extended_root/jenkins-extended-benchmark-report.json"

  mkdir -p "$extended_root"

  verify_main_branch_protection_contract

  ensure_cargo_tool cargo-machete cargo-machete "$cargo_machete_version"
  ensure_cargo_tool cargo-udeps cargo-udeps "$cargo_udeps_version"

  CARGO_TARGET_DIR="$target_dir" \
    RLDYOURTERM_UDEPS_TOOLCHAIN="$fuzz_toolchain" \
    bash scripts/ci/run_dead_weight_checks.sh extended

  CARGO_TARGET_DIR="$target_dir" \
    env TERMINAL_BENCHMARK_SCALE="$extended_benchmark_scale" \
    bash scripts/ci/run_terminal_benchmark_full.sh "$benchmark_report"

  CARGO_TARGET_DIR="$target_dir" bash scripts/ci/run_e2e_governance.sh --mode release
  CARGO_TARGET_DIR="$target_dir" bash scripts/mvp/run_matrix.sh "$extended_matrix_repeat"
  CARGO_TARGET_DIR="$target_dir" \
    cargo +"$fuzz_toolchain" fuzz run parser_feed -- -max_total_time="$extended_fuzz_seconds"
}

run_scorecard_suite() {
  local root="$1"
  local scorecard_root="$root/scorecard"
  local commit_ref="${pr_checkout_sha:-${pr_head_sha:-HEAD}}"

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
    run_ci_suite "$report_root" "$(default_target_dir ci)"
    ;;
  codeql)
    run_codeql_suite "$report_root" "$(default_target_dir codeql)"
    ;;
  scorecard)
    run_scorecard_suite "$report_root"
    ;;
  extended)
    run_extended_suite "$report_root" "$(default_target_dir extended)"
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
