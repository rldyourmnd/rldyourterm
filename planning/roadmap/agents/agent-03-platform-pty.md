# Agent 03 — Platform PTY Adapter

## Scope (only)
- `crates/foundation-platform/src/pty.rs`

## Goal
Implement PTY adapter using portable-pty with single-writer safety and lifecycle semantics.

## Requirements
- Req: R-01, R-09
- Context7 API alignment for open/spawn/read/write/resize/wait/kill.

## Done
- adapter compiles and maps errors into foundation error model.
- has tests/mocks for writer uniqueness and resize path.
