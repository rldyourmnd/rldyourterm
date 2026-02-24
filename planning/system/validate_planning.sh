#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

errors=0

fail() {
  echo "[FAIL] $1"
  errors=$((errors + 1))
}

pass() {
  echo "[PASS] $1"
}

require_file() {
  local file="$1"
  if [[ -f "$file" ]]; then
    pass "file exists: $file"
  else
    fail "missing file: $file"
  fi
}

echo "Validating planning knowledge system..."

authoritative_paths=(
  "planning/discovery"
  "planning/adr"
  "planning/architecture"
  "planning/stack"
  "planning/quality"
  "planning/risk"
  "planning/roadmap"
  "planning/operations"
  "planning/settings"
  "planning/v1.0.0-development-blueprint.md"
  "metrics/version/1.0.0"
)

required_files=(
  "planning/README.md"
  "planning/system/README.md"
  "planning/system/source-of-truth-and-precedence-v1.0.0.md"
  "planning/system/traceability-matrix-v1.0.0.md"
  "planning/system/codex-session-playbook-v1.0.0.md"
  "planning/system/dependency-evidence-context7-v1.0.0.md"
  "planning/system/gap-closure-register-v1.0.0.md"
  "planning/system/planning-validation-checklist-v1.0.0.md"
  "planning/discovery/v1.0.0-answer-lock.md"
  "planning/quality/v1.0.0-quality-gates.md"
  "planning/operations/v1.0.0-manual-test-plan.md"
)

for f in "${required_files[@]}"; do
  require_file "$f"
done

if rg -n "render fps target 120|performance fps-target <hz>|1h test window|Run sustained output for at least 30 minutes" "${authoritative_paths[@]}" >/tmp/planning_bad_patterns.txt 2>/dev/null; then
  fail "legacy inconsistent patterns found (see /tmp/planning_bad_patterns.txt)"
else
  pass "no legacy inconsistent fps/long-run patterns"
fi

if rg -n "monitor-driven|refresh-rate|render cadence monitor-auto" planning metrics >/tmp/planning_monitor_patterns.txt 2>/dev/null; then
  pass "monitor-driven cadence policy is present"
else
  fail "monitor-driven cadence policy markers are missing"
fi

if rg -n "TODO|TBD|XXX" "${authoritative_paths[@]}" >/tmp/planning_placeholders.txt 2>/dev/null; then
  fail "unresolved placeholders found (see /tmp/planning_placeholders.txt)"
else
  pass "no unresolved TODO/TBD/XXX placeholders"
fi

for req in R-01 R-02 R-03 R-04 R-05 R-06 R-07 R-08 R-09 R-10 R-11 R-12; do
  if rg -q "$req" planning/system/traceability-matrix-v1.0.0.md; then
    pass "traceability row exists: $req"
  else
    fail "traceability row missing: $req"
  fi
done

if [[ $errors -gt 0 ]]; then
  echo "Validation completed with $errors error(s)."
  exit 1
fi

echo "Validation completed successfully."
