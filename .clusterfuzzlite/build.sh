#!/bin/bash -eu

if [ ! -d "$SRC/rldyourterm" ]; then
  echo "repository root not found at $SRC/rldyourterm" >&2
  exit 1
fi

cd "$SRC/rldyourterm"

# ClusterFuzzLite sets RUSTUP_TOOLCHAIN in the runtime container and may pin
# an older nightly than the repository MSRV. Force fresh nightly selection for
# fuzz builds to keep CI aligned with workspace rust-version constraints.
export RUSTUP_TOOLCHAIN="nightly"
cargo +nightly fuzz build -O

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
