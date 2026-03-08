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

#[cfg(test)]
use self::output::{
    OutputDrainBudget, OutputDrainPressure, OutputQueueSnapshot, take_output_chunk_buffer,
};
use self::output::{
    OutputQueueBackpressure, output_drain_budget, output_drain_budget_exhausted,
    recycle_output_chunk_buffer, should_flush_output_batch, spawn_reader_pump, spawn_wait_pump,
    warm_output_chunk_pool,
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
    BoundaryFailureOutcome, apply_pty_boundary_failure, fatal_pty_boundary_failure,
    mark_pty_boundary_recovered as shared_mark_pty_boundary_recovered, runtime_boundary_notice,
};
use crate::runtime_shared::shutdown::{
    JoinThreadOutcome, child_exit_drain_timed_out as shared_child_exit_drain_timed_out,
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
use rldyourterm_ui::{UiBootstrapConfig, UiCommandOutcome, UiRuntime, UiRuntimeCommand};
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
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
use rldyourterm_ui::DEFAULT_SCROLLBACK_CAP;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
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

use crate::shared::{
    ai_cli_spawn_env_overrides, fatal_boundary_reason_token, is_disconnect_error,
    session_boundary_token, write_all_and_flush,
};

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

    let (output_tx, output_rx) = sync_channel::<Vec<u8>>(PTY_OUTPUT_QUEUE_CAPACITY);
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
        cols = DEFAULT_COLS,
        rows = DEFAULT_ROWS,
        "spawning PTY child process"
    );
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: spawn_env,
        size: PtySize {
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
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
    output_rx: Receiver<Vec<u8>>,
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
    output_rx: Receiver<Vec<u8>>,
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
            gpu_cache_dir: resolve_gpu_cache_dir(),
            started_at: Instant::now(),
            render_backend: RenderBackendCoordinator::new(initial_mode),
            window: None,
            window_control: None,
            window_id: None,
            _context: None,
            surface: None,
            window_size: PhysicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT),
            terminal: TerminalState::new(DEFAULT_COLS, DEFAULT_ROWS, DEFAULT_SCROLLBACK_CAP),
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
            last_viewport_cols: DEFAULT_COLS,
            last_viewport_rows: DEFAULT_ROWS,
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

    fn dispatch_terminal_responses(
        &mut self,
        responses: &TerminalResponseBuffer,
        event_loop: &ActiveEventLoop,
    ) -> bool {
        let mut emitted_terminal_response = false;
        let mut saw_write_error = false;
        let mut should_exit = false;
        responses.for_each_terminal_response(|data| {
            if should_exit {
                return;
            }
            emitted_terminal_response = true;
            trace!(bytes = data.len(), "sending terminal response to PTY");
            match self.write_pty_chunk(data, event_loop, "failed to write terminal response to PTY")
            {
                PtyWriteOutcome::Written => {}
                PtyWriteOutcome::RecoverableFailure => {
                    saw_write_error = true;
                }
                PtyWriteOutcome::ExitLoop => {
                    should_exit = true;
                }
            }
        });
        if should_exit {
            return false;
        }
        !emitted_terminal_response || saw_write_error || self.finish_pty_write(event_loop)
    }

    fn reader_pump_finished(&self) -> bool {
        match self.reader_pump.as_ref() {
            Some(handle) => handle.is_finished(),
            None => true,
        }
    }

    fn child_exit_drain_complete(&self) -> bool {
        self.reader_pump_finished()
            && !self.output_event_pending.load(Ordering::Acquire)
            && self.output_batch.is_empty()
    }

    fn begin_child_exit_drain(&mut self, event_loop: &ActiveEventLoop) {
        self.child_exit_pending = true;
        self.child_exit_drain_started_at
            .get_or_insert_with(Instant::now);
        self.drain_output_queue(event_loop);
        if self.child_exit_drain_complete() {
            self.child_exit_pending = false;
            self.child_exit_drain_started_at = None;
            event_loop.exit();
        }
    }

    fn shutdown(&mut self) {
        debug!(
            exit_code = ?self.exit_code,
            has_fatal_error = self.fatal_error.is_some(),
            "shutdown: beginning teardown"
        );

        self.persist_gpu_pipeline_cache();

        let child_exited = self.exit_code.is_some() || self.pty.try_wait().ok().flatten().is_some();
        if child_exited {
            if let Some(handle) = self.reader_pump.take() {
                join_pump_thread_with_timeout(handle, "reader_pump");
            }
            if let Some(handle) = self.wait_pump.take() {
                join_pump_thread_with_timeout(handle, "wait_pump");
            }
            if let Err(error) = self.pty.close() {
                warn!(error = %error, "failed to close PTY during GUI shutdown");
                if self.fatal_error.is_none() {
                    self.fatal_error = Some(anyhow!("failed to close PTY: {error}"));
                }
            }
            return;
        }

        if let Err(error) = self.pty.close() {
            warn!(error = %error, "failed to close PTY during GUI shutdown");
            if self.fatal_error.is_none() {
                self.fatal_error = Some(anyhow!("failed to close PTY: {error}"));
            }
        }

        if let Some(handle) = self.reader_pump.take() {
            join_pump_thread_with_timeout(handle, "reader_pump");
        }

        if let Some(handle) = self.wait_pump.take() {
            join_pump_thread_with_timeout(handle, "wait_pump");
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }

    fn emit_runtime_notice(&mut self, message: &str) {
        let mut line = String::from("\r\n");
        line.push_str(message);
        line.push_str("\r\n");
        self.response_buffer_scratch
            .feed_terminal(&mut self.terminal, line.as_bytes());
        self.queue_redraw();
    }

    fn request_redraw_if_needed(&mut self) {
        if !self.redraw_pending || self.redraw_in_flight {
            return;
        }
        if (self.window_control.is_some() || self.window.is_some()) && self.request_window_redraw()
        {
            self.redraw_pending = false;
            self.redraw_in_flight = true;
        }
    }

    fn apply_output_bytes(&mut self, data: &[u8], event_loop: &ActiveEventLoop) -> bool {
        trace!(bytes = data.len(), "pty output received");
        let mut response_buffer = std::mem::take(&mut self.response_buffer_scratch);
        for chunk in terminal_feed_chunks(data) {
            response_buffer.feed_terminal(&mut self.terminal, chunk);
            if !self.dispatch_terminal_responses(&response_buffer, event_loop) {
                self.response_buffer_scratch = response_buffer;
                return false;
            }
        }
        self.response_buffer_scratch = response_buffer;
        true
    }

    fn flush_output_batch(&mut self, batch: &mut Vec<u8>, event_loop: &ActiveEventLoop) -> bool {
        if batch.is_empty() {
            return true;
        }
        if !self.apply_output_bytes(batch.as_slice(), event_loop) {
            return false;
        }
        batch.clear();
        true
    }

    fn append_output_chunk_to_batch(
        &mut self,
        batch: &mut Vec<u8>,
        data: &[u8],
        event_loop: &ActiveEventLoop,
    ) -> bool {
        if data.len() >= OUTPUT_BATCH_MAX_BYTES {
            if !self.flush_output_batch(batch, event_loop) {
                return false;
            }
            return self.apply_output_bytes(data, event_loop);
        }
        if should_flush_output_batch(batch.len(), data.len())
            && !self.flush_output_batch(batch, event_loop)
        {
            return false;
        }
        batch.extend_from_slice(data);
        true
    }

    fn recycle_output_chunk(&self, chunk: Vec<u8>) {
        recycle_output_chunk_buffer(&self.output_recycle_tx, chunk);
    }

    fn drain_output_queue(&mut self, event_loop: &ActiveEventLoop) {
        let mut drained_any = false;
        let mut batch = std::mem::take(&mut self.output_batch);
        let drain_started = Instant::now();
        let mut drained_bytes = 0usize;
        let mut budget_exhausted = false;
        let mut active_budget = output_drain_budget(self.output_backpressure.snapshot());

        'drain: loop {
            while let Ok(data) = self.output_rx.try_recv() {
                self.output_backpressure.note_dequeue(data.len());
                drained_any = true;
                drained_bytes = drained_bytes.saturating_add(data.len());
                if !self.append_output_chunk_to_batch(&mut batch, &data, event_loop) {
                    self.recycle_output_chunk(data);
                    self.output_batch = batch;
                    return;
                }
                self.recycle_output_chunk(data);
                active_budget = output_drain_budget(self.output_backpressure.snapshot());
                if output_drain_budget_exhausted(
                    drained_bytes,
                    drain_started.elapsed(),
                    active_budget,
                ) {
                    budget_exhausted = true;
                    break 'drain;
                }
            }

            // Release the pending flag only after we observed queue empty.
            self.output_event_pending.store(false, Ordering::Release);

            // Handle producer race: data may arrive between empty check and flag reset.
            match self.output_rx.try_recv() {
                Ok(data) => {
                    self.output_backpressure.note_dequeue(data.len());
                    self.output_event_pending.store(true, Ordering::Release);
                    drained_any = true;
                    drained_bytes = drained_bytes.saturating_add(data.len());
                    if !self.append_output_chunk_to_batch(&mut batch, &data, event_loop) {
                        self.recycle_output_chunk(data);
                        self.output_batch = batch;
                        return;
                    }
                    self.recycle_output_chunk(data);
                    active_budget = output_drain_budget(self.output_backpressure.snapshot());
                    if output_drain_budget_exhausted(
                        drained_bytes,
                        drain_started.elapsed(),
                        active_budget,
                    ) {
                        budget_exhausted = true;
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        if !self.flush_output_batch(&mut batch, event_loop) {
            self.output_batch = batch;
            return;
        }
        if batch.capacity() > OUTPUT_BATCH_MAX_BYTES * 2 {
            batch.shrink_to(OUTPUT_BATCH_INITIAL_CAPACITY);
        }
        self.output_batch = batch;

        if !drained_any {
            return;
        }

        if budget_exhausted {
            let queue_snapshot = self.output_backpressure.snapshot();
            debug!(
                drained_bytes,
                elapsed_ms = drain_started.elapsed().as_millis(),
                queue_pressure = ?active_budget.pressure,
                queue_bytes = queue_snapshot.queued_bytes,
                queue_chunks = queue_snapshot.queued_chunks,
                drain_byte_budget = active_budget.max_bytes_per_tick,
                drain_latency_budget_ms = active_budget.max_latency.as_millis(),
                "output drain budget exhausted; scheduling continuation"
            );
            let _ = self.event_proxy.send_event(GuiEvent::OutputReady);
        }

        let title = self.terminal.window_title();
        if !title.is_empty() {
            self.set_window_title(title);
        }

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyRead) {
            self.fatal_error = Some(error);
            event_loop.exit();
            return;
        }
        self.queue_redraw();
    }
}

impl ApplicationHandler<GuiEvent> for GuiRuntimeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        debug!(
            window_exists = self.window.is_some(),
            "ApplicationHandler::resumed fired"
        );
        if let Err(error) = self.bootstrap_window(event_loop) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        warn!("ApplicationHandler::suspended fired by compositor");
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        debug!(
            exit_code = ?self.exit_code,
            has_fatal_error = self.fatal_error.is_some(),
            "ApplicationHandler::exiting - event loop shutting down"
        );
        self.persist_gpu_pipeline_cache();
        // Release window resources while the Wayland/X11 connection is still
        // alive.  This ensures the compositor receives surface-destroy and
        // removes the window from the dock/taskbar.
        self.release_window_resources();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: GuiEvent) {
        match event {
            GuiEvent::OutputReady => self.drain_output_queue(event_loop),
            GuiEvent::Exited(code) => {
                info!(
                    exit_code = code,
                    "child process exited; draining pending output"
                );
                self.exit_code = Some(code);
                self.begin_child_exit_drain(event_loop);
            }
            GuiEvent::PtyFailure {
                ref boundary,
                ref message,
            } => {
                warn!(?boundary, %message, "pty boundary failure event");
                if *boundary == SessionBoundary::PtyWait {
                    self.fatal_error = Some(fatal_pty_boundary_failure(
                        &mut self.session_policy,
                        *boundary,
                        message,
                    ));
                    event_loop.exit();
                    return;
                }
                if *boundary == SessionBoundary::PtyRead {
                    if self.exit_code.is_some() {
                        self.begin_child_exit_drain(event_loop);
                        return;
                    }
                    match self
                        .pty
                        .try_wait()
                        .context("failed to poll PTY after reader boundary failure")
                    {
                        Ok(Some(code)) => {
                            self.exit_code = Some(code);
                            info!(
                                exit_code = code,
                                "reader boundary reported after child exit; draining remaining output"
                            );
                            self.begin_child_exit_drain(event_loop);
                            return;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.fatal_error = Some(error);
                            event_loop.exit();
                            return;
                        }
                    }
                }
                match self.handle_pty_boundary_failure(*boundary, message) {
                    Ok(PtyBoundaryLoopAction::Continue) => {}
                    Ok(PtyBoundaryLoopAction::ExitLoop) => {
                        event_loop.exit();
                    }
                    Err(policy_error) => {
                        self.fatal_error = Some(policy_error);
                        event_loop.exit();
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => self.handle_close_requested(event_loop),
            WindowEvent::RedrawRequested => {
                self.redraw_in_flight = false;
                if let Err(error) = self.draw_frame() {
                    self.fatal_error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::Moved(_) => {
                self.handle_monitor_affecting_event(MonitorAffectingWindowEvent::Moved);
            }
            WindowEvent::Resized(size) => {
                self.apply_window_extent_change(
                    event_loop,
                    size,
                    "ignoring zero-sized resize event to avoid synthetic PTY geometry",
                    "window framebuffer exceeded runtime safety limits; dimensions were clamped",
                    MonitorAffectingWindowEvent::Resized,
                );
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    self.apply_window_extent_change(
                        event_loop,
                        window.inner_size(),
                        "ignoring zero-sized scale-factor resize event to avoid synthetic PTY geometry",
                        "scale-factor framebuffer exceeded runtime safety limits; dimensions were clamped",
                        MonitorAffectingWindowEvent::ScaleFactorChanged,
                    );
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } if !is_synthetic => self.handle_keyboard_input(&event, event_loop),
            WindowEvent::Ime(Ime::Commit(text)) => self.handle_text_commit(&text, event_loop),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Focused(focused) => {
                debug!(focused, "window focus changed");
            }
            WindowEvent::Occluded(occluded) => {
                debug!(occluded, "window occlusion changed");
            }
            WindowEvent::Destroyed => {
                warn!("window destroyed by compositor");
            }
            _ => {
                trace!(event = ?event, "unhandled window event");
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.render_backend.deferred_gpu_init_pending() {
            self.try_deferred_gpu_init(event_loop);
        }
        if self.fatal_error.is_some() {
            event_loop.exit();
            return;
        }
        if self.child_exit_pending {
            let now = Instant::now();
            self.drain_output_queue(event_loop);
            if self.child_exit_drain_complete() {
                self.child_exit_pending = false;
                self.child_exit_drain_started_at = None;
                event_loop.exit();
            } else if self
                .child_exit_drain_started_at
                .map(|started_at| child_exit_drain_timed_out(started_at, now))
                .unwrap_or(false)
            {
                let elapsed_ms = self
                    .child_exit_drain_started_at
                    .map(|started_at| now.saturating_duration_since(started_at).as_millis())
                    .unwrap_or(0);
                warn!(
                    elapsed_ms,
                    max_wait_ms = CHILD_EXIT_DRAIN_MAX_WAIT.as_millis(),
                    "child-exit output drain exceeded max wait budget; forcing shutdown"
                );
                self.child_exit_pending = false;
                self.child_exit_drain_started_at = None;
                event_loop.exit();
            } else {
                event_loop
                    .set_control_flow(ControlFlow::WaitUntil(now + CHILD_EXIT_DRAIN_POLL_INTERVAL));
            }
            return;
        }
        if self.render_backend.deferred_gpu_init_pending()
            && let Some(retry_at) = self.render_backend.deferred_retry_deadline()
            && Instant::now() < retry_at
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(retry_at));
            return;
        }

        self.request_redraw_if_needed();
        let wait_policy = self.render_backend.wait_policy(
            self.ui_runtime.active_render_path(),
            self.gpu_renderer.is_initialized(),
            self.redraw_pending,
            self.ui_runtime.cadence().frame_interval(),
        );

        trace!(
            render_path = ?self.ui_runtime.active_render_path(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            wait_policy = ?wait_policy,
            "about_to_wait: selecting control flow"
        );
        event_loop.set_control_flow(wait_policy.control_flow(Instant::now()));
    }
}

fn join_pump_thread_with_timeout(handle: JoinHandle<()>, thread_label: &'static str) {
    if matches!(
        shared_join_thread_with_timeout(
            handle,
            SHUTDOWN_JOIN_TIMEOUT,
            SHUTDOWN_JOIN_POLL_INTERVAL,
            thread_label,
        ),
        JoinThreadOutcome::TimedOut
    ) {
        warn!(
            thread_label,
            timeout_ms = SHUTDOWN_JOIN_TIMEOUT.as_millis(),
            "GUI shutdown thread join timed out; detaching thread to avoid shutdown hang"
        );
    }
}

#[cfg(test)]
fn is_runtime_palette_shortcut_key(key: Key<&str>, modifiers: ModifiersState) -> bool {
    is_runtime_palette_shortcut_winit(key, modifiers)
}

#[cfg(test)]
fn encode_winit_key_event(key: &Key, modifiers: ModifiersState) -> Option<Vec<u8>> {
    crate::runtime_shared::input::runtime_key_event_from_winit(key, modifiers)
        .and_then(crate::runtime_shared::input::encode_runtime_key_event)
}

fn resolve_gpu_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches/rldyourterm"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
            Some(PathBuf::from(xdg).join("rldyourterm"))
        } else {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/rldyourterm"))
        }
    }
}

fn child_exit_drain_timed_out(started_at: Instant, now: Instant) -> bool {
    shared_child_exit_drain_timed_out(started_at, now, CHILD_EXIT_DRAIN_MAX_WAIT)
}

#[cfg(test)]
mod tests {
    use super::{
        BackendSyncAction, CHILD_EXIT_DRAIN_MAX_WAIT, CLIPBOARD_PASTE_CAP_BYTES, DEFAULT_FG,
        DEFAULT_FG_U32, DeferredGpuInitState, GpuFailureHandling, MAX_FEED_BYTES_PER_CALL,
        MAX_FRAMEBUFFER_HEIGHT, MAX_FRAMEBUFFER_PIXELS, MAX_FRAMEBUFFER_WIDTH, MAX_VIEWPORT_CELLS,
        MAX_VIEWPORT_COLS, MAX_VIEWPORT_ROWS, MonitorAffectingWindowEvent, OUTPUT_BATCH_MAX_BYTES,
        OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_MAX_LATENCY, OutputDrainBudget,
        OutputDrainPressure, OutputQueueSnapshot, PTY_OUTPUT_CHUNK_BYTES,
        PTY_OUTPUT_RECYCLE_POOL_WARMUP, RenderBackendCoordinator, RenderWaitPolicy,
        ViewportGeometry, cadence_resync_command_for_monitor_event, cap_framebuffer_extent,
        cap_paste_text, cap_terminal_geometry, child_exit_drain_timed_out,
        deferred_gpu_init_backoff, dispatch_gpu_failure_command, dispatch_runtime_palette_command,
        emit_gpu_auto_fallback_observability, encode_winit_key_event,
        is_runtime_palette_shortcut_key, output_drain_budget, output_drain_budget_exhausted,
        read_clipboard_text_for_paste, recycle_output_chunk_buffer, render_wait_policy,
        resolve_cell_colors, sample_monitor_refresh_rate_millihz, should_flush_output_batch,
        take_output_chunk_buffer, terminal_feed_chunks, viewport_geometry_changed,
        warm_output_chunk_pool,
    };
    use crate::shared::{PtyBoundaryPolicyDecision, classify_pty_boundary_failure};
    use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
    use rldyourterm_foundation::api::{
        clipboard::ClipboardAdapter,
        common::{ContractResult, MonitorTiming},
        window::WindowControl,
    };
    use rldyourterm_foundation::error::{
        ClipboardFailureCode, ClipboardOperation, FoundationError, Recoverability,
        WindowFailureCode, WindowOperation,
    };
    use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
    use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
    use rldyourterm_services::terminal::{ANSI_PALETTE, Attrs, Color, color_to_u32};
    use rldyourterm_settings::SettingsService;
    use rldyourterm_ui::{UiBootstrapConfig, UiRuntime, UiRuntimeCommand};
    use std::sync::mpsc::sync_channel;
    use std::time::{Duration, Instant};
    use winit::dpi::PhysicalSize;
    use winit::keyboard::{Key, ModifiersState};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StubClipboardScenario {
        Text(&'static str),
        Empty,
        Error,
    }

    struct StubClipboard {
        scenario: StubClipboardScenario,
    }

    impl ClipboardAdapter for StubClipboard {
        fn set_text(&self, _text: &str) -> ContractResult<()> {
            Ok(())
        }

        fn get_text(&self) -> ContractResult<Option<String>> {
            match self.scenario {
                StubClipboardScenario::Text(text) => Ok(Some(text.to_owned())),
                StubClipboardScenario::Empty => Ok(None),
                StubClipboardScenario::Error => Err(FoundationError::clipboard(
                    ClipboardOperation::GetText,
                    ClipboardFailureCode::BoundaryFault,
                    Recoverability::Degrade,
                    "test clipboard failure",
                    None,
                )),
            }
        }

        fn clear(&self) -> ContractResult<()> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StubWindowControlScenario {
        Timing(Option<u32>),
        Error,
    }

    struct StubWindowControl {
        scenario: StubWindowControlScenario,
    }

    impl WindowControl for StubWindowControl {
        fn request_redraw(&self) -> ContractResult<()> {
            Ok(())
        }

        fn set_title(&self, _title: &str) -> ContractResult<()> {
            Ok(())
        }

        fn current_monitor_timing(&self) -> ContractResult<MonitorTiming> {
            match self.scenario {
                StubWindowControlScenario::Timing(refresh_rate_millihz) => Ok(MonitorTiming {
                    monitor_name: Some("stub-monitor".to_owned()),
                    refresh_rate_millihz,
                }),
                StubWindowControlScenario::Error => Err(FoundationError::window(
                    WindowOperation::QueryMonitorTiming,
                    WindowFailureCode::BoundaryFault,
                    Recoverability::Degrade,
                    "monitor timing unavailable",
                    None,
                )),
            }
        }

        fn close(&self) -> ContractResult<()> {
            Ok(())
        }
    }

    #[test]
    fn clipboard_dispatch_returns_non_empty_text() {
        let clipboard = StubClipboard {
            scenario: StubClipboardScenario::Text("wave3"),
        };
        assert_eq!(
            read_clipboard_text_for_paste(&clipboard),
            Some("wave3".to_owned())
        );
    }

    #[test]
    fn clipboard_dispatch_ignores_empty_text() {
        let clipboard = StubClipboard {
            scenario: StubClipboardScenario::Empty,
        };
        assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
    }

    #[test]
    fn clipboard_dispatch_ignores_adapter_errors() {
        let clipboard = StubClipboard {
            scenario: StubClipboardScenario::Error,
        };
        assert_eq!(read_clipboard_text_for_paste(&clipboard), None);
    }

    #[test]
    fn clipboard_paste_cap_limits_payload_to_64kb() {
        let payload = "x".repeat(70 * 1024);
        assert_eq!(cap_paste_text(&payload).len(), CLIPBOARD_PASTE_CAP_BYTES);
    }

    #[test]
    fn clipboard_paste_cap_preserves_utf8_boundary() {
        let payload = format!("{}🚀", "a".repeat(CLIPBOARD_PASTE_CAP_BYTES - 1));
        let capped = cap_paste_text(&payload);
        assert_eq!(capped.len(), CLIPBOARD_PASTE_CAP_BYTES - 1);
        assert_eq!(capped.chars().last(), Some('a'));
    }

    #[test]
    fn viewport_geometry_cap_preserves_small_dimensions() {
        assert_eq!(cap_terminal_geometry(120, 32), (120, 32));
    }

    #[test]
    fn viewport_geometry_change_detects_pixel_only_updates() {
        assert!(viewport_geometry_changed(
            ViewportGeometry {
                cols: 120,
                rows: 32,
                pixel_width: 1280,
                pixel_height: 800,
            },
            ViewportGeometry {
                cols: 120,
                rows: 32,
                pixel_width: 1400,
                pixel_height: 800,
            }
        ));
        assert!(!viewport_geometry_changed(
            ViewportGeometry {
                cols: 120,
                rows: 32,
                pixel_width: 1280,
                pixel_height: 800,
            },
            ViewportGeometry {
                cols: 120,
                rows: 32,
                pixel_width: 1280,
                pixel_height: 800,
            }
        ));
    }

    #[test]
    fn viewport_geometry_cap_enforces_axis_and_cell_limits() {
        let (cols, rows) = cap_terminal_geometry(20_000, 20_000);
        assert!(cols as usize <= MAX_VIEWPORT_COLS);
        assert!(rows as usize <= MAX_VIEWPORT_ROWS);
        assert!((cols as usize) * (rows as usize) <= MAX_VIEWPORT_CELLS);
    }

    #[test]
    fn framebuffer_extent_cap_preserves_small_dimensions() {
        let capped = cap_framebuffer_extent(PhysicalSize::new(1920, 1080));
        assert_eq!(capped, PhysicalSize::new(1920, 1080));
    }

    #[test]
    fn framebuffer_extent_cap_enforces_axis_and_pixel_limits() {
        let capped = cap_framebuffer_extent(PhysicalSize::new(100_000, 100_000));
        assert!(capped.width <= MAX_FRAMEBUFFER_WIDTH);
        assert!(capped.height <= MAX_FRAMEBUFFER_HEIGHT);
        assert!(u64::from(capped.width) * u64::from(capped.height) <= MAX_FRAMEBUFFER_PIXELS);
    }

    #[test]
    fn deferred_gpu_init_backoff_is_bounded_and_monotonic() {
        let first = deferred_gpu_init_backoff(1);
        let second = deferred_gpu_init_backoff(2);
        let third = deferred_gpu_init_backoff(3);
        let fourth = deferred_gpu_init_backoff(4);
        let saturated = deferred_gpu_init_backoff(8);

        assert!(first <= second);
        assert!(second <= third);
        assert!(third <= fourth);
        assert_eq!(fourth, saturated);
    }

    #[test]
    fn deferred_gpu_init_state_transitions_are_consistent() {
        let mut state = DeferredGpuInitState::new(true);
        assert!(state.is_pending());
        assert_eq!(state.next_attempt(), 1);
        assert_eq!(state.begin_attempt(), 1);
        assert_eq!(state.retry_deadline(), None);

        state.schedule_retry(1, Duration::from_millis(10));
        assert!(state.is_pending());
        assert_eq!(state.next_attempt(), 2);
        assert!(state.retry_deadline().is_some());

        state.record_failure_attempt(2);
        assert_eq!(state.next_attempt(), 3);

        state.mark_exhausted(2);
        assert!(!state.is_pending());
        assert_eq!(state.retry_deadline(), None);
        assert_eq!(state.next_attempt(), 3);

        state.sync_with_target_path(ActiveRenderPath::Gpu, false);
        assert!(state.is_pending());
        assert_eq!(state.next_attempt(), 3);

        state.mark_ready();
        assert!(!state.is_pending());
        assert_eq!(state.next_attempt(), 1);

        state.sync_with_target_path(ActiveRenderPath::Cpu, false);
        assert!(!state.is_pending());
        assert_eq!(state.next_attempt(), 1);
        assert_eq!(state.retry_deadline(), None);
    }

    #[test]
    fn render_backend_coordinator_tracks_sequences_and_sync_policy() {
        let mut coordinator = RenderBackendCoordinator::new(RenderMode::Auto);

        assert!(coordinator.deferred_gpu_init_pending());
        assert_eq!(coordinator.begin_render_attempt(), 1);
        assert_eq!(coordinator.begin_render_attempt(), 2);
        assert_eq!(coordinator.current_render_attempt_sequence(), 2);
        assert_eq!(coordinator.next_gpu_failure_sequence(), 1);
        assert_eq!(coordinator.next_gpu_failure_sequence(), 2);

        assert_eq!(
            coordinator.sync_with_target_path(ActiveRenderPath::Cpu, true),
            BackendSyncAction::ReleaseGpuBackend
        );
        assert!(!coordinator.deferred_gpu_init_pending());

        coordinator.mark_deferred_ready();
        assert_eq!(
            coordinator.wait_policy(
                ActiveRenderPath::Gpu,
                true,
                true,
                Some(Duration::from_millis(16))
            ),
            RenderWaitPolicy::EventDriven
        );
    }

    #[test]
    fn render_wait_policy_uses_event_driven_gpu_lane() {
        assert_eq!(
            render_wait_policy(true, true, Some(Duration::from_millis(8))),
            RenderWaitPolicy::EventDriven
        );
    }

    #[test]
    fn render_wait_policy_uses_cpu_cadence_when_dirty() {
        assert_eq!(
            render_wait_policy(false, true, Some(Duration::from_millis(16))),
            RenderWaitPolicy::CadenceTimed(Duration::from_millis(16))
        );
    }

    #[test]
    fn render_wait_policy_falls_back_to_event_driven_without_cadence() {
        assert_eq!(
            render_wait_policy(false, true, None),
            RenderWaitPolicy::EventDriven
        );
        assert_eq!(
            render_wait_policy(false, false, Some(Duration::from_millis(16))),
            RenderWaitPolicy::EventDriven
        );
    }

    #[test]
    fn output_batch_flush_policy_only_triggers_on_overflow_with_existing_batch() {
        assert!(!should_flush_output_batch(0, 32));
        assert!(!should_flush_output_batch(128, 256));
        assert!(should_flush_output_batch(OUTPUT_BATCH_MAX_BYTES - 64, 128));
    }

    #[test]
    fn output_chunk_pool_warmup_stops_at_channel_capacity() {
        let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(2);
        warm_output_chunk_pool(&recycle_tx);
        let chunk_count = recycle_rx.try_iter().count();
        assert_eq!(chunk_count, PTY_OUTPUT_RECYCLE_POOL_WARMUP.min(2));
    }

    #[test]
    fn output_chunk_take_reuses_preallocated_buffer_when_available() {
        let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
        let mut seeded = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
        let seeded_ptr = seeded.as_ptr();
        seeded.truncate(32);
        recycle_tx.send(seeded).expect("seed recycle buffer");

        let reused = take_output_chunk_buffer(&recycle_rx);
        assert_eq!(reused.len(), PTY_OUTPUT_CHUNK_BYTES);
        assert_eq!(reused.as_ptr(), seeded_ptr);
    }

    #[test]
    fn output_chunk_recycle_roundtrip_preserves_allocation() {
        let (recycle_tx, recycle_rx) = sync_channel::<Vec<u8>>(1);
        let mut chunk = vec![0_u8; PTY_OUTPUT_CHUNK_BYTES];
        let ptr = chunk.as_ptr();
        chunk.truncate(777);
        recycle_output_chunk_buffer(&recycle_tx, chunk);

        let roundtrip = take_output_chunk_buffer(&recycle_rx);
        assert_eq!(roundtrip.len(), PTY_OUTPUT_CHUNK_BYTES);
        assert_eq!(roundtrip.as_ptr(), ptr);
    }

    #[test]
    fn output_drain_budget_triggers_on_byte_limit() {
        let budget = OutputDrainBudget {
            pressure: OutputDrainPressure::Normal,
            max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_MAX_LATENCY,
        };
        assert!(!output_drain_budget_exhausted(
            OUTPUT_DRAIN_MAX_BYTES_PER_TICK - 1,
            Duration::ZERO,
            budget,
        ));
        assert!(output_drain_budget_exhausted(
            OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
            Duration::ZERO,
            budget,
        ));
    }

    #[test]
    fn output_drain_budget_triggers_on_elapsed_limit() {
        let budget = OutputDrainBudget {
            pressure: OutputDrainPressure::Normal,
            max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_MAX_LATENCY,
        };
        assert!(!output_drain_budget_exhausted(
            0,
            OUTPUT_DRAIN_MAX_LATENCY.saturating_sub(Duration::from_millis(1)),
            budget,
        ));
        assert!(output_drain_budget_exhausted(
            0,
            OUTPUT_DRAIN_MAX_LATENCY,
            budget
        ));
    }

    #[test]
    fn output_drain_budget_escalates_with_queue_pressure() {
        let normal = output_drain_budget(OutputQueueSnapshot {
            queued_bytes: 0,
            queued_chunks: 0,
        });
        assert_eq!(normal.pressure, OutputDrainPressure::Normal);
        assert_eq!(normal.max_bytes_per_tick, OUTPUT_DRAIN_MAX_BYTES_PER_TICK);

        let elevated = output_drain_budget(OutputQueueSnapshot {
            queued_bytes: 3 * 1024 * 1024,
            queued_chunks: 4,
        });
        assert_eq!(elevated.pressure, OutputDrainPressure::Elevated);
        assert_eq!(
            elevated.max_bytes_per_tick,
            OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK
        );

        let critical = output_drain_budget(OutputQueueSnapshot {
            queued_bytes: 10 * 1024 * 1024,
            queued_chunks: 220,
        });
        assert_eq!(critical.pressure, OutputDrainPressure::Critical);
        assert_eq!(
            critical.max_bytes_per_tick,
            OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK
        );
    }

    #[test]
    fn child_exit_drain_timeout_boundary_is_deterministic() {
        let started_at = Instant::now();
        assert!(!child_exit_drain_timed_out(
            started_at,
            started_at + CHILD_EXIT_DRAIN_MAX_WAIT.saturating_sub(Duration::from_millis(1))
        ));
        assert!(child_exit_drain_timed_out(
            started_at,
            started_at + CHILD_EXIT_DRAIN_MAX_WAIT
        ));
    }

    #[test]
    fn terminal_feed_chunking_respects_core_per_call_limit() {
        let payload = vec![b'x'; MAX_FEED_BYTES_PER_CALL * 2 + 17];
        let chunk_sizes: Vec<usize> = terminal_feed_chunks(&payload)
            .map(|chunk| chunk.len())
            .collect();
        assert_eq!(
            chunk_sizes,
            vec![MAX_FEED_BYTES_PER_CALL, MAX_FEED_BYTES_PER_CALL, 17]
        );
    }

    #[test]
    fn detects_palette_shortcut_with_ctrl_or_cmd_shift_p() {
        let key = Key::Character("p".into());
        assert!(is_runtime_palette_shortcut_key(
            key.as_ref(),
            ModifiersState::CONTROL | ModifiersState::SHIFT
        ));
        assert!(is_runtime_palette_shortcut_key(
            key.as_ref(),
            ModifiersState::SUPER | ModifiersState::SHIFT
        ));
        assert!(!is_runtime_palette_shortcut_key(
            key.as_ref(),
            ModifiersState::CONTROL
        ));
        assert!(!is_runtime_palette_shortcut_key(
            key.as_ref(),
            ModifiersState::SHIFT
        ));
    }

    #[test]
    fn palette_dispatch_updates_render_mode_via_runtime_path() {
        let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
        let mut settings = SettingsService::default();

        let message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "mode cpu")
            .expect("dispatch mode cpu");
        assert!(message.contains("mode=cpu"));
        assert_eq!(settings.state().mode, RenderMode::Cpu);
        assert_eq!(ui_runtime.render_mode(), RenderMode::Cpu);
    }

    #[test]
    fn palette_dispatch_toggles_diagnostics_state() {
        let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
        let mut settings = SettingsService::default();

        let on_message =
            dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug on")
                .expect("dispatch debug on");
        assert!(on_message.contains("diagnostics=on"));
        assert!(settings.state().debug_mode);

        let off_message =
            dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug off")
                .expect("dispatch debug off");
        assert!(off_message.contains("diagnostics=off"));
        assert!(!settings.state().debug_mode);
    }

    fn test_ui_runtime(mode: RenderMode) -> UiRuntime {
        UiRuntime::bootstrap(UiBootstrapConfig::single_window(mode, 60_000))
            .expect("ui runtime bootstrap")
    }

    #[test]
    fn injected_gpu_failure_falls_back_without_forcing_exit_in_auto_mode() {
        let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

        let first = dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 10)
            .expect("first gpu failure");
        assert_eq!(
            first,
            GpuFailureHandling::RetryScheduled {
                failure_streak: 1,
                retry_budget_remaining: 1
            }
        );
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

        let second = dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SubmitError, 20)
            .expect("second gpu failure");
        assert_eq!(
            second,
            GpuFailureHandling::RetryScheduled {
                failure_streak: 2,
                retry_budget_remaining: 0
            }
        );
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

        let third =
            dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SwapchainOutOfDate, 30)
                .expect("third gpu failure");
        assert_eq!(
            third,
            GpuFailureHandling::FallbackToCpu {
                transition_sequence: 1
            }
        );
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Cpu);
    }

    #[test]
    fn forced_gpu_mode_reports_explicit_gpu_failure() {
        let mut ui_runtime = test_ui_runtime(RenderMode::Gpu);
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

        let decision =
            dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 7)
                .expect("gpu failure decision");
        assert_eq!(decision, GpuFailureHandling::FatalForcedGpu);
        assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);
    }

    #[test]
    fn monitor_timing_sampling_uses_window_control_contract() {
        let control = StubWindowControl {
            scenario: StubWindowControlScenario::Timing(Some(144_000)),
        };
        assert_eq!(
            sample_monitor_refresh_rate_millihz(Some(&control)),
            Some(144_000)
        );
    }

    #[test]
    fn monitor_timing_sampling_returns_none_when_contract_fails() {
        let control = StubWindowControl {
            scenario: StubWindowControlScenario::Error,
        };
        assert_eq!(sample_monitor_refresh_rate_millihz(Some(&control)), None);
    }

    #[test]
    fn monitor_affecting_events_emit_expected_cadence_resync_commands() {
        let sampled_refresh = Some(144_000);

        assert_eq!(
            cadence_resync_command_for_monitor_event(
                MonitorAffectingWindowEvent::Moved,
                sampled_refresh,
            ),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 144_000,
            }
        );
        assert_eq!(
            cadence_resync_command_for_monitor_event(
                MonitorAffectingWindowEvent::Resized,
                sampled_refresh,
            ),
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz: 144_000,
            }
        );
        assert_eq!(
            cadence_resync_command_for_monitor_event(
                MonitorAffectingWindowEvent::ScaleFactorChanged,
                sampled_refresh,
            ),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 144_000,
            }
        );
    }

    #[test]
    fn cadence_resync_commands_use_zero_when_monitor_timing_is_unavailable() {
        assert_eq!(
            cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Moved, None),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 0,
            }
        );
        assert_eq!(
            cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Resized, None),
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz: 0,
            }
        );
        assert_eq!(
            cadence_resync_command_for_monitor_event(
                MonitorAffectingWindowEvent::ScaleFactorChanged,
                None,
            ),
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 0,
            }
        );
    }

    #[test]
    fn gpu_auto_fallback_emits_correlated_diagnostics_and_runtime_notice() {
        let diagnostics = DiagnosticsSink::default();
        let (event, notice) = emit_gpu_auto_fallback_observability(
            &diagnostics,
            7,
            3,
            41,
            GpuFailureKind::SwapchainOutOfDate,
            2_500,
        );

        assert_eq!(event.kind, EventKind::RenderModeTransition);
        let correlation = event
            .correlation_id
            .as_ref()
            .expect("fallback diagnostics must include correlation");
        assert!(event.message.contains("transition-seq=7"));
        assert!(event.message.contains("failure-seq=3"));
        assert!(event.message.contains("render-attempt-seq=41"));
        assert!(event.message.contains("observed-ms=2500"));
        assert!(notice.contains("transition-seq=7"));
        assert!(notice.contains("failure-seq=3"));
        assert!(notice.contains("render-attempt-seq=41"));
        assert!(notice.contains("observed-ms=2500"));
        assert!(notice.contains(correlation.as_str()));
    }

    #[test]
    fn gui_write_boundary_policy_stays_recoverable_with_budget() {
        let mut session_policy = SessionController::with_recoverable_budget(2);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let decision =
            classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
                .expect("recoverable write boundary should classify");

        assert_eq!(
            decision,
            PtyBoundaryPolicyDecision::Continue {
                attempt: 1,
                remaining_budget: 1,
            }
        );
    }

    #[test]
    fn gui_write_boundary_policy_escalates_after_budget_exhaustion() {
        let mut session_policy = SessionController::with_recoverable_budget(1);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let first = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
            .expect("first write boundary should stay recoverable");
        assert_eq!(
            first,
            PtyBoundaryPolicyDecision::Continue {
                attempt: 1,
                remaining_budget: 0,
            }
        );

        let second = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
            .expect("second write boundary should escalate after budget exhaustion");
        assert_eq!(
            second,
            PtyBoundaryPolicyDecision::Fatal {
                reason: FatalBoundaryReason::RecoverableBudgetExhausted,
            }
        );
    }

    #[test]
    fn color_to_u32_default_uses_default_color() {
        let default_fg = color_to_u32(Color::Default, DEFAULT_FG);
        assert_eq!(default_fg, DEFAULT_FG_U32);
    }

    #[test]
    fn color_to_u32_indexed_looks_up_palette() {
        let red = color_to_u32(Color::Indexed(1), DEFAULT_FG);
        assert_eq!(red, ANSI_PALETTE[1]);
    }

    #[test]
    fn color_to_u32_rgb_constructs_correctly() {
        let c = color_to_u32(Color::Rgb(0xFF, 0x80, 0x00), DEFAULT_FG);
        assert_eq!(c, 0x00FF_8000);
    }

    #[test]
    fn resolve_cell_colors_inverse_swaps_fg_bg() {
        let attrs = Attrs {
            fg: Color::Indexed(1),
            bg: Color::Indexed(2),
            inverse: true,
            ..Attrs::default()
        };
        let (fg, bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, ANSI_PALETTE[2]);
        assert_eq!(bg, ANSI_PALETTE[1]);
    }

    #[test]
    fn resolve_cell_colors_dim_halves_fg() {
        let attrs = Attrs {
            fg: Color::Rgb(200, 100, 50),
            dim: true,
            ..Attrs::default()
        };
        let (fg, _bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, rldyourterm_render_cpu::rgb_to_u32(100, 50, 25));
    }

    #[test]
    fn encode_named_keys_without_modifiers() {
        use winit::keyboard::NamedKey;

        let mods = ModifiersState::empty();

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::F1), mods),
            Some(b"\x1bOP".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::F5), mods),
            Some(b"\x1b[15~".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::PageUp), mods),
            Some(b"\x1b[5~".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::Insert), mods),
            Some(b"\x1b[2~".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), mods),
            Some(b"\x1b[A".to_vec()),
        );
    }

    #[test]
    fn encode_ctrl_arrow_produces_modified_csi() {
        use winit::keyboard::NamedKey;

        let ctrl = ModifiersState::CONTROL;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), ctrl),
            Some(b"\x1b[1;5D".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowRight), ctrl),
            Some(b"\x1b[1;5C".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), ctrl),
            Some(b"\x1b[1;5A".to_vec()),
        );
        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowDown), ctrl),
            Some(b"\x1b[1;5B".to_vec()),
        );
    }

    #[test]
    fn encode_shift_arrow_produces_modified_csi() {
        use winit::keyboard::NamedKey;

        let shift = ModifiersState::SHIFT;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), shift),
            Some(b"\x1b[1;2A".to_vec()),
        );
    }

    #[test]
    fn encode_alt_arrow_produces_modified_csi() {
        use winit::keyboard::NamedKey;

        let alt = ModifiersState::ALT;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), alt),
            Some(b"\x1b[1;3D".to_vec()),
        );
    }

    #[test]
    fn encode_ctrl_shift_f1_produces_modified_csi() {
        use winit::keyboard::NamedKey;

        let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::F1), ctrl_shift),
            Some(b"\x1b[1;6P".to_vec()),
        );
    }

    #[test]
    fn encode_shift_tab_produces_reverse_tab() {
        use winit::keyboard::NamedKey;

        let shift = ModifiersState::SHIFT;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::Tab), shift),
            Some(b"\x1b[Z".to_vec()),
        );
    }

    #[test]
    fn encode_alt_backspace_produces_esc_del() {
        use winit::keyboard::NamedKey;

        let alt = ModifiersState::ALT;

        assert_eq!(
            encode_winit_key_event(&Key::Named(NamedKey::Backspace), alt),
            Some(b"\x1b\x7f".to_vec()),
        );
    }
}
