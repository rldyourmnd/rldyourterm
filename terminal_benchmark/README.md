# terminal_benchmark

Self-authored benchmark harness for canonical headless `rldyourterm` paths.

## Scope

This suite intentionally benchmarks only stable public paths that are reproducible without a GUI/GPU requirement:
- `TerminalState` ingest throughput
- scrollback pressure and trimming path
- `SessionController` lifecycle and recoverable-boundary control flow
- `UiRuntime` command-path handling
- `SettingsService` palette parse/apply path
- shell resolution and launch-plan derivation
- `GlyphCache` mixed glyph raster path
- headless `render_gpu` surface policy helpers
- `CpuRenderer::render_full`
- `CpuRenderer::render_delta`
- combined ingest + CPU delta render cycle
- headless CPU pixel raster path via `render_terminal_buffer`

It does **not** benchmark interactive windowing, `winit` event-loop jitter, or live `wgpu` surface presentation. Those are environment-sensitive and belong to a separate future GPU/display benchmark lane. The JSON report makes that explicit through `coverage.benchmarked_layers` and `coverage.verified_only_layers`.

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

Full benchmark suite:

```bash
bash scripts/ci/run_terminal_benchmark_full.sh
```

Canonical local system suite:

```bash
bash scripts/ci/run_terminal_system_suite.sh
```

The system suite writes a JSON report that references the validated full benchmark report and the exact executed validation commands.

## Scenarios

- `core-ingest-burst`: ANSI-heavy AI CLI style output through `TerminalState`
- `core-scrollback-flood`: deep history / trimming pressure through `TerminalState`
- `core-parser-throughput`: parser-only ANSI throughput without grid dispatch
- `core-grid-scroll`: grid scrolling and dirty-row bookkeeping throughput
- `service-session-runtime-cycle`: session lifecycle and recoverable-boundary orchestration
- `ui-command-cycle`: command-driven UI control path
- `settings-apply-cycle`: command palette parse/apply cycle
- `shell-resolution-plan`: shell resolution and launch planning over deterministic availability cases
- `font-cache-mixed-raster`: glyph lookup and atlas raster path over mixed corpora
- `gpu-surface-policy`: headless surface recovery/configuration policy path
- `cpu-render-full`: deterministic full CPU frame snapshot
- `cpu-render-delta`: dirty-row CPU frame update
- `cpu-cycle-ingest-render-delta`: steady-state ingest plus CPU delta render cycle
- `cpu-pixel-raster-delta`: canonical CPU raster path over a dirty terminal buffer

## Scale presets

- `quick`: fast local smoke baseline
- `standard`: default repeatable benchmark baseline
- `stress`: larger sustained workload for deeper throughput checks
