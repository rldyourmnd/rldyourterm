#!/usr/bin/env bash
set -euo pipefail

# Refresh a benchmark baseline JSON file from a validated benchmark report.
#
# Equivalent to the former refresh_terminal_benchmark_baseline.py script.
# Requires: jq, cargo (with rldyourterm-terminal-benchmark buildable).

usage() {
  cat >&2 <<USAGE
usage: $0 <report> <output> [options]

positional arguments:
  report                  path to the benchmark report JSON
  output                  path to write the generated baseline JSON

options:
  --comparison-mode MODE  enforced or advisory (overrides suite default)
  --environment-scope SC  environment scope (overrides suite default)
  --notes TEXT            custom notes string
USAGE
  exit 2
}

fail() {
  echo "benchmark baseline refresh failed: $1" >&2
  exit 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "required command not found: $1" >&2
    exit 1
  fi
}

require_command jq
require_command cargo

if [[ $# -lt 2 ]]; then
  usage
fi

report_path="$1"
output_path="$2"
shift 2

comparison_mode_override=""
environment_scope_override=""
notes_override=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --comparison-mode)
      [[ $# -ge 2 ]] || usage
      comparison_mode_override="$2"; shift 2 ;;
    --environment-scope)
      [[ $# -ge 2 ]] || usage
      environment_scope_override="$2"; shift 2 ;;
    --notes)
      [[ $# -ge 2 ]] || usage
      notes_override="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; usage ;;
  esac
done

if [[ ! -f "$report_path" ]]; then
  fail "report file not found: $report_path"
fi

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Load the report JSON.
report_json="$(< "$report_path")"

suite="$(jq -r '.suite // empty' <<<"$report_json")"
if [[ -z "$suite" ]]; then
  fail "report does not contain a suite field"
fi

# Suite defaults (mirrors the Python DEFAULTS_BY_SUITE).
case "$suite" in
  canonical-headless)
    default_comparison_mode="enforced"
    default_environment_scope="portable-headless"
    defaults_json=$(jq -n '{
      max_mean_nanos_ratio: 2.5,
      max_p95_nanos_ratio: 3.0,
      min_primary_units_per_second_ratio: 0.40,
      min_bytes_per_second_ratio: 0.40
    }')
    ;;
  live-display)
    default_comparison_mode="advisory"
    default_environment_scope="local-display-session"
    defaults_json=$(jq -n '{
      max_mean_nanos_ratio: 3.0,
      max_p95_nanos_ratio: 3.5,
      min_primary_units_per_second_ratio: 0.35
    }')
    ;;
  *)
    fail "unsupported suite '${suite}'"
    ;;
esac

# Resolve effective comparison_mode and environment_scope.
comparison_mode="${comparison_mode_override:-$default_comparison_mode}"
environment_scope="${environment_scope_override:-$default_environment_scope}"

# Load environment snapshot from the Rust binary.
env_snapshot_output=$(
  cd "$REPO_ROOT" && \
  cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
    environment snapshot --report "$report_path" 2>&1
) || fail "environment snapshot generation failed"

env_snapshot_json="$env_snapshot_output"

# Validate that the output is valid JSON.
if ! jq empty <<<"$env_snapshot_json" 2>/dev/null; then
  fail "environment snapshot is not valid JSON"
fi

report_environment_scope="$(jq -r '.environment_scope // empty' <<<"$env_snapshot_json")"
if [[ -z "$report_environment_scope" ]]; then
  fail "environment snapshot must include a non-empty environment_scope"
fi

if [[ "$environment_scope" != "$report_environment_scope" ]]; then
  fail "requested environment_scope is incompatible with the benchmark report: requested='${environment_scope}' report='${report_environment_scope}'"
fi

# Extract environment_requirements for controlled-display-session.
environment_requirements="null"
if [[ "$environment_scope" == "controlled-display-session" ]]; then
  environment_requirements="$(jq '.environment_requirements // null' <<<"$env_snapshot_json")"
  if [[ "$environment_requirements" == "null" ]]; then
    fail "controlled-display-session baseline refresh requires a controlled live-display report"
  fi
fi

# Validate results array.
results_count="$(jq '.results | if type == "array" then length else -1 end' <<<"$report_json")"
if [[ "$results_count" -le 0 ]]; then
  fail "report.results must be a non-empty list"
fi

# Build scenarios object from report results.
# Each entry: { "scenario_name": { "baseline_metrics": {...}, "thresholds": {} } }
scenarios_json="$(jq -c '
  [.results[] |
    if type != "object" then error("report.results entries must be objects") else . end |
    .scenario as $s |
    if ($s | type) != "string" or ($s | length) == 0
      then error("report result scenario must be a non-empty string") else . end |
    .stats as $stats |
    if ($stats | type) != "object"
      then error("report result \($s) must contain stats") else . end |
    {
      key: $s,
      value: {
        baseline_metrics: (
          {
            mean_nanos: $stats.mean_nanos,
            p95_nanos: $stats.p95_nanos,
            primary_units_per_second: .primary_units_per_second
          } + (
            if (.bytes_per_second | type) == "number" and (.bytes_per_second > 0)
            then { bytes_per_second: .bytes_per_second }
            else {}
            end
          )
        ),
        thresholds: {}
      }
    }
  ] | from_entries
' <<<"$report_json")" || fail "failed to extract scenarios from report"

# Build the generated_at_utc timestamp.
generated_at_utc="$(date -u +"%Y-%m-%dT%H:%M:%S+00:00")"

# Determine notes.
if [[ -n "$notes_override" ]]; then
  notes="$notes_override"
elif [[ "$suite" == "canonical-headless" ]]; then
  notes="Generated from canonical full benchmark report. Update only after intentional performance-baseline review."
elif [[ "$environment_scope" == "controlled-display-session" ]]; then
  notes="Generated from a controlled live-display benchmark report with monitor-aware cadence. Use only for calibrated controlled-display validation."
else
  notes="Generated from local live-display benchmark report. Advisory only unless calibrated for a controlled display environment."
fi

# Assemble the final baseline JSON payload.
benchmark_tool="$(jq -r '.benchmark_tool // null' <<<"$report_json")"
scale="$(jq '.scale // null' <<<"$report_json")"
scenario_selection="$(jq '.scenario_selection // null' <<<"$report_json")"

payload_json="$(jq -n \
  --arg baseline_tool "terminal-benchmark-thresholds" \
  --arg benchmark_tool "$benchmark_tool" \
  --arg suite "$suite" \
  --argjson scale "$scale" \
  --arg comparison_mode "$comparison_mode" \
  --arg environment_scope "$environment_scope" \
  --arg generated_at_utc "$generated_at_utc" \
  --argjson scenario_selection "$scenario_selection" \
  --argjson defaults "$defaults_json" \
  --argjson environment_requirements "$environment_requirements" \
  --arg notes "$notes" \
  --argjson scenarios "$scenarios_json" \
  '{
    baseline_tool: $baseline_tool,
    benchmark_tool: $benchmark_tool,
    suite: $suite,
    scale: $scale,
    comparison_mode: $comparison_mode,
    environment_scope: $environment_scope,
    generated_at_utc: $generated_at_utc,
    scenario_selection: $scenario_selection,
    defaults: $defaults,
    environment_requirements: $environment_requirements,
    notes: $notes,
    scenarios: $scenarios
  }'
)"

# Ensure parent directory exists.
output_dir="$(dirname "$output_path")"
if [[ -n "$output_dir" ]] && [[ "$output_dir" != "." ]]; then
  mkdir -p "$output_dir"
fi

# Write with trailing newline (matches Python json.dumps + "\n").
echo "$payload_json" | jq --indent 2 '.' > "$output_path"

echo "benchmark baseline refreshed: $output_path"
