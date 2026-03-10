<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: 706ac44 docs(governance): align authority docs with current repo state
Scope: entire project, Cargo.toml, crates/, .github/workflows/, scripts/ci/, terminal_benchmark/
Area: CORE
-->

# rldyourterm - Project Overview

## Purpose
Crash-intolerant AI terminal runtime written in Rust for long-running AI CLI sessions.

## Priorities
1. Stability
2. AI CLI compatibility
3. Speed

## Workspace
- Rust edition 2024
- Minimum rust-version 1.92
- Cargo workspace with 14 packages
- Runtime crates: foundation, foundation-platform, core, services, features/*, ui, app
- Tooling crates: integration-tests, terminal-benchmark

## Normative Architecture
VSA policy:
- `app -> features -> services -> core`
- `foundation` is consumed through explicit ports
- `foundation-platform` implements foundation contracts

## Current Crate-Level Wiring
Source: `cargo metadata --format-version 1 --no-deps`
- `rldyourterm-app -> {rldyourterm-ui, rldyourterm-services, rldyourterm-foundation, rldyourterm-foundation-platform, rldyourterm-render-cpu, rldyourterm-render-gpu, rldyourterm-font, rldyourterm-settings, rldyourterm-shell-integration, rldyourterm-diagnostics}`
- `rldyourterm-ui -> rldyourterm-services`
- `rldyourterm-services -> {rldyourterm-core, rldyourterm-foundation}`
- `rldyourterm-render-cpu -> {rldyourterm-font, rldyourterm-services}`
- `rldyourterm-render-gpu -> {rldyourterm-font, rldyourterm-services}`
- `rldyourterm-settings -> rldyourterm-services`
- `rldyourterm-diagnostics -> rldyourterm-foundation`
- `rldyourterm-integration-tests -> rldyourterm-core`
- `rldyourterm-terminal-benchmark -> {rldyourterm-font, rldyourterm-render-cpu, rldyourterm-services}`

## Key Files
- `crates/app/src/main.rs` - CLI parsing, shell resolution, runtime dispatch
- `crates/app/src/gui_runtime.rs` plus `gui_runtime_*.rs` - GUI runtime orchestration
- `crates/app/src/pty_runtime.rs` plus `pty_runtime_*.rs` - TTY runtime orchestration
- `crates/app/src/runtime_shared/` - shared runtime helpers
- `crates/services/src/terminal.rs` - narrow terminal-domain export surface for upper layers
- `crates/features/render_cpu/src/` - CPU render paths
- `crates/features/render_gpu/src/` - GPU render paths
- `terminal_benchmark/src/` - canonical headless benchmark harness

## Current CI and Governance Surface
- `.github/workflows/ci.yml` runs on push and pull_request to `main`
- CI fan-out jobs: check, test, coverage, clippy, benchmark-smoke, fmt, msrv, audit, deny, ci-gate
- Additional PR-visible workflows: CodeQL, ClusterFuzzLite PR fuzzing, Scorecard, Semantic PR, PR Automation
- Governance script: `scripts/ci/run_e2e_governance.sh`
- Governance validators: `scripts/ci/validate_authority_docs.sh`, `scripts/ci/validate_vsa_dependency_graph.sh`
- Release workflow: `.github/workflows/release.yml` (manual `workflow_dispatch`)

## Knowledge Layer
- `AGENTS.md` - highest authority
- `CLAUDE.md` - repo conventions and commands
- `.serena/memories/` - project knowledge
