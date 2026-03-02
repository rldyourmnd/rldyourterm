# Dependency Evidence (Context7) For v1.0.0

## Snapshot Metadata

- Retrieval date: 2026-03-02.
- Evidence source: Context7 (official docs registries only).
- Scope: boundary dependencies defined in `planning/stack/*`.

## Evidence Table

| Dependency | Context7 ID | Authoritative Source | Evidence Used In Design |
|---|---|---|---|
| `portable-pty` | `/websites/rs_portable-pty` | `docs.rs/portable-pty/latest` | PTY lifecycle interfaces: `openpty`, `spawn_command`, `try_clone_reader`, `take_writer`, `resize`, child lifecycle (`try_wait/wait/kill`). Supports single-writer ownership model and session-safe PTY orchestration. |
| `winit` | `/websites/rs_winit_winit` | `docs.rs/winit/latest` | Window and monitor APIs/events: `WindowEvent::{Moved, Resized, ScaleFactorChanged, RedrawRequested}`, `Window::current_monitor`, monitor refresh-rate via `refresh_rate_millihertz`. Supports monitor-driven cadence and transfer re-sync. |
| `wgpu` | `/websites/rs_wgpu` | `docs.rs/wgpu/latest` | Surface and presentation contracts: `SurfaceConfiguration` (`present_mode`, `desired_maximum_frame_latency`), `Surface::configure`, `SurfaceError::{Timeout, Outdated, Lost, OutOfMemory, Other}`. Supports deterministic degrade/fallback path design. |

## Policy Notes

- Self-authored-first remains mandatory for runtime logic.
- External crates are integration-boundary dependencies, not core runtime replacements.
- Third-party terminal-core engines are out of scope for v1.0.

## Required Follow-up During Implementation

1. Validate exact crate API compatibility when workspace is recreated.
2. Keep this table in sync if dependency versions or contracts change.
3. Record contract-impacting changes in ADR/stack docs before code merge.
