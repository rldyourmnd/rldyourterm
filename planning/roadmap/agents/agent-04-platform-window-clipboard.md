# Agent 04 — Platform Window + Clipboard Adapter

## Scope (only)
- `crates/foundation-platform/src/window.rs`
- `crates/foundation-platform/src/clipboard.rs`
- `crates/foundation/src/window.rs`
- `crates/foundation/src/clipboard.rs`

## Goal
Implement monitor-aware window adapter and safe clipboard adapter.

## Requirements
- Req: R-03, R-04
- monitor timing retrieval and safe fallback when unavailable.

## Done
- window adapter exposes monitor timing and redraw behavior contractually.
- clipboard adapter handles get/set with typed failures.
