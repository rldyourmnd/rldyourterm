#!/usr/bin/env bash
set -euo pipefail

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "[FAIL] missing required authority file: $path" >&2
    exit 1
  fi
}

for path in \
  AGENTS.md \
  CLAUDE.md \
  README.md \
  .serena/memories/CORE_00_overview.md \
  .serena/memories/CORE_01_conventions.md \
  .serena/memories/CORE_02_commands.md \
  .serena/memories/CORE_03_dependency_status.md \
  .serena/memories/CORE_04_current_state_and_risks.md \
  .serena/memories/BACKEND_01_services.md
do
  require_file "$path"
  if ! git ls-files --error-unmatch "$path" >/dev/null 2>&1; then
    echo "[FAIL] authority file must be tracked in git: $path" >&2
    exit 1
  fi
done

reject_fixed() {
  local needle="$1"
  shift
  if rg -n -F "$needle" "$@" >/dev/null; then
    echo "[FAIL] stale authority-doc claim detected: $needle" >&2
    rg -n -F "$needle" "$@" >&2
    exit 1
  fi
}

reject_fixed 'crates/app/src/shared.rs' CLAUDE.md
reject_fixed 'Modules organized as single-file crates (lib.rs per feature crate)' .serena/memories/CORE_01_conventions.md
reject_fixed 'app -> {ui, services, core, foundation, foundation-platform, features/*}' .serena/memories/CORE_00_overview.md
reject_fixed 'push/PR to `main` and `dev`' .serena/memories/CORE_02_commands.md .serena/memories/CORE_03_dependency_status.md
reject_fixed 'rustsec/audit-check@v2' .serena/memories/CORE_02_commands.md .serena/memories/CORE_03_dependency_status.md
reject_fixed 'planning validation' README.md
reject_fixed 'planning + `scripts/mvp/*` flow' .serena/memories/CORE_04_current_state_and_risks.md

python3 - <<'PY'
from pathlib import Path
import sys

release = Path(".github/workflows/release.yml").read_text(encoding="utf-8")
required_counts = {
    "MSRV_RUST_TOOLCHAIN:": 1,
    "rustup toolchain install \"${MSRV_RUST_TOOLCHAIN}\" --profile minimal": 1,
    "bash scripts/ci/run_terminal_system_suite.sh": 1,
    "--benchmark-report target/terminal-benchmark/release-benchmark-report.json": 1,
    "--governance-mode release": 1,
}
for needle, expected in required_counts.items():
    actual = release.count(needle)
    if actual != expected:
        print(
            f"[FAIL] release workflow invariant mismatch for {needle!r}: expected {expected}, found {actual}",
            file=sys.stderr,
        )
        sys.exit(1)

ci = Path(".github/workflows/ci.yml").read_text(encoding="utf-8")
if ci.count("branches: [main]") < 2:
    print("[FAIL] ci workflow must remain scoped to main push and PR triggers", file=sys.stderr)
    sys.exit(1)
if ci.count("bash scripts/ci/run_terminal_benchmark_smoke.sh") != 1:
    print("[FAIL] ci workflow must keep exactly one benchmark smoke gate", file=sys.stderr)
    sys.exit(1)
if "bash scripts/ci/run_terminal_system_suite.sh" in ci:
    print("[FAIL] ci workflow must not inline the full terminal system suite", file=sys.stderr)
    sys.exit(1)

claude = Path("CLAUDE.md").read_text(encoding="utf-8")
if "runtime_shared/" not in claude:
    print("[FAIL] CLAUDE.md must document runtime_shared entry points", file=sys.stderr)
    sys.exit(1)
PY

echo "[PASS] authority docs and workflow invariants are synchronized"
