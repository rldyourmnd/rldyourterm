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

## 2026-03-02 Runtime Revalidation Snapshot

- `portable-pty` (`/websites/rs_portable-pty`):
  - `Child::try_wait` is non-blocking; `Child::wait` blocks until exit.
  - `ChildKiller::clone_killer` is explicitly intended for independent signaling from a thread while another handle may be blocked in `wait`.
  - `MasterPty::take_writer` is single-acquire (invalid to call more than once); `try_clone_reader` is the intended read-side duplication path.
- `winit` (`/websites/rs_winit_winit`):
  - `WindowEvent::RedrawRequested` is emitted both for OS invalidation and `request_redraw()`.
  - duplicate redraw requests are coalesced by `winit`, which validates event-driven pacing without busy-loop redraw spam.
- `wgpu` (`/websites/rs_wgpu`):
  - `SurfaceError::{Timeout, Outdated, Lost, OutOfMemory, Other}` semantics confirm retry/reconfigure/fallback classification boundaries in render failure handling.

Implementation alignment updated in:
- `crates/foundation-platform/src/pty.rs`
- `crates/services/src/render_mode.rs`
- `crates/ui/src/lib.rs`
- `crates/app/src/main.rs`

## Policy Notes

- Self-authored-first remains mandatory for runtime logic.
- External crates are integration-boundary dependencies, not core runtime replacements.
- Third-party terminal-core engines are out of scope for v1.0.

## Required Follow-up During Implementation

1. Validate exact crate API compatibility when workspace is recreated.
2. Keep this table in sync if dependency versions or contracts change.
3. Record contract-impacting changes in ADR/stack docs before code merge.
