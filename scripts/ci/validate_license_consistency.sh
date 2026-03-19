#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT_DIR"

fail() {
  echo "license consistency validation failed: $*" >&2
  exit 1
}

require_pattern() {
  local file="$1"
  local pattern="$2"
  local description="$3"
  if ! rg -q --multiline "$pattern" "$file"; then
    fail "$description ($file)"
  fi
}

forbid_pattern() {
  local pattern="$1"
  shift
  local scope=("$@")
  if rg -n "$pattern" "${scope[@]}" >/tmp/rldyourterm-license-check.out 2>/dev/null; then
    cat /tmp/rldyourterm-license-check.out >&2
    rm -f /tmp/rldyourterm-license-check.out
    fail "forbidden pattern '$pattern' found"
  fi
  rm -f /tmp/rldyourterm-license-check.out
}

require_pattern "LICENSE" "GNU Affero General Public License" \
  "LICENSE must contain the GNU Affero General Public License text"
require_pattern "Cargo.toml" '^license = "AGPL-3\.0-or-later"$' \
  "workspace root must declare AGPL-3.0-or-later"
require_pattern "fuzz/Cargo.toml" '^license = "AGPL-3\.0-or-later"$' \
  "fuzz crate must declare AGPL-3.0-or-later"
require_pattern "deny.toml" '"AGPL-3\.0-or-later"' \
  "cargo-deny allowlist must include AGPL-3.0-or-later"
require_pattern "README.md" 'AGPL-3\.0-or-later' \
  "README must mention AGPL-3.0-or-later"

while IFS= read -r manifest; do
  require_pattern "$manifest" '^license\.workspace = true$' \
    "workspace member must inherit license from workspace.package"
done < <(find crates terminal_benchmark -name Cargo.toml | sort)

while IFS= read -r file; do
  require_pattern "$file" 'SPDX-License-Identifier:\s*AGPL-3\.0-or-later' \
    "SPDX-tagged first-party file must use AGPL-3.0-or-later"
done < <(
  rg --files \
    crates \
    fuzz \
    terminal_benchmark \
    ops/jenkins \
    scripts/ci \
    scripts/mvp \
    .github/workflows \
  | while IFS= read -r path; do
      if rg -q 'SPDX-License-Identifier:' "$path"; then
        printf '%s\n' "$path"
      fi
    done
)

forbid_pattern 'GPL-3\.0-only' \
  README.md \
  Cargo.toml \
  deny.toml \
  crates \
  fuzz \
  terminal_benchmark \
  scripts \
  ops \
  .github

echo "LICENSE_CONSISTENCY_RESULT status=pass license=AGPL-3.0-or-later"
