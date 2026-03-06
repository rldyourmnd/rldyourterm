# Dependency Evidence (Context7) For v1.0.0

## Snapshot Metadata

- Retrieval date: 2026-03-06.
- Evidence source: Context7 (official docs registries only).
- Scope: boundary dependencies defined in `planning/stack/*`.

## Evidence Table

| Dependency | Context7 ID | Authoritative Source | Evidence Used In Design |
|---|---|---|---|
| `portable-pty` | `/websites/rs_portable-pty` | `docs.rs/portable-pty/latest` | PTY lifecycle interfaces: `openpty`, `spawn_command`, `try_clone_reader`, `take_writer`, `resize`, child lifecycle (`try_wait/wait/kill`). Supports single-writer ownership model and session-safe PTY orchestration. |
| `winit` | `/websites/rs_winit_winit` | `docs.rs/winit/latest` | Window and monitor APIs/events: `WindowEvent::{Moved, Resized, ScaleFactorChanged, RedrawRequested}`, `Window::current_monitor`, monitor refresh-rate via `refresh_rate_millihertz`. Supports monitor-driven cadence and transfer re-sync. |
| `wgpu` | `/websites/rs_wgpu` | `docs.rs/wgpu/latest` | Surface and presentation contracts: `SurfaceConfiguration` (`present_mode`, `desired_maximum_frame_latency`), `Surface::configure`, `SurfaceError::{Timeout, Outdated, Lost, OutOfMemory, Other}`. Supports deterministic degrade/fallback path design. |
| `crossterm` | `/crossterm-rs/crossterm` | `github.com/crossterm-rs/crossterm` (API docs/wiki) | Interactive TTY control contracts: `terminal::{enable_raw_mode,disable_raw_mode}`, `event::{poll,read}`, `Event::Resize`, key-modifier model. Used only in app interactive runtime boundary path. |
| `softbuffer` | `/websites/rs_softbuffer_softbuffer` | `docs.rs/softbuffer/latest` | GUI CPU presentation contracts: `Context::new`, `Surface::new`, `Surface::resize`, `buffer_mut`, `present`. Used for single-window GUI runtime path in app layer. |

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
- `crossterm` (`/crossterm-rs/crossterm`):
  - `enable_raw_mode`/`disable_raw_mode` are the canonical raw-mode lifecycle entry/exit points.
  - `event::poll` + `event::read` provide bounded input loop handling without blocking forever on terminal events.
  - `Event::Resize(cols, rows)` is the resize boundary used to propagate PTY size updates.
- `softbuffer` (`/websites/rs_softbuffer_softbuffer`):
  - `Context::new(window)` + `Surface::new(&context, window)` establish software presentation surface bound to `winit` window handle.
  - `Surface::resize`, `buffer_mut`, `present` are sufficient for deterministic redraw loop in GUI MVP path.
  - GUI bootstrap/present failures remain bounded by deterministic fallback to TTY runtime with explicit warning events.

## 2026-03-06 Runtime Revalidation Snapshot

- `wgpu` (`/websites/rs_wgpu`):
  - `RequestAdapterOptions.power_preference` is the adapter selection hint; runtime uses `HighPerformance` for primary low-latency interactive path.
  - `DeviceDescriptor.memory_hints` supports `Performance | MemoryUsage | Manual`; runtime uses `MemoryHints::Performance`.
  - `SurfaceConfiguration.present_mode` auto modes (`AutoVsync` / `AutoNoVsync`) are explicitly documented as graceful fallback variants.
  - `desired_maximum_frame_latency` is a backend hint; value `1` is valid low-latency configuration for GUI-like responsiveness.
- `winit` (`/websites/rs_winit_winit`):
  - `WindowEvent::RedrawRequested` includes OS invalidation and explicit `request_redraw()`; duplicate redraw requests are coalesced, keeping event-driven pacing deterministic.
- `portable-pty` (`/websites/rs_portable-pty`):
  - `take_writer()` remains single-acquire; `try_clone_reader()` remains the intended read-side duplication path.
  - `try_wait`/`wait` semantics remain consistent with non-blocking poll + blocking termination wait.

Implementation alignment updated in:
- `crates/foundation-platform/src/pty.rs`
- `crates/services/src/render_mode.rs`
- `crates/features/render_gpu/src/lib.rs`
- `crates/ui/src/lib.rs`
- `crates/app/src/main.rs`
- `crates/app/src/pty_runtime.rs`
- `crates/app/src/gui_runtime.rs`

## Policy Notes

- Self-authored-first remains mandatory for runtime logic.
- External crates are integration-boundary dependencies, not core runtime replacements.
- Third-party terminal-core engines are out of scope for v1.0.

## Required Follow-up During Implementation

1. Validate exact crate API compatibility when workspace is recreated.
2. Keep this table in sync if dependency versions or contracts change.
3. Record contract-impacting changes in ADR/stack docs before code merge.
