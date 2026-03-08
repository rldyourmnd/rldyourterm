#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_e2e_governance.sh [--mode <ci|release>] [--with-matrix]

Modes:
  ci      - VSA dependency graph + policy-level checks.
  release - ci mode + extended validation.

Flags:
  --with-matrix  additionally runs `bash scripts/mvp/run_matrix.sh 3`.
USAGE
}

mode="ci"
with_matrix="0"

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
    --with-matrix)
      with_matrix="1"
      shift
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
  ci|release) ;;
  *)
    echo "invalid mode: $mode (expected ci|release)" >&2
    exit 2
    ;;
esac

echo "GOVERNANCE_E2E_START mode=$mode with_matrix=$with_matrix"

echo "GOVERNANCE_E2E_STEP step=vsa-dependency-graph"
bash scripts/ci/validate_vsa_dependency_graph.sh

if [[ "$with_matrix" == "1" ]]; then
  echo "GOVERNANCE_E2E_STEP step=compatibility-matrix repeat=3"
  bash scripts/mvp/run_matrix.sh 3
fi

echo "GOVERNANCE_E2E_RESULT status=pass mode=$mode with_matrix=$with_matrix"
