<!-- Memory Metadata
Last updated: 2026-03-09
Last commit: 7ea60ad chore: remove dead orchestrator infrastructure and align VSA ordering
Scope: crates/foundation/, crates/foundation-platform/
Area: BACKEND
-->

# Foundation Layer

## API Traits (crates/foundation/src/api/)
- `PtyIo` trait: take_reader, take_writer, resize, kill, wait, try_wait, close
- `PtyFactory` trait: spawn(PtySpawnConfig) -> PtyIo
- `WindowControl` trait: request_redraw, set_title, current_monitor_timing, clipboard, close, poll_events
- `WindowFactory` trait: init(WindowConfig) -> WindowControl
- `WindowEventSink` trait: on_event(WindowEvent)
- `ClipboardAdapter` trait: set_text, get_text, clear (`ContractResult`)

Note: Deprecated clipboard outcome enums (`ClipboardSetOutcome`, `ClipboardGetOutcome`, and the third deprecated clear enum) were removed from `api/clipboard.rs` in 0dd2fa2. The `ClipboardAdapter` trait contract is now the sole public API for clipboard operations.

## Data Types
- `PtySize` (cols, rows, pixel_width, pixel_height)
- `PtySpawnConfig` (shell_command, args, cwd, env, size)
- `WindowConfig` (title, width, height, min_width, min_height, high_dpi)
- `WindowSize`, WindowInput, WindowEvent, WindowSignal enums

## Platform Implementations (crates/foundation-platform/)
- `pty.rs` - PlatformPtyFactory/PlatformPtyIo using portable-pty, with:
  - Mutex-protected inner state (PtyInner)
  - Bounded spawn cleanup
  - Post-kill reap with backoff
  - Single-writer violation detection (recoverable-degrade)
- `window.rs` - Window implementation using winit
- `clipboard.rs` - Clipboard using arboard

## Runtime wiring status (2026-03-04)
- PTY path is wired through `PlatformPtyFactory` in GUI and TTY runtimes.
- Clipboard adapter path is wired in app runtime.
- Window adapter path is wired as primary app runtime window lifecycle (`G-010` closed).
- App runtime window control is contract-based (`WindowFactory/WindowControl`); direct app-owned `window.request_redraw`, `window.set_title`, `window.current_monitor` path is validator-guarded.

## Removed Code (2026-03-06, 4f1a89f)
- `crates/foundation/src/pty.rs` deleted: empty module that only re-exported from error.rs
- `ClipboardFailureKind`, `ClipboardFailure` structs removed from `clipboard.rs`: 77 lines of dead code never used in production

## Error System (foundation/src/error.rs)
Rich error taxonomy covering all foundation boundary faults.
