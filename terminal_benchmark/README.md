# terminal_benchmark

Self-authored benchmark harness for canonical headless `rldyourterm` paths.

## Scope

This suite intentionally benchmarks only stable public paths that are reproducible without a GUI/GPU requirement:
- `TerminalState` ingest throughput
- scrollback pressure and trimming path
- `CpuRenderer::render_full`
- `CpuRenderer::render_delta`
- combined ingest + CPU delta render cycle
- headless CPU pixel raster path via `render_terminal_buffer`

It does **not** benchmark interactive windowing, `winit` event-loop jitter, or live `wgpu` surface presentation. Those are environment-sensitive and belong to a separate future GPU/display benchmark lane.

## Run

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- --scenario all --scale standard
```

JSON output to a file:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --scenario all \
  --scale stress \
  --format json \
  --output /tmp/rldyourterm-terminal-benchmark.json
```

CI smoke parity:

```bash
bash scripts/ci/run_terminal_benchmark_smoke.sh
```

## Scenarios

- `core-ingest-burst`: ANSI-heavy AI CLI style output through `TerminalState`
- `core-scrollback-flood`: deep history / trimming pressure through `TerminalState`
- `cpu-render-full`: deterministic full CPU frame snapshot
- `cpu-render-delta`: dirty-row CPU frame update
- `cpu-cycle-ingest-render-delta`: steady-state ingest plus CPU delta render cycle
- `cpu-pixel-raster-delta`: canonical CPU raster path over a dirty terminal buffer

## Scale presets

- `quick`: fast local smoke baseline
- `standard`: default repeatable benchmark baseline
- `stress`: larger sustained workload for deeper throughput checks
