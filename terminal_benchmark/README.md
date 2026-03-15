# terminal_benchmark

Self-authored benchmark harness for `rldyourterm` performance evidence.

## Scope

`terminal_benchmark` now has two suites:
- `canonical-headless`: deterministic CI-safe throughput and control-path benchmark coverage
- `live-display`: real `winit` window plus `wgpu`/`softbuffer` presentation timing on a live display session

The canonical headless suite intentionally benchmarks only stable public paths that are reproducible without a GUI/GPU requirement:
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

The live-display suite benchmarks:
- startup to first present on a real window surface
- steady redraw/present timing for GPU and CPU paths
- real resize cycles for GPU and CPU paths

The JSON reports keep these suites separate on purpose:
- headless reports include `coverage.benchmarked_layers` and `coverage.verified_only_layers`
- live-display reports include `environment` and display-specific workload metadata
- live-display reports also expose `pacing_mode` and `monitor_refresh_rate_millihz`, so each run states whether it used monitor cadence or the same event-driven fallback that production uses when cadence is unavailable

`live-display` is intentionally local/manual. It is not a required PR CI gate because it depends on a real display session and platform-specific presentation timing.

## Run

Canonical headless suite:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --suite canonical-headless \
  --scenario all \
  --scale standard
```

Canonical headless JSON output:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --suite canonical-headless \
  --scenario all \
  --scale stress \
  --format json \
  --output /tmp/rldyourterm-terminal-benchmark.json
```

Live-display quick run:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --suite live-display \
  --scenario all \
  --scale quick
```

Live-display JSON output:

```bash
cargo run --locked -p rldyourterm-terminal-benchmark -- \
  --suite live-display \
  --scenario all \
  --scale standard \
  --format json \
  --output /tmp/rldyourterm-terminal-display-benchmark.json
```

Headless CI smoke parity:

```bash
bash scripts/ci/run_terminal_benchmark_smoke.sh
```

Full benchmark suite:

```bash
bash scripts/ci/run_terminal_benchmark_full.sh
```

Live-display local smoke:

```bash
bash scripts/ci/run_terminal_display_benchmark_smoke.sh
```

Live-display local full suite:

```bash
bash scripts/ci/run_terminal_display_benchmark_full.sh
```

Controlled live-display suite for monitor-aware calibration:

```bash
bash scripts/ci/run_terminal_display_benchmark_controlled.sh
```

Threshold validation against versioned baselines:

```bash
TERMINAL_BENCHMARK_BASELINE=terminal_benchmark/baselines/canonical-headless.standard.json \
  bash scripts/ci/run_terminal_benchmark_full.sh

TERMINAL_DISPLAY_BENCHMARK_BASELINE=terminal_benchmark/baselines/live-display.quick.json \
  bash scripts/ci/run_terminal_display_benchmark_smoke.sh
```

Canonical local system suite:

```bash
bash scripts/ci/run_terminal_system_suite.sh
```

Canonical local system suite with optional live-display extension:

```bash
bash scripts/ci/run_terminal_system_suite.sh \
  --with-live-display smoke
```

Canonical local system suite with controlled live-display validation:

```bash
bash scripts/ci/run_terminal_system_suite.sh \
  --with-live-display controlled
```

System suite with benchmark baselines:

```bash
bash scripts/ci/run_terminal_system_suite.sh \
  --benchmark-baseline terminal_benchmark/baselines/canonical-headless.standard.json \
  --with-live-display smoke \
  --live-display-baseline terminal_benchmark/baselines/live-display.quick.json
```

The system suite writes a JSON report that references the validated headless benchmark report and, when requested, the validated live-display report alongside the exact executed validation commands and baseline policy inputs.

## Scenarios

Canonical headless scenarios:
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

Live-display scenarios:
- `startup-first-frame-gpu`: window creation, GPU initialization, and first present
- `startup-first-frame-cpu`: window creation, CPU rasterization, and first softbuffer present
- `steady-redraw-gpu`: repeated GPU redraw and present on a live surface
- `steady-redraw-cpu`: repeated CPU redraw and softbuffer present on a live surface
- `resize-cycle-gpu`: real resize cycles plus GPU redraw/present
- `resize-cycle-cpu`: real resize cycles plus CPU redraw/softbuffer present

For CPU live-display scenarios, the JSON report also includes `cpu_phase_stats` with per-iteration timing aggregates for:
- `buffer_acquire`
- `raster`
- `present`

All live-display scenarios also include `display_phase_stats` with:
- `redraw_dispatch`
- `frame_gap`

CPU live-display reports also include `cpu_buffer_age_counts`, which records how often the live `softbuffer` framebuffer arrived with:
- `age_0`
- `age_1`
- `age_2`
- `age_3_plus`

The live-display JSON report also records:
- `pacing_mode`
- `monitor_refresh_rate_millihz`
- `monitor_name`
- `monitor_scale_factor`

This is important for RCA. The production-aligned CPU display path uses the framebuffer age contract directly:
- `age=1`: repaint current-frame damage only
- `age=2`: repaint the union of current-frame damage and the previous presented frame's damage
- any other age: force a full redraw

The benchmark harness now follows the same redraw-shaping rule as the production GUI runtime:
- when monitor timing is available for CPU steady/resize scenarios, it may use cadence-timed redraw scheduling
- when monitor timing is unavailable, it falls back to event-driven redraw scheduling instead of inventing a benchmark-only timer

This is important for RCA as well:
- large `redraw_dispatch` points to event-loop or compositor wait before `RedrawRequested`
- large `frame_gap` points to wide effective spacing between completed redraws
- large `buffer_acquire` points to time spent inside `softbuffer::buffer_mut()`

The controlled live-display runner adds environment validation on top of schema validation:
- CPU monitor-aware scenarios (`steady-redraw-cpu`, `resize-cycle-cpu`) must use `monitor-cadence`
- `monitor_refresh_rate_millihz` must be present and positive for those scenarios
- `monitor_scale_factor` must be present and positive for those scenarios
- optional session/display-server expectations can be supplied through environment variables

## Scale presets

- `quick`: fast local smoke baseline
- `standard`: default repeatable benchmark baseline
- `stress`: larger sustained workload for deeper throughput checks

## Baselines

- `terminal_benchmark/baselines/canonical-headless.standard.json`: enforced threshold policy for the canonical headless full suite
- `terminal_benchmark/baselines/live-display.quick.json`: advisory threshold policy for quick local live-display smoke
- `terminal_benchmark/baselines/live-display.standard.json`: advisory threshold policy for standard local live-display full runs

Advisory live-display baselines remain non-blocking. The validator emits regression details to stderr, but it does not fail the command unless the policy is intentionally promoted to enforced for a controlled display environment.

Refresh a baseline from a validated report:

```bash
bash scripts/ci/refresh_terminal_benchmark_baseline.sh \
  /tmp/report.json \
  terminal_benchmark/baselines/custom.json
```

Refresh a controlled live-display baseline only from a monitor-aware controlled report:

```bash
bash scripts/ci/run_terminal_display_benchmark_calibration.sh \
  /tmp/live-display-controlled-report.json \
  terminal_benchmark/baselines/live-display.controlled.json \
  /tmp/live-display-controlled-calibration.json
```

The calibration wrapper defaults to `comparison_mode=advisory`. Set `TERMINAL_DISPLAY_BENCHMARK_COMPARISON_MODE=enforced` only when you intentionally want the controlled baseline to become a hard gate.

`environment_scope` is now fail-closed:
- `portable-headless` baselines only apply to canonical headless reports
- `local-display-session` baselines only apply to generic live-display reports
- `controlled-display-session` baselines can only be refreshed from, and applied to, monitor-aware controlled live-display reports

Controlled baselines also embed calibrated environment requirements:
- suite-level display server metadata
- optional session type
- per-scenario CPU monitor cadence, refresh rate, and scale factor

That makes controlled calibration host-specific by design instead of treating all monitor-aware sessions as interchangeable.

The same flow is also available as a manual self-hosted GitHub Actions workflow:
- `.github/workflows/display-benchmark.yml`
- required runner labels: `self-hosted`, `display-benchmark`, and the selected OS label
- artifacts:
  - `live-display-runner-readiness.json`
  - `live-display-controlled-report.json`
  - `live-display-controlled-baseline.json`
  - `live-display-controlled-calibration.json`

Artifact upload is preserved on failed workflow runs as well, so readiness evidence and any partial calibration outputs remain available for RCA.
