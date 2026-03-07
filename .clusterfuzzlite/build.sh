#!/bin/bash -eu

if [ ! -d "$SRC/rldyourterm" ]; then
  echo "repository root not found at $SRC/rldyourterm" >&2
  exit 1
fi

cd "$SRC/rldyourterm"

cargo fuzz build -O

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
