<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: 706ac44 docs(governance): align authority docs with current repo state
Scope: Cargo.toml, Cargo.lock, .github/workflows/ci.yml, .github/workflows/release.yml
Area: CORE
-->

# Dependency Freshness Status

## Workspace Dependency Baseline
Source: `Cargo.toml` `[workspace.dependencies]`
- `portable-pty = 0.9`
- `winit = 0.30`
- `wgpu = 28.0`
- `crossterm = 0.29`
- `softbuffer = 0.4`
- `fontdue = 0.9`
- `font8x8 = 0.3`
- `clap = 4.5`
- `serde = 1.0`
- `tracing = 0.1`
- `thiserror = 2.0`
- `anyhow = 1.0`

## Current Toolchains
- Workspace minimum rust-version: 1.92
- CI toolchain: 1.94.0
- MSRV CI toolchain: 1.92.0

## Security and Supply-Chain Gates
- Security audit uses `cargo-audit` installed in CI and release preflight
- Dependency policy uses `cargo deny check bans licenses advisories sources`
- VSA internal dependency policy is validated via `scripts/ci/validate_vsa_dependency_graph.sh`

## Benchmark and Governance Tooling
- Headless benchmark crate: `terminal_benchmark/`
- Governance validator: `scripts/ci/run_e2e_governance.sh`
- Authority-doc sync validator: `scripts/ci/validate_authority_docs.sh`

## Current Status
- Workspace is pinned through `Cargo.lock`
- CI and release preflight use `--locked`
- No stale `main/dev` branch assumptions should remain in authority docs or memories
