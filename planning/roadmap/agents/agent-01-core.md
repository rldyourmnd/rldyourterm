# Agent 01 — Core Engine

## Scope (only)
- `crates/core/src/*`

## Goal
Implement deterministic terminal core primitives:
- grid/cursor/scrollback,
- parser baseline behavior,
- state transitions,
- core events and errors.

## Requirements
- Req: R-06, R-08
- VSA: no OS/PTY/window/wgpu imports.

## Done
- deterministic unit tests for state/parser/scrollback cap.
- no panics on malformed input paths.
