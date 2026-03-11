<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: f483875 fix(font): align degraded glyph capability with fallback rasterization
Scope: runtime state, crates/app/src/, crates/features/, scripts/ci/, .github/workflows/
Area: CORE
-->

# Current Implementation State and Known Risks

## Verified Runtime State
- GUI runtime is split across `gui_runtime.rs` and dedicated ownership modules: `gui_runtime_app_handler.rs`, `gui_runtime_backend.rs`, `gui_runtime_lifecycle.rs`, `gui_runtime_output.rs`, `gui_runtime_render.rs`, `gui_runtime_terminal_io.rs`, `gui_runtime_window.rs`, `gui_runtime_mouse.rs`
- TTY runtime is split across `pty_runtime.rs`, `pty_runtime_control.rs`, `pty_runtime_output.rs`, and `pty_runtime_terminal_io.rs`
- Shared runtime behavior lives in `crates/app/src/runtime_shared/`
- CPU rendering uses `rldyourterm-render-cpu`
- GPU rendering uses `rldyourterm-render-gpu`
- Terminal-domain types are exposed to upper layers through `crates/services/src/terminal.rs`

## Governance State
- CI on `main` validates locked quality gates, benchmark smoke, and VSA governance
- Release/manual full validation now delegates locked quality validation to `scripts/ci/run_terminal_system_suite.sh`, which emits a machine-readable JSON report and validates the referenced full benchmark report
- `scripts/ci/run_e2e_governance.sh` currently validates authority-doc sync and VSA dependency graph; optional `--with-matrix` runs the MVP compatibility matrix
- Release authority remains manual via `.github/workflows/release.yml`

## Current Primary Risk
- The main remaining systemic risk is environment variance for live-display metrics: the suite is validated and baseline-aware, but thresholds remain advisory and warning-only unless calibrated for a controlled display environment
- The repository now has a dedicated controlled-display validation lane for that calibration work: `scripts/ci/run_terminal_display_benchmark_controlled.sh`
- The repository now also has a canonical controlled calibration wrapper: `scripts/ci/run_terminal_display_benchmark_calibration.sh`
- The repository now also has a manual self-hosted workflow for that same flow: `.github/workflows/display-benchmark.yml`; it now requires a dedicated `display-benchmark` self-hosted runner label in addition to the OS label
- That workflow now emits a runner-readiness artifact before calibration so host suitability is evidenced separately from benchmark results
- That workflow now uploads readiness and partial calibration artifacts even when the calibration run fails, preserving RCA evidence
- Benchmark baseline tooling now fail-closes on `environment_scope`: `controlled-display-session` baselines can only be refreshed from and applied to monitor-aware controlled live-display reports, while generic local live-display reports remain `local-display-session`
- Controlled live-display baselines now embed calibrated environment requirements, so benchmark validation also rejects host-profile drift within the broader `controlled-display-session` scope
- The controlled calibration wrapper defaults to advisory comparison mode; `enforced` remains opt-in for intentionally hardened calibration hosts
- Live `softbuffer` CPU display runs in the current local environment are dominated by framebuffer `age=2`, not `age=1`
- The CPU display path now persists actual repaint rows for history replay; when `softbuffer` returns `age=2`, replay uses the rows physically repainted into the retained buffer, while fresh or older buffers still force a full redraw
- Live-display reports now record `pacing_mode` and `monitor_refresh_rate_millihz`, which makes it explicit whether a run used monitor cadence or the production-consistent `event-driven` fallback
- Live-display reports now also record suite-level `session_type` and `display_server_hint`, plus scenario-level `monitor_name` and `monitor_scale_factor`
- In the current local environment, monitor cadence is unavailable and live-display CPU scenarios correctly fall back to `event-driven`
- Live-display reports now also record `display_phase_stats.redraw_dispatch`, which separates event-loop or compositor wait before `RedrawRequested` from time spent inside `softbuffer::buffer_mut()`
- Live-display reports now also record `display_phase_stats.frame_gap`, which captures effective spacing between completed redraws
- The live-display resize state machine now treats `winit::Window::request_inner_size` as a four-way outcome (`applied-sync`, `queued-async`, `ignored`, `exhausted`) so ignored no-op resizes do not stall resize-cycle scenarios
- In the current local environment, the suite reports `session_type=wayland` and `display_server_hint=wayland`, while `winit` monitor metadata remains unavailable for the benchmark window; CPU steady redraw therefore stays on the production-consistent `event-driven` fallback
- In the current local environment, both `redraw_dispatch` and `buffer_acquire` remain material for steady CPU redraw, while `present` stays negligible

## Mitigations Present in Repository
- `scripts/ci/validate_authority_docs.sh` blocks known stale governance claims
- `scripts/ci/validate_vsa_dependency_graph.sh` blocks invalid crate edges
- `scripts/ci/run_terminal_benchmark_smoke.sh` keeps canonical headless benchmark paths live in CI
- `scripts/ci/run_terminal_benchmark_full.sh` exercises the full canonical headless benchmark suite and validates its JSON report schema
- `scripts/ci/run_terminal_display_benchmark_smoke.sh` and `scripts/ci/run_terminal_display_benchmark_full.sh` exercise the local/manual live-display suite over real `winit` plus `wgpu`/`softbuffer` presentation paths
- `scripts/ci/run_terminal_display_benchmark_controlled.sh` adds monitor-aware environment validation on top of the live-display suite and is intended for calibration hosts where monitor metadata must be present
- `scripts/ci/run_terminal_display_benchmark_controlled.sh` now accepts advisory baselines directly, keeping the default controlled calibration flow self-compatible with later controlled validation runs
- `scripts/ci/run_terminal_display_benchmark_calibration.sh` is the canonical host-side flow for producing a controlled live-display report, refreshing a controlled baseline from that report, and validating the calibrated result end-to-end
- `scripts/ci/validate_terminal_display_calibration_report.py` validates the machine-readable calibration report that ties together the controlled report, controlled baseline, and comparison mode
- `scripts/ci/run_terminal_display_benchmark_runner_readiness.sh` emits a machine-readable readiness report for the self-hosted calibration host and fails early if the display session is unsuitable
- CPU live-display reports include phase-level timing (`redraw_dispatch`, `frame_gap`, `buffer_acquire`, `raster`, `present`), `cpu_buffer_age_counts`, `pacing_mode`, `monitor_refresh_rate_millihz`, and monitor/session metadata so remaining local display regressions can be isolated before production render code is changed
- `crates/features/render_cpu/src/rasterize.rs` now skips row and cell work for default blank regions after the row-clear pass, which materially reduced `steady-redraw-cpu` raster cost in the local live-display suite
- `crates/features/render_cpu/src/rasterize.rs` now keeps current-frame damage separate from repaint rows so `age=2` incremental redraw stays correct and does not accumulate stale damage history across frames
- `crates/features/font/src/lib.rs` now aligns degraded glyph capability checks with the actual font8x8 fallback raster path, so box-drawing and block glyph availability stays consistent with atlas prewarm and runtime rasterization
- `scripts/ci/validate_terminal_benchmark_thresholds.py` compares validated benchmark reports against versioned baseline policies
- `scripts/ci/refresh_terminal_benchmark_baseline.py` refreshes versioned baseline manifests from validated benchmark reports and rejects scope mismatches between requested baseline scope and inferred report scope
- `scripts/ci/validate_terminal_benchmark_thresholds.py` rejects baseline comparisons when the report and baseline `environment_scope` do not match exactly
- Controlled baselines additionally carry environment requirements for display server hint, optional session type, and per-scenario CPU monitor cadence plus monitor metadata
- `scripts/ci/run_terminal_system_suite.sh` runs the canonical local and release validation lane across fmt, check, test, clippy, MSRV, fuzz compile-path, benchmark smoke/full, governance, optional live-display validation, and optional baseline enforcement
- `scripts/ci/validate_terminal_system_suite_report.py` ensures system-suite evidence stays synchronized with the referenced full benchmark report and governance mode
- The font runtime now degrades to basic `font8x8` ASCII fallback if the bundled primary font cannot be parsed, instead of panicking at construction time
