#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_terminal_system_suite.sh [report-path] [--governance-mode <ci|release>] [--benchmark-report <path>]
USAGE
}

report_path="target/terminal-benchmark/system-suite-report.json"
benchmark_report_path=""
governance_mode="ci"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --governance-mode)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --governance-mode" >&2
        usage
        exit 2
      fi
      governance_mode="$2"
      shift 2
      ;;
    --benchmark-report)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --benchmark-report" >&2
        usage
        exit 2
      fi
      benchmark_report_path="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --*)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
    *)
      report_path="$1"
      shift
      ;;
  esac
done

case "$governance_mode" in
  ci|release) ;;
  *)
    echo "invalid governance mode: $governance_mode" >&2
    usage
    exit 2
    ;;
esac

if [[ -z "$benchmark_report_path" ]]; then
  benchmark_report_path="${report_path%.json}.benchmark.json"
fi

mkdir -p "$(dirname "$report_path")" "$(dirname "$benchmark_report_path")"

quality_gates=(
  "cargo fmt --all -- --check"
  "cargo check --workspace --all-targets --locked"
  "cargo test --workspace --locked"
  "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings"
  "cargo +1.92.0 check --workspace --all-targets --locked"
  "cargo check --manifest-path fuzz/Cargo.toml --locked"
  "bash scripts/ci/run_terminal_benchmark_smoke.sh"
  "bash scripts/ci/run_terminal_benchmark_full.sh $benchmark_report_path"
  "bash scripts/ci/run_e2e_governance.sh --mode $governance_mode"
)

cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.92.0 check --workspace --all-targets --locked
cargo check --manifest-path fuzz/Cargo.toml --locked
bash scripts/ci/run_terminal_benchmark_smoke.sh
bash scripts/ci/run_terminal_benchmark_full.sh "$benchmark_report_path"
bash scripts/ci/run_e2e_governance.sh --mode "$governance_mode"

python3 - "$report_path" "$benchmark_report_path" "$governance_mode" "${quality_gates[@]}" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

report_path = pathlib.Path(sys.argv[1])
benchmark_report_path = pathlib.Path(sys.argv[2])
governance_mode = sys.argv[3]
quality_gates = sys.argv[4:]

payload = {
    "system_tool": "terminal-system-suite",
    "status": "pass",
    "generated_at_utc": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    "governance_mode": governance_mode,
    "benchmark_report": str(benchmark_report_path),
    "quality_gates": quality_gates,
}

report_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY

python3 scripts/ci/validate_terminal_system_suite_report.py \
  "$report_path" \
  --benchmark-report "$benchmark_report_path" \
  --governance-mode "$governance_mode"

echo "terminal system suite ok: $report_path"
