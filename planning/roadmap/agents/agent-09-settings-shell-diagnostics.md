# Agent 09 — Settings + Shell + Diagnostics

## Scope (only)
- `crates/features/settings/src/lib.rs`
- `crates/features/shell_integration/src/lib.rs`
- `crates/features/diagnostics/src/lib.rs`

## Goal
Implement command-palette settings model, shell resolution behavior, and diagnostics event sink.

## Requirements
- Req: R-05, R-07, R-10, R-13
- no silent setting apply failures.

## Done
- typed settings commands and outcomes.
- deterministic fish->zsh fallback resolver.
- diagnostics event model with correlation-id fields.
