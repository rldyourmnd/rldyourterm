#!/usr/bin/env bash
set -euo pipefail

report_path="${1:-$(mktemp -t rldyourterm-terminal-benchmark.XXXXXX.json)}"

cargo run -q --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale quick \
  --warmup-iterations 0 \
  --iterations 1 \
  --format json \
  --output "$report_path" \
  >/dev/null

python3 - "$report_path" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
with report_path.open("r", encoding="utf-8") as handle:
    payload = json.load(handle)

results = payload.get("results")
if not isinstance(results, list) or not results:
    raise SystemExit(f"benchmark smoke failed: no scenario results in {report_path}")

names = [entry.get("scenario") for entry in results]
missing = [name for name in (
    "core-ingest-burst",
    "core-scrollback-flood",
    "cpu-render-full",
    "cpu-render-delta",
    "cpu-cycle-ingest-render-delta",
    "cpu-pixel-raster-delta",
) if name not in names]
if missing:
    raise SystemExit(
        f"benchmark smoke failed: missing scenarios in {report_path}: {', '.join(missing)}"
    )
PY

echo "benchmark smoke ok: $report_path"
