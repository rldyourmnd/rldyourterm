#!/bin/bash -eu

if [ ! -d "$SRC/rldyourterm" ]; then
  echo "repository root not found at $SRC/rldyourterm" >&2
  exit 1
fi

cd "$SRC/rldyourterm"

# ClusterFuzzLite may inject an outdated RUSTUP_TOOLCHAIN (seen in CI logs).
# Use a pinned nightly by default for deterministic fuzz CI and MSRV alignment.
# Override via CFLITE_RUST_TOOLCHAIN when intentionally updating the snapshot.
CFLITE_RUST_TOOLCHAIN="${CFLITE_RUST_TOOLCHAIN:-nightly-2026-03-07}"
export RUSTUP_TOOLCHAIN="${CFLITE_RUST_TOOLCHAIN}"

if ! rustup run "${CFLITE_RUST_TOOLCHAIN}" rustc --version >/dev/null 2>&1; then
  echo "installing missing Rust toolchain: ${CFLITE_RUST_TOOLCHAIN}" >&2
  rustup toolchain install "${CFLITE_RUST_TOOLCHAIN}" --profile minimal
fi

cargo +"${CFLITE_RUST_TOOLCHAIN}" fuzz build -O

FUZZ_OUTPUT_DIR="fuzz/target/x86_64-unknown-linux-gnu/release"
for src in fuzz/fuzz_targets/*.rs; do
  target="$(basename "${src%.rs}")"
  binary="$FUZZ_OUTPUT_DIR/$target"
  if [ ! -x "$binary" ]; then
    echo "missing fuzz binary: $binary" >&2
    exit 1
  fi
  cp "$binary" "$OUT/"
done
