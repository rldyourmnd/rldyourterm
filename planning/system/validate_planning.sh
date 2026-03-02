#!/usr/bin/env bash
set -euo pipefail

errors=0

pass() {
  printf '[PASS] %s\n' "$1"
}

fail() {
  printf '[FAIL] %s\n' "$1"
  errors=$((errors + 1))
}

required_paths=(
  "AGENTS.md"
  "README.md"
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

echo "Validating planning knowledge system..."

for path in "${required_paths[@]}"; do
  if [[ -e "$path" ]]; then
    pass "file exists: $path"
  else
    fail "missing required path: $path"
  fi
done

authoritative_paths=(
  "AGENTS.md"
  "README.md"
  "planning/discovery/v1.0.0-answer-lock.md"
  "planning/quality/v1.0.0-quality-gates.md"
  "planning/quality/v1.0.0-acceptance-matrix.md"
  "planning/operations/v1.0.0-manual-test-plan.md"
  "planning/system/traceability-matrix-v1.0.0.md"
  "planning/system/source-of-truth-and-precedence-v1.0.0.md"
)

authoritative_req_paths=(
  "planning/discovery/v1.0.0-answer-lock.md"
  "planning/quality/v1.0.0-quality-gates.md"
  "planning/quality/v1.0.0-acceptance-matrix.md"
  "planning/operations/v1.0.0-manual-test-plan.md"
  "planning/system/traceability-matrix-v1.0.0.md"
  "planning/system/source-of-truth-and-precedence-v1.0.0.md"
)

if rg -n "render fps target 120|performance fps-target <hz>|1h test window|Run sustained output for at least 30 minutes" "${authoritative_paths[@]}" >/tmp/planning_bad_patterns.txt 2>/dev/null; then
  fail "legacy inconsistent fps/long-run patterns still present (see /tmp/planning_bad_patterns.txt)"
else
  pass "no legacy inconsistent fps/long-run patterns"
fi

if rg -n "monitor-driven|refresh-rate|render cadence monitor-auto" AGENTS.md planning README.md >/tmp/planning_monitor_patterns.txt 2>/dev/null; then
  pass "monitor-driven cadence policy is present"
else
  fail "monitor-driven cadence policy markers are missing"
fi

if rg -n "TODO|TBD|XXX" AGENTS.md planning README.md --glob '!planning/system/validate_planning.sh' >/tmp/planning_todo_patterns.txt 2>/dev/null; then
  fail "unresolved placeholders found (see /tmp/planning_todo_patterns.txt)"
else
  pass "no unresolved TODO/TBD/XXX placeholders"
fi

for req in R-01 R-02 R-03 R-04 R-05 R-06 R-07 R-08 R-09 R-10 R-11 R-12 R-13 R-14; do
  if rg -n "\| ${req} \|" planning/system/traceability-matrix-v1.0.0.md >/dev/null 2>&1; then
    pass "traceability row exists: ${req}"
  else
    fail "missing traceability row: ${req}"
  fi
done

rg -o --no-filename 'R-[0-9]{2}' "${authoritative_req_paths[@]}" 2>/dev/null | sort -u >/tmp/planning_req_ids_all.txt || true
rg -v '^(R-0[1-9]|R-1[0-4])$' /tmp/planning_req_ids_all.txt >/tmp/planning_unexpected_req_ids.txt || true

if [[ -s /tmp/planning_unexpected_req_ids.txt ]]; then
  unexpected_req_ids="$(xargs </tmp/planning_unexpected_req_ids.txt)"
  unexpected_req_pattern="$(paste -sd'|' /tmp/planning_unexpected_req_ids.txt)"
  rg -n "${unexpected_req_pattern}" "${authoritative_req_paths[@]}" >/tmp/planning_unexpected_req_ids_refs.txt 2>/dev/null || true
  fail "unexpected Req IDs in authoritative planning docs: ${unexpected_req_ids} (see /tmp/planning_unexpected_req_ids_refs.txt)"
else
  pass "authoritative planning Req IDs are restricted to R-01..R-14"
fi

if [[ $errors -gt 0 ]]; then
  echo "Validation completed with ${errors} error(s)."
  exit 1
fi

echo "Validation completed successfully with no errors."
