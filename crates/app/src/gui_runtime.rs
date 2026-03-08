#[path = "gui_runtime_app_handler.rs"]
mod app_handler;
#[path = "gui_runtime_lifecycle.rs"]
mod lifecycle;
#[path = "gui_runtime_output.rs"]
mod output;
#[path = "gui_runtime_render.rs"]
mod rendering;
#[path = "gui_runtime_terminal_io.rs"]
mod terminal_io;
#[path = "gui_runtime_window.rs"]
mod windowing;

use std::io::{self, Read, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use self::output::{
    OutputChunk, OutputQueueBackpressure, output_drain_budget, output_drain_budget_exhausted,
    recycle_output_chunk_buffer, should_flush_output_batch, spawn_reader_pump, spawn_wait_pump,
    warm_output_chunk_pool,
};
#[cfg(test)]
use self::output::{
    OutputDrainBudget, OutputDrainPressure, OutputQueueSnapshot, take_output_chunk_buffer,
};
#[cfg(test)]
use self::terminal_io::{
    cap_paste_text, dispatch_runtime_palette_command, read_clipboard_text_for_paste,
};
use self::windowing::cap_framebuffer_extent;
#[cfg(test)]
use self::windowing::{
    ViewportGeometry, cadence_resync_command_for_monitor_event, cap_terminal_geometry,
    sample_monitor_refresh_rate_millihz, viewport_geometry_changed,
};
use crate::gui_runtime_backend::{
    BackendSyncAction, DEFERRED_GPU_INIT_RETRY_BUDGET, GpuFailureHandling,
    RenderBackendCoordinator, deferred_gpu_init_backoff, dispatch_gpu_failure_command,
    emit_gpu_auto_fallback_observability,
};
#[cfg(test)]
use crate::gui_runtime_backend::{DeferredGpuInitState, RenderWaitPolicy, render_wait_policy};
use crate::runtime_shared::input::{
    encode_winit_key_event as shared_encode_winit_key_event, is_local_shutdown_key_winit,
    is_runtime_palette_shortcut_winit, runtime_key_from_winit_borrowed,
};
use crate::runtime_shared::palette::{
    RuntimePaletteView, handle_runtime_palette_key_input,
    runtime_palette_status_line as shared_runtime_palette_status_line, toggle_runtime_palette,
};
use crate::runtime_shared::pty_boundary::{
    BoundaryFailureOutcome, PtyReadFailureResolution, apply_pty_boundary_failure,
    fatal_pty_boundary_failure, mark_pty_boundary_recovered as shared_mark_pty_boundary_recovered,
    resolve_live_pty_read_failure, runtime_boundary_notice,
};
use crate::runtime_shared::shutdown::{
    JoinThreadOutcome, SHUTDOWN_JOIN_POLL_INTERVAL, SHUTDOWN_JOIN_TIMEOUT,
    child_exit_drain_timed_out as shared_child_exit_drain_timed_out,
    join_thread_with_timeout as shared_join_thread_with_timeout,
};
use crate::runtime_shared::terminal::{
    TerminalResponseBuffer, terminal_feed_chunks, terminal_feed_max_bytes_per_call,
};
use anyhow::{Context, Result, anyhow};
use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
use rldyourterm_font::GlyphCache;
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation::api::window::{
    MonitorTiming, WindowConfig as FoundationWindowConfig, WindowControl, WindowFactory,
};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_foundation_platform::window::PlatformWindowFactory;
use rldyourterm_render_cpu::render_terminal_buffer;
use rldyourterm_render_gpu::GpuRenderer;
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::{SessionBoundary, SessionController, SessionState};
use rldyourterm_services::terminal::{CELL_HEIGHT, CELL_WIDTH, TerminalState};
use rldyourterm_settings::{SettingsCommand, SettingsService};
use rldyourterm_ui::{
    DEFAULT_TERMINAL_COLS, DEFAULT_TERMINAL_ROWS, UiBootstrapConfig, UiCommandOutcome, UiRuntime,
    UiRuntimeCommand,
};
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};
use tracing::{debug, info, trace, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Ime, KeyEvent as WinitKeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use winit::platform::startup_notify::{
    EventLoopExtStartupNotify, WindowAttributesExtStartupNotify,
};
use winit::window::{Icon, Window, WindowId};

/// Embedded application icon (decoded at runtime from PNG).
static LOGO_PNG: &[u8] = include_bytes!("../../../LOGO.png");

const DEFAULT_GUI_WIDTH: u32 = 1280;
const DEFAULT_GUI_HEIGHT: u32 = 800;
use rldyourterm_ui::DEFAULT_SCROLLBACK_CAP;
const CHILD_EXIT_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CHILD_EXIT_DRAIN_MAX_WAIT: Duration = Duration::from_millis(750);
#[cfg(test)]
use rldyourterm_render_cpu::{DEFAULT_FG, DEFAULT_FG_U32, resolve_cell_colors};
const CLIPBOARD_PASTE_CAP_BYTES: usize = 64 * 1024;
const PTY_OUTPUT_QUEUE_CAPACITY: usize = 256;
const PTY_OUTPUT_CHUNK_BYTES: usize = 64 * 1024;
const PTY_OUTPUT_RECYCLE_POOL_CAPACITY: usize = PTY_OUTPUT_QUEUE_CAPACITY / 4;
const PTY_OUTPUT_RECYCLE_POOL_WARMUP: usize = 8;
const OUTPUT_BATCH_INITIAL_CAPACITY: usize = 64 * 1024;
const MAX_FEED_BYTES_PER_CALL: usize = terminal_feed_max_bytes_per_call();
const OUTPUT_BATCH_MAX_BYTES: usize = MAX_FEED_BYTES_PER_CALL * 4;
const OUTPUT_DRAIN_MAX_BYTES_PER_TICK: usize = 4 * 1024 * 1024;
const OUTPUT_DRAIN_MAX_LATENCY: Duration = Duration::from_millis(8);
const OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK: usize = 8 * 1024 * 1024;
const OUTPUT_DRAIN_ELEVATED_MAX_LATENCY: Duration = Duration::from_millis(10);
const OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK: usize = 16 * 1024 * 1024;
const OUTPUT_DRAIN_CRITICAL_MAX_LATENCY: Duration = Duration::from_millis(12);
const OUTPUT_DRAIN_ELEVATED_QUEUE_BYTES: usize = 2 * 1024 * 1024;
const OUTPUT_DRAIN_CRITICAL_QUEUE_BYTES: usize = 8 * 1024 * 1024;
const OUTPUT_DRAIN_ELEVATED_QUEUE_CHUNKS: usize = PTY_OUTPUT_QUEUE_CAPACITY / 4;
const OUTPUT_DRAIN_CRITICAL_QUEUE_CHUNKS: usize = (PTY_OUTPUT_QUEUE_CAPACITY * 3) / 4;
const FEED_EVENTS_SCRATCH_INITIAL_CAPACITY: usize = 256;
const DIRTY_ROWS_SCRATCH_INITIAL_CAPACITY: usize = 64;
const MAX_VIEWPORT_COLS: usize = 2_000;
const MAX_VIEWPORT_ROWS: usize = 1_000;
const MAX_VIEWPORT_CELLS: usize = 1_000_000;
const MAX_FRAMEBUFFER_WIDTH: u32 = 16_384;
const MAX_FRAMEBUFFER_HEIGHT: u32 = 16_384;
const MAX_FRAMEBUFFER_PIXELS: u64 = 67_108_864; // 8192 * 8192
#[derive(Debug)]
enum GuiEvent {
    OutputReady,
    Exited(i32),
    PtyFailure {
        boundary: SessionBoundary,
        message: String,
    },
}

type SpawnedPty = (Arc<dyn PtyIo>, Box<dyn Write + Send>, Box<dyn Read + Send>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitorAffectingWindowEvent {
    Moved,
    Resized,
    ScaleFactorChanged,
}

use crate::runtime_shared::display::{fatal_boundary_reason_token, session_boundary_token};
use crate::runtime_shared::io::{is_disconnect_error, write_all_and_flush};
use crate::runtime_shared::spawn_env::ai_cli_spawn_env_overrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PtyBoundaryLoopAction {
    Continue,
    ExitLoop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PtyWriteOutcome {
    Written,
    RecoverableFailure,
    ExitLoop,
}

pub(crate) fn run_interactive_gui_pty(
    shell_executable: &str,
    shell_args: &[String],
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
    window_count: u8,
    clipboard: Arc<dyn ClipboardAdapter>,
) -> Result<i32> {
    if window_count != 1 {
        return Err(anyhow!(
            "gui runtime requires single-window mode; got window_count={window_count}"
        ));
    }

    let (pty, writer, reader) = spawn_pty(shell_executable, shell_args)?;

    let event_loop = build_gui_event_loop()?;
    let proxy = event_loop.create_proxy();

    let (output_tx, output_rx) = sync_channel::<OutputChunk>(PTY_OUTPUT_QUEUE_CAPACITY);
    let (output_recycle_tx, output_recycle_rx) =
        sync_channel::<Vec<u8>>(PTY_OUTPUT_RECYCLE_POOL_CAPACITY);
    warm_output_chunk_pool(&output_recycle_tx);
    let output_event_pending = Arc::new(AtomicBool::new(false));
    let output_backpressure = Arc::new(OutputQueueBackpressure::default());
    let reader_pump = spawn_reader_pump(
        reader,
        proxy.clone(),
        output_tx,
        output_recycle_rx,
        Arc::clone(&output_event_pending),
        Arc::clone(&output_backpressure),
    );
    let wait_pump = spawn_wait_pump(Arc::clone(&pty), proxy.clone());
    let bootstrap = GuiRuntimeBootstrap {
        event_proxy: proxy.clone(),
        initial_mode,
        refresh_rate_millihz,
        window_count,
        output_backpressure,
        clipboard,
    };

    let mut app = GuiRuntimeApp::new(
        pty,
        writer,
        GuiRuntimeChannels {
            output_rx,
            output_recycle_tx,
            output_event_pending,
            reader_pump,
            wait_pump,
        },
        bootstrap,
    )
    .context("failed to initialize GUI runtime app")?;

    info!(
        mode = ?initial_mode,
        active_render_path = ?app.ui_runtime.active_render_path(),
        refresh_rate_millihz,
        windows = window_count,
        "starting GUI runtime"
    );

    event_loop
        .run_app(&mut app)
        .context("GUI event loop failed")?;

    debug!(
        exit_code = ?app.exit_code,
        has_fatal_error = app.fatal_error.is_some(),
        "event loop exited, beginning shutdown"
    );
    app.shutdown();

    if let Some(error) = app.fatal_error.take() {
        return Err(error);
    }

    Ok(app.exit_code.unwrap_or(0))
}

fn build_gui_event_loop() -> Result<EventLoop<GuiEvent>> {
    debug!("building GUI event loop");
    let mut builder = EventLoop::<GuiEvent>::with_user_event();

    #[cfg(target_os = "macos")]
    {
        builder
            .with_activation_policy(ActivationPolicy::Regular)
            .with_activate_ignoring_other_apps(true);
        info!(
            activation_policy = "regular",
            activate_ignoring_other_apps = true,
            "configured macOS GUI event loop activation behavior"
        );
    }

    builder.build().context("failed to create GUI event loop")
}

fn spawn_pty(shell_executable: &str, shell_args: &[String]) -> Result<SpawnedPty> {
    let spawn_env = ai_cli_spawn_env_overrides();
    debug!(
        shell = shell_executable,
        args = ?shell_args,
        env_overrides = spawn_env.len(),
        cols = DEFAULT_TERMINAL_COLS,
        rows = DEFAULT_TERMINAL_ROWS,
        "spawning PTY child process"
    );
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: spawn_env,
        size: PtySize {
            cols: DEFAULT_TERMINAL_COLS,
            rows: DEFAULT_TERMINAL_ROWS,
            pixel_width: 0,
            pixel_height: 0,
        },
    };

    let factory = PlatformPtyFactory;
    let pty_box = factory
        .spawn(spawn_config)
        .context("failed to spawn PTY for GUI runtime")?;

    let reader = pty_box
        .take_reader()
        .context("failed to acquire PTY reader for GUI runtime")?;
    let writer = pty_box
        .take_writer()
        .context("failed to acquire PTY writer for GUI runtime")?;

    let pty: Arc<dyn PtyIo> = Arc::from(pty_box);
    Ok((pty, writer, reader))
}

struct GuiRuntimeApp {
    event_proxy: EventLoopProxy<GuiEvent>,
    pty: Arc<dyn PtyIo>,
    writer: Box<dyn Write + Send>,
    output_rx: Receiver<OutputChunk>,
    output_recycle_tx: SyncSender<Vec<u8>>,
    output_event_pending: Arc<AtomicBool>,
    output_backpressure: Arc<OutputQueueBackpressure>,
    clipboard: Arc<dyn ClipboardAdapter>,
    reader_pump: Option<JoinHandle<()>>,
    wait_pump: Option<JoinHandle<()>>,
    session_policy: SessionController,
    diagnostics: DiagnosticsSink,
    ui_runtime: UiRuntime,
    gpu_renderer: GpuRenderer,
    gpu_cache_dir: Option<PathBuf>,
    started_at: Instant,
    render_backend: RenderBackendCoordinator,

    window: Option<Arc<Window>>,
    window_control: Option<Box<dyn WindowControl>>,
    window_id: Option<WindowId>,
    _context: Option<SoftbufferContext<Arc<Window>>>,
    surface: Option<SoftbufferSurface<Arc<Window>, Arc<Window>>>,
    window_size: PhysicalSize<u32>,
    terminal: TerminalState,
    glyph_cache: GlyphCache,
    settings: SettingsService,
    modifiers: ModifiersState,
    palette_open: bool,
    redraw_pending: bool,
    redraw_in_flight: bool,
    child_exit_pending: bool,
    child_exit_drain_started_at: Option<Instant>,
    last_rendered_cursor_row: Option<u16>,
    last_softbuffer_size: Option<PhysicalSize<u32>>,
    last_viewport_cols: u16,
    last_viewport_rows: u16,
    last_viewport_pixel_width: u16,
    last_viewport_pixel_height: u16,
    output_batch: Vec<u8>,
    response_buffer_scratch: TerminalResponseBuffer,
    dirty_rows_scratch: Vec<u16>,

    exit_code: Option<i32>,
    fatal_error: Option<anyhow::Error>,
}

struct GuiRuntimeBootstrap {
    event_proxy: EventLoopProxy<GuiEvent>,
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
    window_count: u8,
    output_backpressure: Arc<OutputQueueBackpressure>,
    clipboard: Arc<dyn ClipboardAdapter>,
}

struct GuiRuntimeChannels {
    output_rx: Receiver<OutputChunk>,
    output_recycle_tx: SyncSender<Vec<u8>>,
    output_event_pending: Arc<AtomicBool>,
    reader_pump: JoinHandle<()>,
    wait_pump: JoinHandle<()>,
}

impl GuiRuntimeApp {
    fn new(
        pty: Arc<dyn PtyIo>,
        writer: Box<dyn Write + Send>,
        channels: GuiRuntimeChannels,
        bootstrap: GuiRuntimeBootstrap,
    ) -> Result<Self> {
        let GuiRuntimeBootstrap {
            event_proxy,
            initial_mode,
            refresh_rate_millihz,
            window_count,
            output_backpressure,
            clipboard,
        } = bootstrap;
        let GuiRuntimeChannels {
            output_rx,
            output_recycle_tx,
            output_event_pending,
            reader_pump,
            wait_pump,
        } = channels;

        let ui_runtime = UiRuntime::bootstrap(UiBootstrapConfig {
            render_mode: initial_mode,
            refresh_rate_millihz,
            window_count,
            scrollback_cap: DEFAULT_SCROLLBACK_CAP,
        })
        .context("failed to bootstrap UI runtime for GUI app")?;
        let mut session_policy = SessionController::new();
        session_policy
            .mark_running()
            .context("failed to initialize GUI session boundary policy")?;

        Ok(Self {
            event_proxy,
            pty,
            writer,
            output_rx,
            output_recycle_tx,
            output_event_pending,
            output_backpressure,
            clipboard,
            reader_pump: Some(reader_pump),
            wait_pump: Some(wait_pump),
            session_policy,
            diagnostics: DiagnosticsSink::default(),
            ui_runtime,
            gpu_renderer: GpuRenderer::default(),
            gpu_cache_dir: app_handler::resolve_gpu_cache_dir(),
            started_at: Instant::now(),
            render_backend: RenderBackendCoordinator::new(initial_mode),
            window: None,
            window_control: None,
            window_id: None,
            _context: None,
            surface: None,
            window_size: PhysicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT),
            terminal: TerminalState::new(
                DEFAULT_TERMINAL_COLS,
                DEFAULT_TERMINAL_ROWS,
                DEFAULT_SCROLLBACK_CAP,
            ),
            glyph_cache: GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16),
            settings: SettingsService::default(),
            modifiers: ModifiersState::default(),
            palette_open: false,
            redraw_pending: true,
            redraw_in_flight: false,
            child_exit_pending: false,
            child_exit_drain_started_at: None,
            last_rendered_cursor_row: None,
            last_softbuffer_size: None,
            last_viewport_cols: DEFAULT_TERMINAL_COLS,
            last_viewport_rows: DEFAULT_TERMINAL_ROWS,
            last_viewport_pixel_width: DEFAULT_GUI_WIDTH.min(u16::MAX as u32) as u16,
            last_viewport_pixel_height: DEFAULT_GUI_HEIGHT.min(u16::MAX as u32) as u16,
            output_batch: Vec::with_capacity(OUTPUT_BATCH_INITIAL_CAPACITY),
            response_buffer_scratch: TerminalResponseBuffer::with_capacity(
                FEED_EVENTS_SCRATCH_INITIAL_CAPACITY,
            ),
            dirty_rows_scratch: Vec::with_capacity(DIRTY_ROWS_SCRATCH_INITIAL_CAPACITY),
            exit_code: None,
            fatal_error: None,
        })
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }
}

#[cfg(test)]
use app_handler::child_exit_drain_timed_out;

#[cfg(test)]
fn is_runtime_palette_shortcut_key(key: Key<&str>, modifiers: ModifiersState) -> bool {
    is_runtime_palette_shortcut_winit(key, modifiers)
}

#[cfg(test)]
fn encode_winit_key_event(key: &Key, modifiers: ModifiersState) -> Option<Vec<u8>> {
    crate::runtime_shared::input::runtime_key_event_from_winit(key, modifiers)
        .and_then(crate::runtime_shared::input::encode_runtime_key_event)
}

#[cfg(test)]
mod tests;
