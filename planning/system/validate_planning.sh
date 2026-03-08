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

info() {
  printf '[INFO] %s\n' "$1"
}

RG_AVAILABLE=0
if [[ "${VALIDATE_PLANNING_FORCE_GREP:-0}" == "1" ]]; then
  info "forcing grep fallback for planning validation checks"
elif command -v rg >/dev/null 2>&1; then
  RG_AVAILABLE=1
else
  info "ripgrep (rg) is not available; falling back to grep-compatible checks"
fi

search_pattern() {
  local pattern="$1"
  shift
  if [[ $RG_AVAILABLE -eq 1 ]]; then
    rg -n "$pattern" "$@"
  else
    grep -R -nE "$pattern" "$@"
  fi
}

search_pattern_rs() {
  local pattern="$1"
  local path="$2"
  if [[ $RG_AVAILABLE -eq 1 ]]; then
    rg -n "$pattern" "$path" --glob '*.rs'
  else
    grep -R -nE --include='*.rs' "$pattern" "$path"
  fi
}

search_pattern_excluding_validate_script() {
  local pattern="$1"
  shift
  if [[ $RG_AVAILABLE -eq 1 ]]; then
    rg -n "$pattern" "$@" --glob '!planning/system/validate_planning.sh'
  else
    grep -R -nE "$pattern" "$@" --exclude='validate_planning.sh'
  fi
}

extract_req_ids() {
  local pattern="$1"
  shift
  if [[ $RG_AVAILABLE -eq 1 ]]; then
    rg -o --no-filename "$pattern" "$@"
  else
    grep -h -oE "$pattern" "$@"
  fi
}

filter_unexpected_req_ids() {
  local allowed_pattern="$1"
  local file="$2"
  if [[ $RG_AVAILABLE -eq 1 ]]; then
    rg -v "$allowed_pattern" "$file"
  else
    grep -Ev "$allowed_pattern" "$file"
  fi
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
  "planning/quality/v1.0.0-acceptance-matrix.md"
  "planning/operations/v1.0.0-manual-test-plan.md"
  "planning/operations/v1.0.0-release-pack.md"
  "planning/operations/v1.0.0-start-readiness-index.md"
  "planning/operations/v1.0.0-evidence-manifest.json"
  "scripts/ci/validate_release_evidence_freshness.sh"
  "scripts/ci/validate_vsa_dependency_graph.sh"
  "scripts/ci/run_e2e_governance.sh"
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
  "planning/operations/v1.0.0-release-pack.md"
  "planning/operations/v1.0.0-start-readiness-index.md"
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

if search_pattern "render fps target 120|performance fps-target <hz>|1h test window|Run sustained output for at least 30 minutes" "${authoritative_paths[@]}" >/tmp/planning_bad_patterns.txt 2>/dev/null; then
  fail "legacy inconsistent fps/long-run patterns still present (see /tmp/planning_bad_patterns.txt)"
else
  pass "no legacy inconsistent fps/long-run patterns"
fi

manifest_anchor_literal="planning/operations/v1.0.0-evidence-manifest.json"
manifest_anchor_docs=(
  "planning/operations/v1.0.0-release-pack.md"
  "planning/operations/v1.0.0-start-readiness-index.md"
  "planning/system/traceability-matrix-v1.0.0.md"
  "planning/quality/v1.0.0-quality-gates.md"
  "planning/quality/v1.0.0-acceptance-matrix.md"
)
for doc in "${manifest_anchor_docs[@]}"; do
  if search_pattern "$manifest_anchor_literal" "$doc" >/dev/null 2>&1; then
    pass "manifest anchor is present: $doc"
  else
    fail "manifest anchor is missing from $doc"
  fi
done

if search_pattern "CURRENT RUN" planning/operations/v1.0.0-release-pack.md planning/operations/v1.0.0-start-readiness-index.md >/tmp/planning_current_run_claims.txt 2>/dev/null; then
  fail "forbidden bare CURRENT RUN claims remain in release/start readiness docs (see /tmp/planning_current_run_claims.txt)"
else
  pass "no forbidden CURRENT RUN claims in release/start readiness docs"
fi

if search_pattern "monitor-driven|refresh-rate|render cadence monitor-auto" AGENTS.md planning README.md >/tmp/planning_monitor_patterns.txt 2>/dev/null; then
  pass "monitor-driven cadence policy is present"
else
  fail "monitor-driven cadence policy markers are missing"
fi

if search_pattern "^\| G-010 \|.*\| Closed \|$" planning/system/gap-closure-register-v1.0.0.md >/dev/null 2>&1; then
  pass "foundation window ownership gap is closed in gap register (G-010)"
else
  fail "G-010 must be closed with explicit evidence in gap register"
fi

if search_pattern "No free-form command-line input inside GUI/TTY palette UI" planning/settings/settings_palette.md >/dev/null 2>&1; then
  pass "settings palette scope explicitly forbids free-form UI command line"
else
  fail "settings palette scope does not explicitly forbid free-form UI command line"
fi

if search_pattern "arboard::" crates/app/src >/tmp/planning_arch_clipboard_direct.txt 2>/dev/null; then
  fail "direct arboard usage detected in app runtime; clipboard path must remain adapter-based (see /tmp/planning_arch_clipboard_direct.txt)"
else
  pass "no direct clipboard integration detected in app runtime"
fi

if search_pattern_rs "\bwindow\s*\.\s*(request_redraw|set_title|current_monitor)\s*\(" crates/app/src >/tmp/planning_arch_drift_window_direct.txt 2>/dev/null; then
  fail "direct app-owned window control usage detected in app runtime (see /tmp/planning_arch_drift_window_direct.txt)"
else
  pass "no direct app-owned window control usage detected in app runtime"
fi

if search_pattern_excluding_validate_script "TODO|TBD|XXX" AGENTS.md planning README.md >/tmp/planning_todo_patterns.txt 2>/dev/null; then
  fail "unresolved placeholders found (see /tmp/planning_todo_patterns.txt)"
else
  pass "no unresolved TODO/TBD/XXX placeholders"
fi

for req in R-01 R-02 R-03 R-04 R-05 R-06 R-07 R-08 R-09 R-10 R-11 R-12 R-13 R-14; do
  if search_pattern "\| ${req} \|" planning/system/traceability-matrix-v1.0.0.md >/dev/null 2>&1; then
    pass "traceability row exists: ${req}"
  else
    fail "missing traceability row: ${req}"
  fi
done

extract_req_ids 'R-[0-9]{2}' "${authoritative_req_paths[@]}" 2>/dev/null | sort -u >/tmp/planning_req_ids_all.txt || true
filter_unexpected_req_ids '^(R-0[1-9]|R-1[0-4])$' /tmp/planning_req_ids_all.txt >/tmp/planning_unexpected_req_ids.txt || true

if [[ -s /tmp/planning_unexpected_req_ids.txt ]]; then
  unexpected_req_ids="$(xargs </tmp/planning_unexpected_req_ids.txt)"
  unexpected_req_pattern="$(paste -sd'|' /tmp/planning_unexpected_req_ids.txt)"
  search_pattern "${unexpected_req_pattern}" "${authoritative_req_paths[@]}" >/tmp/planning_unexpected_req_ids_refs.txt 2>/dev/null || true
  fail "unexpected Req IDs in authoritative planning docs: ${unexpected_req_ids} (see /tmp/planning_unexpected_req_ids_refs.txt)"
else
  pass "authoritative planning Req IDs are restricted to R-01..R-14"
fi

if [[ $errors -gt 0 ]]; then
  echo "Validation completed with ${errors} error(s)."
  exit 1
fi

echo "Validation completed successfully with no errors."
