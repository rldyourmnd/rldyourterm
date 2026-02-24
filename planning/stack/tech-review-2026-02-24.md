# Technology Review (2026-02-24)

## Checked libraries (Context7)
- `portable-pty`: stable PTY workflow (`openpty`, `spawn_command`, `resize`, reader/writer split).
- `wgpu`: cross-platform GPU abstraction with explicit adapter request and render pipeline/queue submit lifecycle.
- `winit`: event-loop + window lifecycle control (`Resized`, `RedrawRequested`, `CloseRequested`, `request_redraw`).

## Decision for v1.0.0
1. Use all three as baseline primitives.
2. Keep their integration behind our traits (no upstream terminal engine inheritance).
3. Add deterministic fallback policy around render adapters.

## Why this set for our priorities
- Stability first: both `winit` and `portable-pty` expose explicit lifecycle/error boundaries.
- AI-tool compatibility: event loop + PTY split keeps shell streams deterministic.
- Speed target: `wgpu` gives GPU path while CPU path remains explicit and first-class.

## Open points to watch during implementation
- PTY resize propagation (`SIGWINCH`/kernel window-size sync) and cleanup on crash.
- GPU device loss handling and queue submission error paths.
- Event loop scheduling to avoid redraw storms and to maintain idle efficiency.
