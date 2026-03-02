# Agent 02 — Foundation API Contracts

## Scope (only)
- `crates/foundation/src/api/*`
- `crates/foundation/src/error.rs`

## Goal
Define strict integration-boundary contracts for PTY/window/clipboard/diagnostics.

## Requirements
- Req: R-06, R-09, R-10
- enforce typed errors and explicit outcome contracts.

## Done
- traits and data models are compile-checked and documented in code.
- no implementation logic beyond contracts.
