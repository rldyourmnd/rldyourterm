#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_e2e_governance.sh [--mode <ci|release>] [--manifest <path>] [--with-matrix]

Modes:
  ci      - planning + dependency graph + policy-level evidence contract checks.
  release - ci mode + strict HEAD-bound evidence/artifact freshness checks.

Flags:
  --with-matrix  additionally runs `bash scripts/mvp/run_matrix.sh 3`.
USAGE
}

mode="ci"
manifest_path="planning/operations/v1.0.0-evidence-manifest.json"
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
    --manifest)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --manifest" >&2
        usage
        exit 2
      fi
      manifest_path="$2"
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

echo "GOVERNANCE_E2E_START mode=$mode manifest=$manifest_path with_matrix=$with_matrix"

echo "GOVERNANCE_E2E_STEP step=planning-validation"
bash planning/system/validate_planning.sh

echo "GOVERNANCE_E2E_STEP step=vsa-dependency-graph"
bash scripts/ci/validate_vsa_dependency_graph.sh

if [[ "$with_matrix" == "1" ]]; then
  echo "GOVERNANCE_E2E_STEP step=compatibility-matrix repeat=3"
  bash scripts/mvp/run_matrix.sh 3
  if [[ "$mode" == "release" ]]; then
    echo "GOVERNANCE_E2E_STEP step=refresh-evidence-manifest"
    python3 scripts/ci/refresh_release_evidence_manifest.py --output "$manifest_path"
  fi
fi

if [[ "$mode" == "release" ]]; then
  echo "GOVERNANCE_E2E_STEP step=evidence-freshness mode=strict"
  bash scripts/ci/validate_release_evidence_freshness.sh --manifest "$manifest_path" --mode strict
else
  echo "GOVERNANCE_E2E_STEP step=evidence-freshness mode=policy"
  bash scripts/ci/validate_release_evidence_freshness.sh --manifest "$manifest_path" --mode policy
fi

echo "GOVERNANCE_E2E_RESULT status=pass mode=$mode manifest=$manifest_path with_matrix=$with_matrix"
