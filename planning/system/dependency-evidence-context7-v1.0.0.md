# Dependency Evidence (Context7) For v1.0.0

## Snapshot Metadata

- Retrieval date: 2026-02-24.
- Evidence source: Context7 (official docs registries only).
- Scope: dependencies defined in `planning/stack/*` and architecture contracts.

## Evidence Table

| Dependency | Context7 ID | Authoritative Source | Evidence Used In Design |
|---|---|---|---|
| `portable-pty` | `/websites/rs_portable-pty` | `docs.rs/portable-pty/latest` | `PtySystem::openpty`, `SlavePty::spawn_command`, `MasterPty::try_clone_reader`, `MasterPty::take_writer`, `MasterPty::resize`, `Child::try_wait/wait/kill`; confirms single-writer ownership model and PTY lifecycle contracts. |
| `winit` | `/websites/rs_winit_winit` | `docs.rs/winit/latest` | `WindowEvent::{Moved, Resized, ScaleFactorChanged, RedrawRequested}`, `Window::current_monitor`, monitor refresh-rate access via `MonitorHandle::refresh_rate_millihertz`; confirms monitor-aware event model for cadence re-sync. |
| `wgpu` | `/websites/rs_wgpu` | `docs.rs/wgpu/latest` | `SurfaceConfiguration` (`present_mode`, `desired_maximum_frame_latency`), `Surface::configure`, `SurfaceError::{Outdated, Lost, Timeout, OutOfMemory}`; supports bounded fallback and safe surface reconfigure policy. |

## Version Notes

- This repository currently has no `Cargo.toml`; exact crate versions are not pinned yet.
- Until code scaffolding exists, the design references latest stable docs on retrieval date.
- Version pinning must be added at implementation start and reflected in stack docs.

## Design Implications

1. PTY contract remains trait-based with explicit ownership semantics.
2. Render cadence remains monitor-driven and event-triggered, not hardcoded.
3. GPU path uses robust surface/present-mode handling with deterministic fallback.
4. Any unsupported capability must degrade safely without session termination.

## Required Follow-up At Code Start

1. Add concrete crate versions in workspace manifest.
2. Verify APIs against pinned versions.
3. Update this document with exact version numbers and diff notes if APIs changed.
