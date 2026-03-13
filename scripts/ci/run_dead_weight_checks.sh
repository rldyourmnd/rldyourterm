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

ensure_rustup_component() {
  local toolchain="$1"
  local component="$2"

  if rustup component list --toolchain "$toolchain" --installed | grep -Fqx "$component"; then
    return 0
  fi

  rustup component add --toolchain "$toolchain" "$component"
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

  ensure_rustup_component "$toolchain" rust-src
  ensure_rustup_component "$toolchain" rustc-dev
  ensure_rustup_component "$toolchain" llvm-tools-preview
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
