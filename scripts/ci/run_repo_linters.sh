#!/usr/bin/env bash
set -euo pipefail

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "required command not found: $name" >&2
    exit 1
  fi
}

require_command actionlint
require_command shellcheck
require_command python3
require_command rg

actionlint -config-file actionlint.yaml

mapfile -t shell_files < <(rg --files -g '*.sh' scripts/ci scripts/mvp ops/jenkins)
if [[ "${#shell_files[@]}" -eq 0 ]]; then
  echo "no shell files matched repo lint scope" >&2
  exit 1
fi

shellcheck --severity=warning -x "${shell_files[@]}"

mapfile -t python_files < <(rg --files -g '*.py' scripts/ci ops/jenkins/router)
if [[ "${#python_files[@]}" -gt 0 ]]; then
  python3 -m py_compile "${python_files[@]}"
fi
