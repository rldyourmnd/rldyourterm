# Agent 06 — Services Render Mode + Pacing

## Scope (only)
- `crates/services/src/render_mode.rs`
- `crates/services/src/render_pacing.rs`

## Goal
Implement render mode controller (`cpu/gpu/auto`) and monitor-driven cadence policy.

## Requirements
- Req: R-02, R-03, R-04
- no hardcoded FPS in primary path.

## Done
- deterministic fallback decision logic hooks.
- monitor timing re-sync behavior covered by tests.
