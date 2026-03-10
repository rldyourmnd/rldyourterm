<!-- Memory Metadata
Last updated: 2026-03-10
Last commit: a6c5c87 fix(ci): clippy warnings in integration-tests, VSA layer mapping
Scope: crates/ui/, crates/features/, crates/app/src/gui_runtime*.rs, crates/app/src/main.rs, crates/app/src/runtime_shared/, crates/app/src/pty_runtime*.rs, crates/integration-tests/
Area: FRONTEND
-->

# UI and Features Layer

## UiRuntime (crates/ui/src/lib.rs)
- Bootstrap: UiBootstrapConfig -> UiRuntime (creates session, render mode, pacing, terminal)
- Command loop: handle_command(UiRuntimeCommand) -> UiCommandReceipt
- Tick: advances session state, returns UiCommandOutcome
- Single-window enforcement: validate_single_window()
- Constants: SINGLE_WINDOW_BASELINE=1, DEFAULT_SCROLLBACK_CAP=50000

### UiRuntimeCommand enum
Commands for: tick, render mode changes, cadence resync, GPU failures, session boundaries, settings, stop

## Font (crates/features/font/)
- `GlyphCache`: fontdue-backed rasterization cache, parameterized by cell width/height and px_size
- `GlyphBitmap`: rasterized glyph output with metrics (x_offset, y_offset, glyph_width, glyph_height, alpha data)
- `rasterize_for_atlas(cache, ch) -> Vec<u8>`: produces a cell-sized RGBA buffer for GPU atlas upload
- Font data: JetBrains Mono Nerd Font Mono (SIL OFL 1.1) bundled via `include_bytes!` from `assets/fonts/`
- Box drawing characters (U+2500-U+259F range) and block elements use font8x8 pixel-perfect bitmaps scaled to cell size; all other characters use fontdue rasterization
- Used by: render_gpu (atlas pipeline) and gui_runtime.rs (CPU blitting path)

## Render CPU (crates/features/render_cpu/)
- Software renderer using softbuffer
- CpuRenderer with frame buffer management
- Glyph rasterization via rldyourterm-font (GlyphCache/GlyphBitmap) - no direct font8x8 or fontdue dependency
- Imports from `rldyourterm_services` (not `rldyourterm_core`) for Cursor, Grid, TerminalState, Attrs, CELL_*, Color (3ba0d8b)

## Render GPU (crates/features/render_gpu/)
- GpuRenderer using wgpu
- Surface error classification: SurfaceErrorCategory (Timeout, Outdated, Lost, OutOfMemory)
- Recovery actions: Retry, Reconfigure, Degrade
- SurfaceRecoveryPolicy with acquire + reconfigure retry budgets
- Surface runtime state tracking (consecutive failures)
- Imports from `rldyourterm_services` (not `rldyourterm_core`) for Grid, TerminalState, CELL_* (3ba0d8b)

### Dirty-Row Partial Rendering (Phase 1 GPU Optimization)
- `GpuBackend` holds persistent `cell_instances: Vec<CellInstance>` across frames; clean rows are never recomputed
- `GpuBackend::prepare_dirty_rows(terminal, dirty_rows: &[bool])` - updates `cell_instances` in-place only for dirty rows; replaces old full-grid `prepare_cell_data()`
- `upload_dirty_ranges(queue, cell_buffer, cell_instances, dirty_rows, grid_cols, row_byte_size)` - coalesces adjacent dirty rows into contiguous `write_buffer` calls with byte offset; minimizes PCIe transfers
- `render_frame()` signature changed to `render_frame(&mut self, terminal: &TerminalState, dirty_rows: &[bool])`
- Frame skip: `GpuRenderer` tracks `last_cursor_row`, `last_cursor_col`, `last_cursor_visible`; if no dirty rows AND cursor unchanged, the entire render pass is skipped (returns `Ok(())` immediately)
- Performance impact: idle terminal = zero CPU/GPU work; single char edit ~160 cells / ~2.5 KB upload (vs previous 8K cells / 128 KB full upload)

### Atlas Loading
- `build_glyph_atlas` at startup loads only essential ranges: ASCII (0x0020-0x007F), Box Drawing (0x2500-0x257F), Block Elements (0x2580-0x259F) - approximately 255 glyphs
- Non-essential ranges (Cyrillic, Latin Extended, Greek, Nerd Font icons, etc.) loaded on-demand via `ensure_glyph_in_atlas` at first encounter; typical cost ~0.1 ms per glyph
- Atlas constants: ATLAS_GLYPH_WIDTH, ATLAS_GLYPH_HEIGHT, ATLAS_SIZE, ATLAS_GLYPH_COLS, ATLAS_GLYPH_ROWS, ATLAS_SLOTS
- `GpuBackend` holds `glyph_cache: GlyphCache`, `char_to_slot: HashMap<char, u16>`, `next_atlas_slot: u16`

### Pipeline Cache (Vulkan only)
- `GpuBackend` has `pipeline_cache: Option<wgpu::PipelineCache>` and `adapter_info: wgpu::AdapterInfo` fields
- `GpuRenderer::initialize(target, width, height, cache_dir: Option<&Path>)` - loads pipeline cache from disk if available, compiles pipeline with cache reference
- `GpuRenderer::save_pipeline_cache(cache_dir: &Path)` - persists cache atomically (temp file + rename) on shutdown; no-op if adapter does not support `PIPELINE_CACHE` feature
- Cache key derived from `wgpu::util::pipeline_cache_key(&adapter_info)` (adapter-specific, Vulkan-only)

## Settings (crates/features/settings/)
- SettingsService: strict command parser + transactional apply
- SettingsState: mode, shell_target, shell_auto_init, cadence_policy, theme, profile, debug_mode
- SettingsCommand parsing from palette input
- RuntimeProfileState for export/import with schema versioning
- Palette command grammar supports:
  - `mode <auto|cpu|gpu>`
  - `shell <fish|zsh|auto>`
  - `shell auto-init <on|off>`
  - `render cadence monitor-auto`
  - `theme set <cuberpunk|aurora|monochrome>`
  - `profile <balanced|throughput|stability>`
  - `debug <on|off>`

## Shell Integration (crates/features/shell_integration/)
- Shell detection and resolution
- Fish + starship baseline, zsh fallback
- Shell availability checking

## Diagnostics (crates/features/diagnostics/)
- EventKind enum with foundation mappings
- DiagnosticsSink: event emission with auto-incrementing IDs
- CorrelationId for event correlation across boundaries
- Typed payloads: SettingsApplyTypedPayload, ShellResolutionTypedPayload, ShellLaunchTypedPayload
- JSON serialization for payload transport

## App Runtimes (crates/app/src/)
### gui_runtime.rs - GuiRuntimeApp
- Full GUI terminal: winit event loop + PTY + GPU/CPU rendering
- `ApplicationHandler<GuiEvent>` for winit integration
- Uses domain `TerminalState` (Grid + Cursor + Parser + Scrollback) directly; no intermediate TerminalBuffer or EscapeState types
- Window lifecycle routed via `WindowFactory`/`WindowControl` (foundation ports), not direct winit calls
- Clipboard access via `ClipboardAdapter` (foundation port)
- Monitor-affecting window events trigger cadence resync
- Command palette (Ctrl/Cmd+Shift+P) integration
- Palette UI is shortcut-first (`1/2/3/d/i/Esc`) with no free-form command-line input
- Palette settings dispatch uses exhaustive match (not wildcard `_`); unimplemented settings commands (shell, theme, cadence, profile) return explicit "saved (restart required)" user feedback (3d60c0f)
- PTY boundary handling with session policy
- Full debug/trace instrumentation across all `ApplicationHandler` methods and PTY operations

#### Recent Features (PRs #12, #13, #15)

### Cursor Shapes (PR #12)
- DECSCUSR sequences (CSI q Ps 1-6) control cursor shape
- Supported: block (1/2), underline (3/4), bar (5/6)
- Both CPU and GPU renderers implement cursor rendering with blink support
- Cursor state: `CursorShape` enum in `TerminalState`

### Bell (PR #12)
- `pending_bell: bool` side-channel in `TerminalState`
- Dispatched to winit user-attention in app lifecycle
- Allows visual/audio bell notification without blocking PTY

### Selection UI (PRs #12, #13)
- Mouse click+drag text selection
- XOR highlight in CPU renderer (restricted to dirty rows only)
- Uniform-driven highlight in GPU renderer
- Clipboard copy-on release via `ClipboardAdapter`
- Trailing whitespace trimmed per line in copied content
- **Scrollback guard** (PR #13): selection disabled when `viewport_offset > 0`
- **Cursor-on-selection skip** (PR #13): cursor cell excluded from selection XOR/swap in both renderers
- **Post-copy clear** (PR #13): `clear_selection()` called after clipboard copy on mouse release

### GPU Shader Selection (PR #13)
- `cursor_index` computed once in vertex shader, reused by both selection and cursor passes (`is_cursor_cell`)
- `SELECTION_NONE` sentinel (`u32::MAX`) defined in render_gpu, imported by app layer

### DECSTBM Fix (PR #15)
- Bottom param 0 treated as default (last row) per VT spec
- Prevents u16 underflow in scroll region calculations
- Regression test: `decstbm_zero_bottom_does_not_underflow`

#### Key GuiRuntimeApp Methods
- `glyph_cache: GlyphCache` - per-runtime instance of GlyphCache (from rldyourterm-font), initialized with CELL_WIDTH x CELL_HEIGHT at bootstrap; used to blit glyphs into the softbuffer framebuffer in the CPU path
- `gpu_cache_dir: Option<PathBuf>` - resolved at construction via `resolve_gpu_cache_dir()`; passed to `GpuRenderer::initialize()` and `save_pipeline_cache()` on shutdown
- `dispatch_terminal_responses(events)` - iterates `CoreEvent::TerminalResponse` from a feed result and writes the response bytes to the PTY writer; called immediately after `terminal.feed()` in `user_event`
- `release_window_resources()` - drops `Arc<Window>` references in strict order (softbuffer surface, softbuffer context, GpuRenderer, window_control, window) while the Wayland/X11 connection is still alive; prevents ghost window entries in dock/taskbar
- `shutdown()` - closes PTY, saves pipeline cache via `gpu_renderer.save_pipeline_cache()`, joins reader and wait pump threads with a bounded timeout

#### Dirty-Row Render Wiring in gui_runtime
GPU render path: `dirty_rows = terminal.grid.dirty_rows()` is read before calling `render_frame()`; `terminal.grid.take_dirty_rows()` is called only on successful GPU present. This ensures dirty flags are preserved if surface acquisition fails, guaranteeing no lost updates.
CPU render path: full framebuffer repaint on every redraw request (no dirty tracking in CPU path yet).

#### Keyboard Encoding (xterm CSI Modifiers)
`encode_winit_key_event` generates xterm CSI modifier sequences for modified keys. Modifier parameter = `1 + shift + 2*alt + 4*ctrl`. Covered: Ctrl+Arrow, Shift+Arrow, Alt+Arrow, modified F-keys, Shift+Tab, Alt+Backspace, Alt+char. Enables Ctrl+Left/Right word navigation and Shift+Arrow selection in AI CLI tools (Claude Code, Codex).

#### resolve_gpu_cache_dir
Platform-aware: macOS uses `~/Library/Caches/rldyourterm`; Linux uses `$XDG_CACHE_HOME/rldyourterm` with fallback to `~/.cache/rldyourterm`.

#### Deferred GPU Init (e4d6b1d)
- `gpu_init_pending: bool` flag set to `true` at construction when mode is not `RenderMode::Cpu`.
- `bootstrap_window()` always creates a softbuffer context and surface first, regardless of render mode. This ensures the event loop can process time-sensitive terminal queries (e.g., fish DA1) before the blocking GPU init (~1-2 s).
- On first `about_to_wait()` with `gpu_init_pending == true`: `try_deferred_gpu_init()` is called (one-shot, sets flag to false immediately).
  - Drops `self.surface` and `self._context` before calling `GpuRenderer::initialize()`. Drop is mandatory for Wayland surface exclusivity: wgpu and softbuffer cannot both hold the same `wl_surface` simultaneously.
  - On success: calls `terminal.grid.mark_all_dirty()` + `queue_redraw()` to force a full GPU repaint.
  - On failure: app remains in CPU path (softbuffer is lazily recreated on next draw call).
- Result: fish DA1 response arrives in ~1.2 s instead of ~2 s (event loop is unblocked sooner).

#### ApplicationHandler Lifecycle
- `resumed()` - calls `bootstrap_window()`; fatal error sets `fatal_error` and exits event loop
- `suspended()` - logs a warning; no resource release (compositor may resume)
- `about_to_wait()` - runs `try_deferred_gpu_init()` on first call if `gpu_init_pending`, then `request_redraw_if_needed()`
- `exiting()` - calls `release_window_resources()` so compositor receives surface-destroy before connection closes
- `user_event(GuiEvent::Output)` - calls `terminal.feed()`, then `dispatch_terminal_responses()`, updates window title from `terminal.window_title()`, queues redraw
- `window_event` - handles keyboard input, window close, resize, scale factor, monitor moves, palette shortcut

### runtime_shared/ - shared utilities module (decomposed from shared.rs in ff5f095)
Originally `shared.rs`, decomposed into `runtime_shared/` directory with 10 dedicated modules.
Exports (all `pub(crate)`):
- `key_encoding.rs`: `xterm_modifier_param`, `csi_modified`, `tilde_modified`, `fkey_ss3_modified`, `encode_ctrl_letter`
- `pty_boundary.rs`: `PtyBoundaryPolicyDecision` enum, `classify_pty_boundary_failure`
- `display.rs`: `render_mode_token`, `on_off_token`, `session_boundary_token`, `fatal_boundary_reason_token`
- `io.rs`: `write_all_and_flush`, `is_disconnect_error`
- `input.rs`: Input handling utilities
- `palette.rs`: Palette command handling
- `shutdown.rs`: Shutdown coordination
- `spawn_env.rs`: Process spawn environment
- `runtime_config.rs`: Runtime configuration types
- `terminal.rs`: Terminal utilities

Benefit: xterm key encoding is guaranteed identical between GUI and TTY runtimes. Each module has inline tests.

### App Crate Decomposition (2026-03-09)
The `gui_runtime.rs` and `pty_runtime.rs` files have been decomposed into multiple dedicated files:

#### GUI Runtime Files
- `gui_runtime.rs` - GuiRuntimeApp struct definition, bootstrap, public API
- `gui_runtime_app_handler.rs` - ApplicationHandler<GuiEvent> implementation
- `gui_runtime_backend.rs` - GPU/CPU backend switching logic
- `gui_runtime_lifecycle.rs` - Initialization and shutdown lifecycle
- `gui_runtime_output.rs` - PTY output event handling
- `gui_runtime_render.rs` - Frame rendering coordination
- `gui_runtime_terminal_io.rs` - PTY I/O operations
- `gui_runtime_window.rs` - Window operations
- `gui_runtime/tests/` - Test modules (keys.rs, render.rs, output.rs, geometry.rs, runtime.rs)

#### PTY Runtime Files
- `pty_runtime.rs` - PtyRuntimeApp struct definition and main loop
- `pty_runtime_control.rs` - Input handling and control flow
- `pty_runtime_output.rs` - Output processing
- `pty_runtime_terminal_io.rs` - PTY I/O operations
- `pty_runtime/tests.rs` - Test module

#### Runtime Shared Module
`runtime_shared/` directory (10 modules): display, input, io, key_encoding, palette, pty_boundary, runtime_config, shutdown, spawn_env, terminal

### main.rs - CLI entry point
- `LogLevelArg` enum: `Standard` (info, default), `Debug` (debug + targets + file:line + thread names), `Trace` (maximum verbosity)
- `--log-level` CLI flag; `init_tracing(level)` called before `run()`; tracing subscriber initialized CLI-first before any runtime code runs
- GUI runtime is default path; TTY runtime invoked with `--tty` flag; GUI failure auto-falls back to TTY

### pty_runtime.rs - TTY fallback
- Crossterm-based raw mode terminal
- Adaptive poll timeouts based on render cadence
- Same shortcut-first palette model and boundary handling as GUI
