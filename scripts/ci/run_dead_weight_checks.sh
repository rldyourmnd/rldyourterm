#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: run_dead_weight_checks.sh <oss|extended>
USAGE
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "required command not found: $name" >&2
    exit 1
  fi
}

run_cargo_machete() {
  require_command cargo
  require_command cargo-machete

  cargo machete
}

run_cargo_udeps() {
  local toolchain="${RLDYOURTERM_UDEPS_TOOLCHAIN:-nightly-2026-03-11}"

  require_command cargo
  require_command rustup
  require_command cargo-udeps

  rustup component add --toolchain "$toolchain" rust-src rustc-dev llvm-tools-preview
  cargo +"$toolchain" udeps --workspace --all-targets --all-features
}

mode="${1:-}"

case "$mode" in
  oss)
    run_cargo_machete
    ;;
  extended)
    run_cargo_machete
    run_cargo_udeps
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
