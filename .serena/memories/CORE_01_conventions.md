<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: 706ac44 docs(governance): align authority docs with current repo state
Scope: entire codebase, Cargo.toml, crates/, .github/workflows/
Area: CORE
-->

# Code Conventions and Patterns

## Rust and Naming
- Rust edition 2024
- Minimum rust-version 1.92
- snake_case for functions and variables
- PascalCase for types and enums

## Error Handling
- `thiserror` for typed domain and layer errors
- `anyhow` at application boundaries
- Layer-specific errors: core, foundation, services, ui/app
- Recoverable vs fatal boundary classification is explicit in runtime code

## Module Structure
- Large runtime modules are split by responsibility instead of growing as single files
- `crates/app/src/` uses `gui_runtime_*.rs`, `pty_runtime_*.rs`, and `runtime_shared/`
- `crates/features/render_gpu/src/` uses focused internal modules (`atlas.rs`, `bootstrap.rs`, `cell_data.rs`, `frame.rs`, `pipeline_cache.rs`, `surface.rs`)
- `crates/features/render_cpu/src/` is split into `renderer.rs`, `rasterize.rs`, and tests
- `crates/services/src/` keeps orchestration modules separate (`render_mode/`, `render_pacing.rs`, `session.rs`, `terminal.rs`)

## Architecture Patterns
- Foundation ports and platform adapters
- Service-layer controllers for session, render-mode, and pacing policy
- Command-oriented UI runtime
- Shared runtime helpers in `runtime_shared/` to keep GUI and TTY behavior aligned
- Narrow service export surfaces instead of broad core re-exports to upper layers

## Testing Patterns
- Unit tests live next to modules or in dedicated `tests.rs` files
- Integration tests are isolated in `crates/integration-tests/`
- Fuzz targets live in `fuzz/fuzz_targets/`
- Benchmark harness lives in `terminal_benchmark/`

## Governance
- Architecture-sensitive changes must update `AGENTS.md`, `CLAUDE.md`, and Serena memories together
- Governance CI uses `scripts/ci/run_e2e_governance.sh`
- Authority-doc drift is blocked by `scripts/ci/validate_authority_docs.sh`
