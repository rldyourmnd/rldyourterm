<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: 32bdafe docs(governance): align benchmark and system suite guidance
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

## Mitigations Present in Repository
- `scripts/ci/validate_authority_docs.sh` blocks known stale governance claims
- `scripts/ci/validate_vsa_dependency_graph.sh` blocks invalid crate edges
- `scripts/ci/run_terminal_benchmark_smoke.sh` keeps canonical headless benchmark paths live in CI
- `scripts/ci/run_terminal_benchmark_full.sh` exercises the full canonical headless benchmark suite and validates its JSON report schema
- `scripts/ci/run_terminal_display_benchmark_smoke.sh` and `scripts/ci/run_terminal_display_benchmark_full.sh` exercise the local/manual live-display suite over real `winit` plus `wgpu`/`softbuffer` presentation paths
- `scripts/ci/validate_terminal_benchmark_thresholds.py` compares validated benchmark reports against versioned baseline policies
- `scripts/ci/refresh_terminal_benchmark_baseline.py` refreshes versioned baseline manifests from validated benchmark reports
- `scripts/ci/run_terminal_system_suite.sh` runs the canonical local and release validation lane across fmt, check, test, clippy, MSRV, fuzz compile-path, benchmark smoke/full, governance, optional live-display validation, and optional baseline enforcement
- `scripts/ci/validate_terminal_system_suite_report.py` ensures system-suite evidence stays synchronized with the referenced full benchmark report and governance mode
- The font runtime now degrades to basic `font8x8` ASCII fallback if the bundled primary font cannot be parsed, instead of panicking at construction time
