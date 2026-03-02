# Agent 08 — Feature Render GPU

## Scope (only)
- `crates/features/render_gpu/src/lib.rs`

## Goal
Build GPU renderer baseline with explicit surface error classification.

## Requirements
- Req: R-02, R-03
- map wgpu surface errors to retry/reconfigure/degrade categories.

## Done
- surface error handling policy represented in code.
- compile-safe API for service fallback controller integration.
