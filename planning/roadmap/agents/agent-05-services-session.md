# Agent 05 — Services Session Lifecycle

## Scope (only)
- `crates/services/src/session.rs`
- `crates/services/src/error.rs`

## Goal
Implement deterministic session state machine with bounded recoverable behavior.

## Requirements
- Req: R-01, R-07, R-09
- states: starting/running/degraded/stopping/stopped.

## Done
- deterministic transition tests.
- explicit outcome for recoverable/fatal boundaries.
