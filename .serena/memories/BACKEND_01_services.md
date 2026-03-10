<!-- Memory Metadata
Last updated: 2026-03-11
Last commit: 706ac44 docs(governance): align authority docs with current repo state
Scope: crates/services/
Area: BACKEND
-->

# Services Layer (crates/services)

## Module Structure
- `lib.rs` - service module root
- `error.rs` - `ServiceError`
- `render_mode/` - render-mode policy and tests
- `render_pacing.rs` - monitor-driven pacing policy
- `session.rs` plus `session/tests.rs` - session lifecycle controller and tests
- `terminal.rs` - narrow terminal-domain export surface for upper layers

## Dependencies
Source: `cargo metadata --format-version 1 --no-deps`
- `rldyourterm-services -> rldyourterm-core`
- `rldyourterm-services -> rldyourterm-foundation`

## Export Surface
- `render_mode` re-exports `RenderMode` from core
- `terminal.rs` re-exports terminal-domain types needed by app, ui, renderers, and benchmark tooling
- `services` is the intended upper-layer bridge to core domain types

## Responsibilities
- Session lifecycle policy
- GPU failure and render-mode transition policy
- Monitor cadence and pacing policy
- Terminal-domain access surface for upper layers

## Current State
- `services` does not expose broad `CoreEvent` transit APIs to upper layers
- `terminal.rs` is the current narrow import path used by app/features/tooling instead of direct `rldyourterm-core` imports in those layers
