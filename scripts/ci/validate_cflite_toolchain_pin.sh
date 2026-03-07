#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
build_script="${repo_root}/.clusterfuzzlite/build.sh"
dockerfile="${repo_root}/.clusterfuzzlite/Dockerfile"

build_pin="$(
  grep -E '^CFLITE_RUST_TOOLCHAIN="\$\{CFLITE_RUST_TOOLCHAIN:-[^"]+\}"$' "${build_script}" \
    | sed -E 's/^CFLITE_RUST_TOOLCHAIN="\$\{CFLITE_RUST_TOOLCHAIN:-([^"]+)\}"$/\1/' \
    | head -n 1
)"

docker_pin="$(
  grep -E '^ARG CFLITE_RUST_TOOLCHAIN=[^[:space:]]+$' "${dockerfile}" \
    | sed -E 's/^ARG CFLITE_RUST_TOOLCHAIN=([^[:space:]]+)$/\1/' \
    | head -n 1
)"

if [ -z "${build_pin}" ]; then
  echo "::error::Unable to detect default CFLITE_RUST_TOOLCHAIN in ${build_script}." >&2
  exit 1
fi

if [ -z "${docker_pin}" ]; then
  echo "::error::Unable to detect ARG CFLITE_RUST_TOOLCHAIN in ${dockerfile}." >&2
  exit 1
fi

if [ "${build_pin}" != "${docker_pin}" ]; then
  echo "::error::ClusterFuzzLite toolchain pin mismatch: build.sh=${build_pin}, Dockerfile=${docker_pin}." >&2
  exit 1
fi

echo "ClusterFuzzLite toolchain pin synchronized: ${build_pin}"
