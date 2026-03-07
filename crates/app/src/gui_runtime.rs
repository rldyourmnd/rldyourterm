use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use rldyourterm_diagnostics::{CorrelationId, DiagnosticsSink, Event, EventKind};
use rldyourterm_font::GlyphCache;
use rldyourterm_foundation::api::clipboard::ClipboardAdapter;
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation::api::window::{
    MonitorTiming, WindowConfig as FoundationWindowConfig, WindowControl,
    WindowEvent as FoundationWindowEvent, WindowEventSink as FoundationWindowEventSink,
    WindowFactory,
};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_foundation_platform::window::PlatformWindowFactory;
use rldyourterm_render_gpu::GpuRenderer;
use rldyourterm_services::CoreEvent;
use rldyourterm_services::MAX_FEED_BYTES_PER_CALL;
use rldyourterm_services::TerminalState;
use rldyourterm_services::grid::{self, CELL_HEIGHT, CELL_WIDTH};
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::{SessionBoundary, SessionController, SessionState};
use rldyourterm_settings::{SettingsCommand, SettingsPaletteApplyOutcome, SettingsService};
use rldyourterm_ui::{UiBootstrapConfig, UiCommandOutcome, UiRuntime, UiRuntimeCommand};
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};
use tracing::{debug, info, trace, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, Ime, KeyEvent as WinitKeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
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
const DEFERRED_GPU_INIT_RETRY_BUDGET: u8 = 3;
const DEFERRED_GPU_INIT_MIN_BACKOFF: Duration = Duration::from_millis(50);
const DEFERRED_GPU_INIT_MAX_BACKOFF: Duration = Duration::from_millis(400);
use rldyourterm_ui::DEFAULT_SCROLLBACK_CAP;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const CHILD_EXIT_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(5);
const CHILD_EXIT_DRAIN_MAX_WAIT: Duration = Duration::from_millis(750);
const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const DEFAULT_BG_U32: u32 = rgb_to_u32(DEFAULT_BG.0, DEFAULT_BG.1, DEFAULT_BG.2);
#[cfg(test)]
const DEFAULT_FG_U32: u32 = rgb_to_u32(DEFAULT_FG.0, DEFAULT_FG.1, DEFAULT_FG.2);
const CLIPBOARD_PASTE_CAP_BYTES: usize = 64 * 1024;
const PTY_OUTPUT_QUEUE_CAPACITY: usize = 256;
const OUTPUT_BATCH_INITIAL_CAPACITY: usize = 64 * 1024;
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
const RUNTIME_PALETTE_HELP_LINE: &str =
    "[palette] 1:mode cpu 2:mode gpu 3:mode auto d:diagnostics toggle i:info Esc:close";
const RUNTIME_PALETTE_CLOSED_LINE: &str = "[palette] closed";

#[derive(Debug)]
enum GuiEvent {
    OutputReady,
    Exited(i32),
    PtyFailure {
        boundary: SessionBoundary,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputQueueSnapshot {
    queued_bytes: usize,
    queued_chunks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputDrainPressure {
    Normal,
    Elevated,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutputDrainBudget {
    pressure: OutputDrainPressure,
    max_bytes_per_tick: usize,
    max_latency: Duration,
}

#[derive(Debug, Default)]
struct OutputQueueBackpressure {
    queued_bytes: AtomicUsize,
    queued_chunks: AtomicUsize,
}

impl OutputQueueBackpressure {
    fn note_enqueue(&self, bytes: usize) {
        self.queued_bytes.fetch_add(bytes, Ordering::AcqRel);
        self.queued_chunks.fetch_add(1, Ordering::AcqRel);
    }

    fn note_dequeue(&self, bytes: usize) {
        let _ = self
            .queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(bytes))
            });
        let _ = self
            .queued_chunks
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            });
    }

    fn snapshot(&self) -> OutputQueueSnapshot {
        OutputQueueSnapshot {
            queued_bytes: self.queued_bytes.load(Ordering::Acquire),
            queued_chunks: self.queued_chunks.load(Ordering::Acquire),
        }
    }
}

type SpawnedPty = (Arc<dyn PtyIo>, Box<dyn Write + Send>, Box<dyn Read + Send>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GpuFailureHandling {
    RetryScheduled {
        failure_streak: u8,
        retry_budget_remaining: u8,
    },
    FallbackToCpu {
        transition_sequence: u64,
    },
    FatalForcedGpu,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePaletteAction {
    ApplyCommand(&'static str),
    ShowInfo,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitorAffectingWindowEvent {
    Moved,
    Resized,
    ScaleFactorChanged,
}

use crate::shared::{
    PtyBoundaryPolicyDecision, ai_cli_spawn_env_overrides, classify_pty_boundary_failure,
    csi_modified, encode_ctrl_letter, fatal_boundary_reason_token, fkey_ss3_modified,
    is_disconnect_error, on_off_token, render_mode_token, session_boundary_token, tilde_modified,
    write_all_and_flush, xterm_modifier_param,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PtyBoundaryLoopAction {
    Continue,
    ExitLoop,
}

fn dispatch_gpu_failure_command(
    ui_runtime: &mut UiRuntime,
    failure_kind: GpuFailureKind,
    observed_at_millis: u64,
) -> Result<GpuFailureHandling> {
    let receipt = ui_runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: failure_kind,
            observed_at_millis,
        })
        .context("failed to dispatch UiRuntimeCommand::GpuFailure")?;

    match receipt.outcome {
        UiCommandOutcome::GpuRetryScheduled {
            failure_streak,
            retry_budget_remaining,
            ..
        } => Ok(GpuFailureHandling::RetryScheduled {
            failure_streak,
            retry_budget_remaining,
        }),
        UiCommandOutcome::RenderModeTransition(transition) => {
            Ok(GpuFailureHandling::FallbackToCpu {
                transition_sequence: transition.sequence,
            })
        }
        UiCommandOutcome::Noop
            if ui_runtime.render_mode() == RenderMode::Gpu
                && ui_runtime.active_render_path() == ActiveRenderPath::Gpu =>
        {
            Ok(GpuFailureHandling::FatalForcedGpu)
        }
        UiCommandOutcome::Noop => Ok(GpuFailureHandling::Ignored),
        outcome @ (UiCommandOutcome::SessionTransition(_)
        | UiCommandOutcome::CadenceResynced { .. }
        | UiCommandOutcome::SingleWindowConfirmed { .. }) => Err(anyhow!(
            "unexpected UI outcome for GPU failure command: {outcome:?}"
        )),
    }
}

pub fn run_interactive_gui_pty(
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
    let output_event_pending = Arc::new(AtomicBool::new(false));
    let output_backpressure = Arc::new(OutputQueueBackpressure::default());
    let reader_pump = spawn_reader_pump(
        reader,
        proxy.clone(),
        output_tx,
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
        output_rx,
        output_event_pending,
        reader_pump,
        wait_pump,
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

fn spawn_reader_pump(
    mut reader: Box<dyn Read + Send>,
    proxy: EventLoopProxy<GuiEvent>,
    output_tx: SyncSender<Vec<u8>>,
    output_event_pending: Arc<AtomicBool>,
    output_backpressure: Arc<OutputQueueBackpressure>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 65536];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    output_backpressure.note_enqueue(read_bytes);
                    if output_tx.send(buffer[..read_bytes].to_vec()).is_err() {
                        output_backpressure.note_dequeue(read_bytes);
                        break;
                    }
                    if !output_event_pending.swap(true, Ordering::AcqRel)
                        && proxy.send_event(GuiEvent::OutputReady).is_err()
                    {
                        break;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = proxy.send_event(GuiEvent::PtyFailure {
                        boundary: SessionBoundary::PtyRead,
                        message: format!("PTY reader pump failed: {error}"),
                    });
                    break;
                }
            }
        }
    })
}

fn spawn_wait_pump(pty: Arc<dyn PtyIo>, proxy: EventLoopProxy<GuiEvent>) -> JoinHandle<()> {
    thread::spawn(move || match pty.wait() {
        Ok(code) => {
            let _ = proxy.send_event(GuiEvent::Exited(code));
        }
        Err(error) => {
            let _ = proxy.send_event(GuiEvent::PtyFailure {
                boundary: SessionBoundary::PtyWait,
                message: format!("PTY wait failed: {error}"),
            });
        }
    })
}

struct GuiRuntimeApp {
    event_proxy: EventLoopProxy<GuiEvent>,
    pty: Arc<dyn PtyIo>,
    writer: Box<dyn Write + Send>,
    output_rx: Receiver<Vec<u8>>,
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
    render_attempt_sequence: u64,
    gpu_failure_sequence: u64,

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
    gpu_init_pending: bool,
    deferred_gpu_init_failures: u8,
    next_gpu_init_retry_at: Option<Instant>,
    last_rendered_cursor_row: Option<u16>,
    last_softbuffer_size: Option<PhysicalSize<u32>>,
    last_viewport_cols: u16,
    last_viewport_rows: u16,
    output_batch: Vec<u8>,
    feed_events_scratch: Vec<CoreEvent>,
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

#[derive(Debug, Default)]
struct NoopFoundationWindowEventSink;

impl FoundationWindowEventSink for NoopFoundationWindowEventSink {
    fn on_event(&self, _event: FoundationWindowEvent) {}
}

impl GuiRuntimeApp {
    fn new(
        pty: Arc<dyn PtyIo>,
        writer: Box<dyn Write + Send>,
        output_rx: Receiver<Vec<u8>>,
        output_event_pending: Arc<AtomicBool>,
        reader_pump: JoinHandle<()>,
        wait_pump: JoinHandle<()>,
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
            render_attempt_sequence: 0,
            gpu_failure_sequence: 0,
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
            gpu_init_pending: initial_mode != RenderMode::Cpu,
            deferred_gpu_init_failures: 0,
            next_gpu_init_retry_at: None,
            last_rendered_cursor_row: None,
            last_softbuffer_size: None,
            last_viewport_cols: DEFAULT_COLS,
            last_viewport_rows: DEFAULT_ROWS,
            output_batch: Vec::with_capacity(OUTPUT_BATCH_INITIAL_CAPACITY),
            feed_events_scratch: Vec::with_capacity(FEED_EVENTS_SCRATCH_INITIAL_CAPACITY),
            dirty_rows_scratch: Vec::with_capacity(DIRTY_ROWS_SCRATCH_INITIAL_CAPACITY),
            exit_code: None,
            fatal_error: None,
        })
    }

    fn bootstrap_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        // `mut` needed on Linux/FreeBSD for platform-specific window attributes,
        // but triggers unused_mut warning on macOS where those blocks don't compile.
        #[allow(unused_mut)]
        let mut attributes = Window::default_attributes()
            .with_title("rldyourterm")
            .with_inner_size(LogicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT))
            .with_visible(true)
            .with_active(true)
            .with_window_icon(load_app_icon());

        // Set Wayland app_id and X11 WM_CLASS so the compositor identifies the window.
        // Both traits define `with_name` — fully-qualified calls avoid method ambiguity.
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attributes =
                WindowAttributesExtWayland::with_name(attributes, "rldyourterm", "rldyourterm");
        }
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        {
            use winit::platform::x11::WindowAttributesExtX11;
            attributes =
                WindowAttributesExtX11::with_name(attributes, "rldyourterm", "rldyourterm");
        }

        // Activation token for Wayland/X11 focus handoff
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if let Some(token) = event_loop.read_token_from_env() {
            attributes = attributes.with_activation_token(token);
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("failed to create GUI window")?,
        );
        // IME is intentionally disabled for terminal emulators. On Wayland,
        // enabling IME activates zwp_text_input_v3 alongside wl_keyboard (xkb),
        // causing every keypress to be delivered twice (KeyboardInput.text + Ime::Commit).
        // Terminal emulators (Alacritty, foot) rely solely on wl_keyboard for input.
        window.set_ime_allowed(false);

        let window_control = PlatformWindowFactory::from_winit_window(window.clone())
            .init(
                FoundationWindowConfig {
                    title: "rldyourterm".to_owned(),
                    width: DEFAULT_GUI_WIDTH,
                    height: DEFAULT_GUI_HEIGHT,
                    min_width: 1,
                    min_height: 1,
                    high_dpi: true,
                },
                Box::new(NoopFoundationWindowEventSink),
            )
            .context("failed to initialize foundation window control from winit window")?;

        // GPU initialization is deferred to about_to_wait() so the event loop
        // can process time-sensitive terminal queries (e.g. fish DA1) before the
        // blocking GPU init (~1-2s). Always start with softbuffer for immediate
        // CPU rendering; the deferred path will drop it before GPU init (Wayland
        // surface exclusivity).
        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;
        self._context = Some(context);
        self.surface = Some(surface);
        debug!(
            gpu_deferred = self.gpu_init_pending,
            "bootstrap: softbuffer context created, GPU init deferred to event loop"
        );

        self.window_size = cap_framebuffer_extent(window.inner_size());
        self.window_id = Some(window.id());
        self.window_control = Some(window_control);
        self.window = Some(window);

        debug!("bootstrap: updating viewport geometry");
        self.update_viewport_geometry(event_loop);

        // Draw the first frame synchronously before returning.
        // On Wayland, the compositor will not map (show) the window until content
        // is committed to its surface. Deferring to RedrawRequested is unreliable
        // because ControlFlow::Wait may not deliver it before entering sleep.
        debug!("bootstrap: drawing initial frame");
        self.draw_frame()
            .context("failed to draw initial frame during bootstrap")?;

        debug!("bootstrap: applying visibility handshake");
        self.apply_post_draw_visibility_handshake();
        debug!("bootstrap: complete");
        Ok(())
    }

    /// Release all window-bound graphics resources while the Wayland/X11
    /// connection is still alive. On Wayland the compositor only receives the
    /// surface-destroy protocol message when the `Arc<Window>` refcount reaches
    /// zero.  If resources are dropped after `run_app()` returns, the connection
    /// is already closed and the compositor never learns the window is gone -
    /// leaving a ghost entry in the dock/taskbar.
    ///
    fn dispatch_terminal_responses(
        &mut self,
        events: &[CoreEvent],
        event_loop: &ActiveEventLoop,
    ) -> bool {
        let mut emitted_terminal_response = false;
        let mut saw_write_error = false;
        for event in events {
            if let CoreEvent::TerminalResponse { data } = event {
                emitted_terminal_response = true;
                trace!(bytes = data.len(), "sending terminal response to PTY");
                if let Err(error) = write_all_and_flush(&mut *self.writer, data) {
                    saw_write_error = true;
                    match self.handle_pty_io_error(
                        SessionBoundary::PtyWrite,
                        error,
                        "failed to write terminal response to PTY",
                    ) {
                        Ok(PtyBoundaryLoopAction::Continue) => {}
                        Ok(PtyBoundaryLoopAction::ExitLoop) => {
                            event_loop.exit();
                            return false;
                        }
                        Err(policy_error) => {
                            self.fatal_error = Some(policy_error);
                            event_loop.exit();
                            return false;
                        }
                    }
                }
            }
        }
        if emitted_terminal_response
            && !saw_write_error
            && let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite)
        {
            self.fatal_error = Some(error);
            event_loop.exit();
            return false;
        }
        true
    }

    /// Drop order matters: surface before context, context before window,
    /// GPU backend (which holds `wgpu::Surface<'static>` -> `Arc<Window>`)
    /// before the window itself.
    fn release_window_resources(&mut self) {
        debug!(
            window_exists = self.window.is_some(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            has_surface = self.surface.is_some(),
            "releasing window resources"
        );
        // 1. Drop softbuffer surface (holds Arc<Window>)
        self.surface = None;
        self.last_softbuffer_size = None;
        // 2. Drop softbuffer context (holds Arc<Window>)
        self._context = None;
        // 3. Drop GPU backend which holds wgpu::Surface<'static> -> Arc<Window>
        self.gpu_renderer = GpuRenderer::default();
        // 4. Drop foundation control (holds Arc<Window>)
        self.window_control = None;
        // 5. Drop the window itself (final Arc<Window> reference)
        self.window_id = None;
        self.window = None;
        self.redraw_in_flight = false;
        self.sync_deferred_gpu_init_state();
        self.next_gpu_init_retry_at = None;
        debug!("window resources released");
    }

    fn persist_gpu_pipeline_cache(&mut self) {
        if let Some(cache_dir) = &self.gpu_cache_dir {
            self.gpu_renderer.save_pipeline_cache(cache_dir);
        }
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

    fn ensure_softbuffer_surface(&mut self) -> Result<()> {
        if self.surface.is_some() {
            return Ok(());
        }
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| anyhow!("no window for softbuffer initialization"))?;
        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;
        self._context = Some(context);
        self.surface = Some(surface);
        info!("lazily initialized softbuffer surface for CPU fallback");
        Ok(())
    }

    fn try_deferred_gpu_init(&mut self, event_loop: &ActiveEventLoop) {
        if self.ui_runtime.active_render_path() == ActiveRenderPath::Cpu {
            self.gpu_init_pending = false;
            self.next_gpu_init_retry_at = None;
            return;
        }
        if let Some(retry_at) = self.next_gpu_init_retry_at
            && Instant::now() < retry_at
        {
            return;
        }
        self.next_gpu_init_retry_at = None;

        let Some(window) = self.window.clone() else {
            return;
        };
        let size = cap_framebuffer_extent(window.inner_size());
        let w = size.width;
        let h = size.height;

        debug!("deferred GPU init: dropping softbuffer for Wayland surface exclusivity");
        self.surface = None;
        self.last_softbuffer_size = None;
        self._context = None;

        let attempt = self.deferred_gpu_init_failures.saturating_add(1);
        debug!("deferred GPU init: attempting GPU initialization");
        match self
            .gpu_renderer
            .initialize(window, w, h, self.gpu_cache_dir.as_deref())
        {
            Ok(()) => {
                self.gpu_init_pending = false;
                self.deferred_gpu_init_failures = 0;
                info!("GPU backend initialized successfully");
                self.terminal.grid.mark_all_dirty();
                self.queue_redraw();
            }
            Err(e) => {
                self.deferred_gpu_init_failures = attempt;
                self.gpu_failure_sequence = self.gpu_failure_sequence.saturating_add(1);
                let gpu_failure_sequence = self.gpu_failure_sequence;
                let remaining = DEFERRED_GPU_INIT_RETRY_BUDGET.saturating_sub(attempt);
                warn!(
                    error = ?e,
                    attempt,
                    retry_budget = DEFERRED_GPU_INIT_RETRY_BUDGET,
                    retries_remaining = remaining,
                    mode = ?self.ui_runtime.render_mode(),
                    active_path = ?self.ui_runtime.active_render_path(),
                    "deferred GPU init failed"
                );

                if attempt < DEFERRED_GPU_INIT_RETRY_BUDGET {
                    self.gpu_init_pending = true;
                    let backoff = deferred_gpu_init_backoff(attempt);
                    self.next_gpu_init_retry_at = Some(Instant::now() + backoff);
                    self.queue_redraw();
                    return;
                }

                self.gpu_init_pending = false;
                self.next_gpu_init_retry_at = None;
                let observed_at_millis = self
                    .started_at
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                let failure_kind = GpuFailureKind::DeviceLost;

                match dispatch_gpu_failure_command(
                    &mut self.ui_runtime,
                    failure_kind,
                    observed_at_millis,
                ) {
                    Ok(GpuFailureHandling::FallbackToCpu {
                        transition_sequence,
                    }) => {
                        self.gpu_renderer.release_backend();
                        self.sync_deferred_gpu_init_state();
                        self.terminal.grid.mark_all_dirty();
                        let (diagnostics_event, fallback_notice) =
                            emit_gpu_auto_fallback_observability(
                                &self.diagnostics,
                                transition_sequence,
                                gpu_failure_sequence,
                                self.render_attempt_sequence,
                                failure_kind,
                                observed_at_millis,
                            );
                        warn!(
                            transition_sequence,
                            diagnostics_event_id = %diagnostics_event.event_id,
                            diagnostics_correlation = ?diagnostics_event.correlation_id,
                            "deferred GPU init exhausted retry budget; applying deterministic CPU fallback"
                        );
                        self.emit_runtime_notice(&fallback_notice);
                        self.queue_redraw();

                        if self.session_policy.state() == SessionState::Degraded
                            && let Err(error) = self.session_policy.mark_running()
                        {
                            warn!(%error, "session mark_running after deferred GPU fallback failed");
                        }
                    }
                    Ok(GpuFailureHandling::RetryScheduled { .. } | GpuFailureHandling::Ignored) => {
                        self.queue_redraw();
                    }
                    Ok(GpuFailureHandling::FatalForcedGpu) => {
                        let message = format!(
                            "forced GPU mode initialization failed after {} attempts: {:?}",
                            attempt, e
                        );
                        self.diagnostics
                            .emit_kind(EventKind::SessionError, message.clone());
                        self.fatal_error = Some(anyhow!(message));
                        event_loop.exit();
                    }
                    Err(dispatch_error) => {
                        self.fatal_error = Some(dispatch_error);
                        event_loop.exit();
                    }
                }
            }
        }
    }

    fn apply_post_draw_visibility_handshake(&self) {
        if let Some(window) = self.window.as_ref() {
            window.set_visible(true);
            window.focus_window();
            let _ = self.request_window_redraw();
            info!("applied visibility handshake after first frame commit");
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }

    fn sync_deferred_gpu_init_state(&mut self) {
        let target_path = self.ui_runtime.active_render_path();
        if target_path == ActiveRenderPath::Cpu {
            if self.gpu_renderer.is_initialized() {
                self.gpu_renderer.release_backend();
            }
            self.gpu_init_pending = false;
            self.deferred_gpu_init_failures = 0;
            self.next_gpu_init_retry_at = None;
            return;
        }

        self.gpu_init_pending = !self.gpu_renderer.is_initialized();
        if !self.gpu_init_pending {
            self.next_gpu_init_retry_at = None;
            self.deferred_gpu_init_failures = 0;
        }
    }

    fn emit_runtime_notice(&mut self, message: &str) {
        let mut line = String::from("\r\n");
        line.push_str(message);
        line.push_str("\r\n");
        let mut events = std::mem::take(&mut self.feed_events_scratch);
        self.terminal.feed_into(line.as_bytes(), &mut events);
        self.feed_events_scratch = events;
        self.queue_redraw();
    }

    fn toggle_palette(&mut self) {
        self.palette_open = !self.palette_open;
        if self.palette_open {
            self.emit_runtime_notice(RUNTIME_PALETTE_HELP_LINE);
        } else {
            self.emit_runtime_notice(RUNTIME_PALETTE_CLOSED_LINE);
        }
    }

    fn handle_palette_action(&mut self, event: &WinitKeyEvent) -> Result<bool> {
        if !self.palette_open {
            return Ok(false);
        }

        let Some(action) = runtime_palette_action_for_winit_key(
            event.logical_key.as_ref(),
            self.settings.state().debug_mode,
        ) else {
            return Ok(true);
        };

        match action {
            RuntimePaletteAction::Close => {
                self.palette_open = false;
                self.emit_runtime_notice(RUNTIME_PALETTE_CLOSED_LINE);
            }
            RuntimePaletteAction::ShowInfo => {
                let info_line = runtime_palette_info_line(&self.ui_runtime, &self.settings);
                self.emit_runtime_notice(&info_line);
            }
            RuntimePaletteAction::ApplyCommand(command) => {
                let result_line = dispatch_runtime_palette_command(
                    &mut self.ui_runtime,
                    &mut self.settings,
                    command,
                )?;
                self.sync_deferred_gpu_init_state();
                self.palette_open = false;
                self.emit_runtime_notice(&result_line);
            }
        }

        Ok(true)
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
        let mut events = std::mem::take(&mut self.feed_events_scratch);
        for chunk in terminal_feed_chunks(data) {
            self.terminal.feed_into(chunk, &mut events);
            if !self.dispatch_terminal_responses(&events, event_loop) {
                self.feed_events_scratch = events;
                return false;
            }
        }
        self.feed_events_scratch = events;
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
                    self.output_batch = batch;
                    return;
                }
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
                        self.output_batch = batch;
                        return;
                    }
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

    fn update_viewport_geometry(&mut self, event_loop: &ActiveEventLoop) {
        let raw_cols = ((self.window_size.width as usize) / CELL_WIDTH).max(1);
        let raw_rows = ((self.window_size.height as usize) / CELL_HEIGHT).max(1);
        let (cols, rows) = cap_terminal_geometry(raw_cols, raw_rows);
        if cols as usize != raw_cols || rows as usize != raw_rows {
            warn!(
                raw_cols,
                raw_rows,
                cols,
                rows,
                max_cols = MAX_VIEWPORT_COLS,
                max_rows = MAX_VIEWPORT_ROWS,
                max_cells = MAX_VIEWPORT_CELLS,
                "viewport geometry exceeded runtime safety limits; dimensions were clamped"
            );
        }

        // Skip PTY resize when terminal dimensions are unchanged to avoid
        // redundant SIGWINCH delivery during Wayland startup event bursts.
        if cols == self.last_viewport_cols && rows == self.last_viewport_rows {
            trace!(cols, rows, "viewport: skipped (dimensions unchanged)");
            return;
        }

        debug!(
            cols,
            rows,
            width = self.window_size.width,
            height = self.window_size.height,
            "viewport: resizing"
        );

        self.terminal.resize(cols, rows);
        self.last_viewport_cols = cols;
        self.last_viewport_rows = rows;

        if let Err(error) = self.pty.resize(PtySize {
            cols,
            rows,
            pixel_width: self.window_size.width.min(u16::MAX as u32) as u16,
            pixel_height: self.window_size.height.min(u16::MAX as u32) as u16,
        }) {
            let detail = format!("failed to resize PTY to viewport: {error}");
            match self.handle_pty_boundary_failure(SessionBoundary::PtyResize, &detail) {
                Ok(PtyBoundaryLoopAction::Continue) => {}
                Ok(PtyBoundaryLoopAction::ExitLoop) => {
                    event_loop.exit();
                }
                Err(policy_error) => {
                    self.fatal_error = Some(policy_error);
                    event_loop.exit();
                }
            }
            return;
        }

        debug!("viewport: pty resize complete");

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyResize) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn handle_close_requested(&mut self, event_loop: &ActiveEventLoop) {
        debug!("window close requested by user or compositor");
        self.emit_close_intent();
        self.exit_code.get_or_insert(0);
        event_loop.exit();
    }

    fn handle_monitor_affecting_event(&mut self, monitor_event: MonitorAffectingWindowEvent) {
        let sampled_refresh_rate_millihz =
            sample_monitor_refresh_rate_millihz(self.window_control.as_deref());
        let command =
            cadence_resync_command_for_monitor_event(monitor_event, sampled_refresh_rate_millihz);

        match self.ui_runtime.handle_command(command) {
            Ok(receipt) => match receipt.outcome {
                UiCommandOutcome::CadenceResynced {
                    previous_refresh_rate_millihz,
                    current_refresh_rate_millihz,
                    generation,
                    monitor_transfer,
                    ..
                } => {
                    info!(
                        monitor_event = monitor_affecting_event_token(monitor_event),
                        sampled_refresh_rate_millihz = sampled_refresh_rate_millihz.unwrap_or(0),
                        previous_refresh_rate_millihz = ?previous_refresh_rate_millihz,
                        current_refresh_rate_millihz = ?current_refresh_rate_millihz,
                        generation,
                        monitor_transfer,
                        "GUI runtime re-synced cadence after monitor-affecting event"
                    );
                }
                UiCommandOutcome::Noop => {}
                other => {
                    warn!(
                        monitor_event = monitor_affecting_event_token(monitor_event),
                        sampled_refresh_rate_millihz = sampled_refresh_rate_millihz.unwrap_or(0),
                        outcome = ?other,
                        "unexpected UI outcome while processing monitor-affecting cadence event"
                    );
                }
            },
            Err(error) => {
                warn!(
                    monitor_event = monitor_affecting_event_token(monitor_event),
                    sampled_refresh_rate_millihz = sampled_refresh_rate_millihz.unwrap_or(0),
                    error = %error,
                    "failed to dispatch cadence re-sync command after monitor-affecting event"
                );
                self.emit_runtime_notice(&format!(
                    "[runtime] cadence-resync dispatch failed event={} sampled-refresh-millihz={} detail={error}",
                    monitor_affecting_event_token(monitor_event),
                    sampled_refresh_rate_millihz.unwrap_or(0),
                ));
            }
        }
    }

    fn handle_keyboard_input(&mut self, event: &WinitKeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }

        if is_local_shutdown_key(event, self.modifiers) {
            self.emit_close_intent();
            self.exit_code.get_or_insert(0);
            event_loop.exit();
            return;
        }

        if is_runtime_palette_shortcut(event, self.modifiers) {
            self.toggle_palette();
            return;
        }

        match self.handle_palette_action(event) {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                self.fatal_error = Some(error);
                event_loop.exit();
                return;
            }
        }

        if is_paste_shortcut(&event.logical_key, self.modifiers) {
            self.handle_clipboard_paste(event_loop);
            return;
        }

        // Determine bytes to send to PTY. Priority:
        // 1. Alt+text: ESC prefix + text (terminal alt-key convention)
        // 2. Plain text (no Ctrl/Alt/Super): event.text directly
        // 3. Named keys + Ctrl combos: encode_winit_key_event for escape sequences
        let bytes = if self.modifiers.alt_key()
            && !self.modifiers.control_key()
            && !self.modifiers.super_key()
        {
            event.text.as_ref().filter(|t| !t.is_empty()).map(|text| {
                let mut b = vec![0x1b];
                b.extend_from_slice(text.as_bytes());
                b
            })
        } else if !self.modifiers.control_key()
            && !self.modifiers.alt_key()
            && !self.modifiers.super_key()
        {
            event
                .text
                .as_ref()
                .filter(|t| !t.is_empty())
                .map(|text| text.as_bytes().to_vec())
        } else {
            None
        }
        .or_else(|| encode_winit_key_event(&event.logical_key, self.modifiers));

        if let Some(ref bytes) = bytes {
            trace!(key = ?event.logical_key, len = bytes.len(), "keyboard input to PTY");
            if let Err(error) = write_all_and_flush(&mut *self.writer, bytes) {
                match self.handle_pty_io_error(
                    SessionBoundary::PtyWrite,
                    error,
                    "failed to write keyboard input to PTY",
                ) {
                    Ok(PtyBoundaryLoopAction::Continue) => {}
                    Ok(PtyBoundaryLoopAction::ExitLoop) => {
                        event_loop.exit();
                    }
                    Err(policy_error) => {
                        self.fatal_error = Some(policy_error);
                        event_loop.exit();
                    }
                }
                return;
            }
        }

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn handle_text_commit(&mut self, text: &str, event_loop: &ActiveEventLoop) {
        warn!(
            len = text.len(),
            "IME commit received unexpectedly (IME should be disabled)"
        );
        if text.is_empty() {
            return;
        }

        if let Err(error) = write_all_and_flush(&mut *self.writer, text.as_bytes()) {
            match self.handle_pty_io_error(
                SessionBoundary::PtyWrite,
                error,
                "failed to write IME text to PTY",
            ) {
                Ok(PtyBoundaryLoopAction::Continue) => {}
                Ok(PtyBoundaryLoopAction::ExitLoop) => {
                    event_loop.exit();
                }
                Err(policy_error) => {
                    self.fatal_error = Some(policy_error);
                    event_loop.exit();
                }
            }
            return;
        }

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn handle_pty_io_error(
        &mut self,
        boundary: SessionBoundary,
        error: io::Error,
        error_context: &'static str,
    ) -> Result<PtyBoundaryLoopAction> {
        if is_disconnect_error(&error)
            && let Some(code) = self
                .pty
                .try_wait()
                .context("failed to poll PTY after disconnecting GUI I/O failure")?
        {
            self.exit_code = Some(code);
            info!(
                boundary = session_boundary_token(boundary),
                code, "PTY child already exited after disconnecting GUI I/O failure"
            );
            return Ok(PtyBoundaryLoopAction::ExitLoop);
        }

        let detail = format!("{error_context}: {error}");
        self.handle_pty_boundary_failure(boundary, &detail)
    }

    fn handle_pty_boundary_failure(
        &mut self,
        boundary: SessionBoundary,
        detail: &str,
    ) -> Result<PtyBoundaryLoopAction> {
        match classify_pty_boundary_failure(&mut self.session_policy, boundary)? {
            PtyBoundaryPolicyDecision::Continue {
                attempt,
                remaining_budget,
            } => {
                warn!(
                    boundary = session_boundary_token(boundary),
                    attempt,
                    remaining_budget,
                    state = self.session_policy.state().as_str(),
                    detail,
                    "recoverable PTY boundary failure in GUI runtime; continuing in degraded mode"
                );
                self.emit_runtime_notice(&format!(
                    "[runtime] recoverable pty-boundary={} attempt={} remaining-budget={} detail={detail}",
                    session_boundary_token(boundary),
                    attempt,
                    remaining_budget,
                ));
                Ok(PtyBoundaryLoopAction::Continue)
            }
            PtyBoundaryPolicyDecision::Fatal { reason } => Err(anyhow!(
                "fatal PTY boundary failure boundary={} reason={} detail={detail}",
                session_boundary_token(boundary),
                fatal_boundary_reason_token(reason),
            )),
        }
    }

    fn mark_pty_boundary_recovered(&mut self, boundary: SessionBoundary) -> Result<()> {
        if self.session_policy.state() != SessionState::Degraded {
            return Ok(());
        }

        let transition = self.session_policy.mark_running().map_err(|error| {
            anyhow!(
                "failed to mark PTY boundary recovery boundary={}: {error}",
                session_boundary_token(boundary),
            )
        })?;

        info!(
            boundary = session_boundary_token(boundary),
            from = transition.from.as_str(),
            to = transition.to.as_str(),
            "PTY boundary recovered; GUI runtime returned to running state"
        );
        self.emit_runtime_notice(&format!(
            "[runtime] recovered pty-boundary={}",
            session_boundary_token(boundary),
        ));
        Ok(())
    }

    fn handle_clipboard_paste(&mut self, event_loop: &ActiveEventLoop) {
        let Some(text) = read_clipboard_text_for_paste(self.clipboard.as_ref()) else {
            return;
        };
        debug!(bytes = text.len(), "clipboard paste");
        let text = cap_paste_text(&text);
        let paste_result = if self.terminal.bracketed_paste_enabled() {
            write_all_and_flush(&mut *self.writer, b"\x1b[200~")
                .and_then(|()| write_all_and_flush(&mut *self.writer, text.as_bytes()))
                .and_then(|()| write_all_and_flush(&mut *self.writer, b"\x1b[201~"))
        } else {
            write_all_and_flush(&mut *self.writer, text.as_bytes())
        };
        if let Err(error) = paste_result {
            match self.handle_pty_io_error(
                SessionBoundary::PtyWrite,
                error,
                "failed to write clipboard paste to PTY",
            ) {
                Ok(PtyBoundaryLoopAction::Continue) => {}
                Ok(PtyBoundaryLoopAction::ExitLoop) => {
                    event_loop.exit();
                }
                Err(policy_error) => {
                    self.fatal_error = Some(policy_error);
                    event_loop.exit();
                }
            }
            return;
        }
        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn request_window_redraw(&self) -> bool {
        if let Some(window_control) = self.window_control.as_ref() {
            if let Err(error) = window_control.request_redraw() {
                warn!(
                    error = %error,
                    "failed to request redraw via window control"
                );
                return false;
            }
            return true;
        }
        warn!("window control unavailable while requesting redraw");
        false
    }

    fn set_window_title(&self, title: &str) {
        if let Some(window_control) = self.window_control.as_ref() {
            if let Err(error) = window_control.set_title(title) {
                warn!(
                    error = %error,
                    "failed to set title via window control"
                );
            }
            return;
        }
        warn!("window control unavailable while setting title");
    }

    fn emit_close_intent(&self) {
        if let Some(window_control) = self.window_control.as_ref()
            && let Err(error) = window_control.close()
        {
            warn!(
                error = %error,
                "failed to propagate close intent via window control"
            );
        }
    }

    fn draw_frame(&mut self) -> Result<()> {
        self.render_attempt_sequence = self.render_attempt_sequence.saturating_add(1);
        let render_attempt_sequence = self.render_attempt_sequence;

        trace!(
            render_path = ?self.ui_runtime.active_render_path(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            render_attempt_sequence,
            "draw_frame: begin"
        );

        if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
            && self.gpu_renderer.is_initialized()
        {
            let dirty_rows = self.terminal.grid.dirty_rows();
            let scroll_count = self.terminal.grid.scroll_count();
            match self
                .gpu_renderer
                .render_frame(&self.terminal, dirty_rows, scroll_count)
            {
                Ok(()) => {
                    self.terminal.grid.clear_dirty_rows();
                    let _ = self
                        .ui_runtime
                        .handle_command(UiRuntimeCommand::GpuFramePresented)
                        .context("failed to dispatch UiRuntimeCommand::GpuFramePresented")?;
                    trace!("draw_frame: presented (GPU)");
                    return Ok(());
                }
                Err(error) => {
                    self.gpu_failure_sequence = self.gpu_failure_sequence.saturating_add(1);
                    let gpu_failure_sequence = self.gpu_failure_sequence;
                    let observed_at_millis =
                        self.started_at
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64;
                    let failure_kind = error.failure_kind();

                    warn!(
                        gpu_failure_sequence,
                        render_attempt_sequence,
                        failure_kind = ?failure_kind,
                        gpu_error = ?error,
                        observed_at_millis,
                        mode = ?self.ui_runtime.render_mode(),
                        active_path = ?self.ui_runtime.active_render_path(),
                        "gpu render failed; routing through ui runtime command path"
                    );

                    match dispatch_gpu_failure_command(
                        &mut self.ui_runtime,
                        failure_kind,
                        observed_at_millis,
                    )? {
                        GpuFailureHandling::RetryScheduled {
                            failure_streak,
                            retry_budget_remaining,
                        } => {
                            warn!(
                                gpu_failure_sequence,
                                render_attempt_sequence,
                                failure_kind = ?failure_kind,
                                failure_streak,
                                retry_budget_remaining,
                                mode = ?self.ui_runtime.render_mode(),
                                active_path = ?self.ui_runtime.active_render_path(),
                                "gpu retry scheduled; session remains active"
                            );
                            self.queue_redraw();
                            return Ok(());
                        }
                        GpuFailureHandling::FallbackToCpu {
                            transition_sequence,
                        } => {
                            self.gpu_renderer.release_backend();
                            self.sync_deferred_gpu_init_state();
                            // Force full redraw on CPU path: GPU previously cleared dirty
                            // flags via take_dirty_rows, so the CPU softbuffer has no valid
                            // content and needs every row repainted.
                            self.terminal.grid.mark_all_dirty();
                            let (diagnostics_event, fallback_notice) =
                                emit_gpu_auto_fallback_observability(
                                    &self.diagnostics,
                                    transition_sequence,
                                    gpu_failure_sequence,
                                    render_attempt_sequence,
                                    failure_kind,
                                    observed_at_millis,
                                );
                            warn!(
                                gpu_failure_sequence,
                                render_attempt_sequence,
                                transition_sequence,
                                diagnostics_event_id = %diagnostics_event.event_id,
                                diagnostics_correlation = ?diagnostics_event.correlation_id,
                                mode = ?self.ui_runtime.render_mode(),
                                active_path = ?self.ui_runtime.active_render_path(),
                                "gpu failure applied deterministic cpu fallback; session remains active"
                            );
                            self.emit_runtime_notice(&fallback_notice);

                            // Defensive: if session was degraded from prior PTY boundary issue,
                            // re-mark as running since terminal is operational on CPU path
                            if self.session_policy.state() == SessionState::Degraded
                                && let Err(error) = self.session_policy.mark_running()
                            {
                                tracing::warn!(%error, "session mark_running after CPU fallback failed");
                            }
                        }
                        GpuFailureHandling::FatalForcedGpu => {
                            return Err(anyhow!(
                                "forced gpu mode render failure: kind={failure_kind:?} observed_at_millis={observed_at_millis} render_attempt_sequence={render_attempt_sequence} gpu_failure_sequence={gpu_failure_sequence}"
                            ));
                        }
                        GpuFailureHandling::Ignored => {
                            trace!(
                                "draw_frame: gpu failure handling ignored (already on CPU path)"
                            );
                        }
                    }
                }
            }
        }

        let width = self.window_size.width;
        let height = self.window_size.height;
        if width == 0 || height == 0 {
            debug!(width, height, "draw_frame: skipped, zero window dimensions");
            return Ok(());
        }

        // Lazily create softbuffer surface on first CPU render (e.g. after GPU fallback).
        // Cannot create at bootstrap when GPU surface already owns the Wayland buffer queue.
        self.ensure_softbuffer_surface()
            .context("failed to initialize softbuffer for CPU render")?;

        let surface = self
            .surface
            .as_mut()
            .ok_or_else(|| anyhow!("softbuffer surface is not initialized"))?;

        let nz_width = NonZeroU32::new(width).ok_or_else(|| anyhow!("zero width is invalid"))?;
        let nz_height = NonZeroU32::new(height).ok_or_else(|| anyhow!("zero height is invalid"))?;
        let target_size = PhysicalSize::new(width, height);
        if self.last_softbuffer_size != Some(target_size) {
            surface
                .resize(nz_width, nz_height)
                .map_err(|error| anyhow!("failed to resize softbuffer surface: {error}"))?;
            self.last_softbuffer_size = Some(target_size);
        }

        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!("failed to acquire softbuffer frame: {error}"))?;
        render_terminal(
            &mut buffer,
            width as usize,
            height as usize,
            &mut self.terminal,
            &mut self.glyph_cache,
            self.last_rendered_cursor_row,
            &mut self.dirty_rows_scratch,
        );
        self.last_rendered_cursor_row = Some(self.terminal.cursor.row);
        buffer
            .present()
            .map_err(|error| anyhow!("failed to present GUI frame: {error}"))?;
        trace!("draw_frame: presented (CPU)");
        Ok(())
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
                let capped_size = cap_framebuffer_extent(size);
                if capped_size != size {
                    warn!(
                        requested_width = size.width,
                        requested_height = size.height,
                        capped_width = capped_size.width,
                        capped_height = capped_size.height,
                        max_width = MAX_FRAMEBUFFER_WIDTH,
                        max_height = MAX_FRAMEBUFFER_HEIGHT,
                        max_pixels = MAX_FRAMEBUFFER_PIXELS,
                        "window framebuffer exceeded runtime safety limits; dimensions were clamped"
                    );
                }
                self.window_size = capped_size;
                if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
                    && self.gpu_renderer.is_initialized()
                {
                    self.gpu_renderer
                        .resize(self.window_size.width, self.window_size.height);
                }
                self.update_viewport_geometry(event_loop);
                self.handle_monitor_affecting_event(MonitorAffectingWindowEvent::Resized);
                self.queue_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    let raw_size = window.inner_size();
                    let capped_size = cap_framebuffer_extent(raw_size);
                    if capped_size != raw_size {
                        warn!(
                            requested_width = raw_size.width,
                            requested_height = raw_size.height,
                            capped_width = capped_size.width,
                            capped_height = capped_size.height,
                            max_width = MAX_FRAMEBUFFER_WIDTH,
                            max_height = MAX_FRAMEBUFFER_HEIGHT,
                            max_pixels = MAX_FRAMEBUFFER_PIXELS,
                            "scale-factor framebuffer exceeded runtime safety limits; dimensions were clamped"
                        );
                    }
                    self.window_size = capped_size;
                    if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
                        && self.gpu_renderer.is_initialized()
                    {
                        self.gpu_renderer
                            .resize(self.window_size.width, self.window_size.height);
                    }
                    self.update_viewport_geometry(event_loop);
                    self.handle_monitor_affecting_event(
                        MonitorAffectingWindowEvent::ScaleFactorChanged,
                    );
                    self.queue_redraw();
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
        if self.gpu_init_pending {
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
        if self.gpu_init_pending
            && let Some(retry_at) = self.next_gpu_init_retry_at
            && Instant::now() < retry_at
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(retry_at));
            return;
        }

        self.request_redraw_if_needed();

        trace!(
            render_path = ?self.ui_runtime.active_render_path(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            "about_to_wait: selecting control flow"
        );

        if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
            && self.gpu_renderer.is_initialized()
        {
            // GPU: VSync drives frame pacing via PresentMode::AutoVsync.
            // Wait sleeps until the next event (PTY data, input, resize).
            // PTY proxy wakes the loop via EventLoopProxy — no busy-spin needed.
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            // CPU: software timer for frame pacing (no VSync via softbuffer).
            if self.redraw_pending {
                let cadence = self.ui_runtime.cadence();
                match cadence.frame_interval() {
                    Some(interval) => {
                        event_loop
                            .set_control_flow(ControlFlow::WaitUntil(Instant::now() + interval));
                    }
                    None => {
                        // No cadence available (headless, VNC, or monitor detection failed).
                        // Wait for events to avoid busy-spin.
                        event_loop.set_control_flow(ControlFlow::Wait);
                    }
                }
            } else {
                // Nothing dirty in CPU fallback path: sleep until external input/PTY events.
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }
}

fn render_terminal(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    terminal: &mut TerminalState,
    glyph_cache: &mut GlyphCache,
    prev_cursor_row: Option<u16>,
    dirty_rows_scratch: &mut Vec<u16>,
) {
    if width == 0 || height == 0 {
        return;
    }

    let grid_rows = terminal.grid.height() as usize;
    let grid_cols = terminal.grid.width() as usize;
    let visible_rows = (height / CELL_HEIGHT).max(1).min(grid_rows);
    let visible_cols = (width / CELL_WIDTH).max(1).min(grid_cols);

    // Build dirty set: grid dirty rows + cursor rows (current and previous)
    // Using dirty_rows() &[bool] for O(1) lookup instead of Vec::contains O(n)
    let dirty_flags = terminal.grid.dirty_rows();
    let cursor_row = terminal.cursor.row;

    let mut dirty = std::mem::take(dirty_rows_scratch);
    dirty.clear();
    if dirty.capacity() < (visible_rows / 4 + 2) {
        dirty.reserve((visible_rows / 4 + 2) - dirty.capacity());
    }
    for row in 0..visible_rows {
        let r = row as u16;
        if dirty_flags.get(row).copied().unwrap_or(false)
            || r == cursor_row
            || prev_cursor_row == Some(r)
        {
            dirty.push(r);
        }
    }
    terminal.grid.clear_dirty_rows();

    if dirty.is_empty() {
        *dirty_rows_scratch = dirty;
        return;
    }

    // Render only dirty rows
    for &row in &dirty {
        let row_idx = row as usize;
        if row_idx >= visible_rows {
            continue;
        }
        let base_y = row_idx * CELL_HEIGHT;
        let clear_end_y = (base_y + CELL_HEIGHT).min(height);

        // Clear row band to default background
        for py in base_y..clear_end_y {
            let start = py * width;
            buffer[start..start + width].fill(DEFAULT_BG_U32);
        }

        // Redraw cells for this row
        if let Ok(cells) = terminal.grid.row_cells(row) {
            for (col, cell) in cells.iter().take(visible_cols).enumerate() {
                let x = col * CELL_WIDTH;
                let (fg, bg) = resolve_cell_colors(&cell.attrs);

                if bg != DEFAULT_BG_U32 {
                    draw_cell_bg(buffer, width, height, x, base_y, bg);
                }

                if cell.ch != ' ' {
                    let glyph = glyph_cache.get(cell.ch);
                    draw_glyph_blended(
                        buffer,
                        width,
                        height,
                        x,
                        base_y,
                        glyph,
                        fg,
                        cell.attrs.bold,
                    );
                }

                if cell.attrs.underline {
                    draw_underline(buffer, width, height, x, base_y, fg);
                }

                if cell.attrs.strikethrough {
                    draw_strikethrough(buffer, width, height, x, base_y, fg);
                }
            }
        }
    }

    // Clear area below the grid (handles first frame and resize)
    let grid_pixel_height = visible_rows * CELL_HEIGHT;
    if grid_pixel_height < height {
        let any_bottom_dirty = dirty.iter().any(|&r| (r as usize) + 1 >= visible_rows);
        if any_bottom_dirty {
            for py in grid_pixel_height..height {
                let start = py * width;
                buffer[start..start + width].fill(DEFAULT_BG_U32);
            }
        }
    }

    // Draw cursor
    if terminal.cursor.visible {
        let crow = terminal.cursor.row as usize;
        let ccol = terminal.cursor.col as usize;
        if crow < visible_rows && ccol < visible_cols {
            draw_cursor(buffer, width, height, ccol * CELL_WIDTH, crow * CELL_HEIGHT);
        }
    }

    *dirty_rows_scratch = dirty;
}

fn resolve_cell_colors(attrs: &grid::Attrs) -> (u32, u32) {
    let mut fg = grid::color_to_u32(attrs.fg, DEFAULT_FG);
    let mut bg = grid::color_to_u32(attrs.bg, DEFAULT_BG);

    if attrs.dim {
        let (r, g, b) = u32_to_rgb(fg);
        fg = rgb_to_u32(r / 2, g / 2, b / 2);
    }

    if attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    (fg, bg)
}

const fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn u32_to_rgb(c: u32) -> (u8, u8, u8) {
    ((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn draw_cell_bg(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, bg: u32) {
    for py in y..(y + CELL_HEIGHT).min(height) {
        let row_start = py * width;
        for px in x..(x + CELL_WIDTH).min(width) {
            buffer[row_start + px] = bg;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_blended(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    cell_x: usize,
    cell_y: usize,
    glyph: &rldyourterm_font::GlyphBitmap,
    fg: u32,
    bold: bool,
) {
    if glyph.glyph_width == 0 || glyph.glyph_height == 0 {
        return;
    }

    let (fg_r, fg_g, fg_b) = u32_to_rgb(fg);

    for gy in 0..glyph.glyph_height {
        for gx in 0..glyph.glyph_width {
            let coverage = glyph.data[gy * glyph.glyph_width + gx];
            if coverage == 0 {
                continue;
            }

            let px = cell_x as i32 + glyph.x_offset + gx as i32;
            let py = cell_y as i32 + glyph.y_offset + gy as i32;
            if px < 0 || py < 0 {
                continue;
            }
            let px = px as usize;
            let py = py as usize;
            if px >= width || py >= height {
                continue;
            }

            let idx = py * width + px;
            let (bg_r, bg_g, bg_b) = u32_to_rgb(buffer[idx]);

            // Alpha blend (expanded form, no subtraction to avoid u32 underflow):
            // result = bg * (255 - a) / 255 + fg * a / 255
            let a = coverage as u32;
            let inv_a = 255 - a;
            let r = (bg_r as u32 * inv_a + fg_r as u32 * a) / 255;
            let g = (bg_g as u32 * inv_a + fg_g as u32 * a) / 255;
            let b = (bg_b as u32 * inv_a + fg_b as u32 * a) / 255;
            buffer[idx] = rgb_to_u32(r as u8, g as u8, b as u8);

            // Bold via double-strike (1px right shift)
            if bold && px + 1 < width {
                let bold_idx = py * width + px + 1;
                let (bbg_r, bbg_g, bbg_b) = u32_to_rgb(buffer[bold_idx]);
                let br = (bbg_r as u32 * inv_a + fg_r as u32 * a) / 255;
                let bg_val = (bbg_g as u32 * inv_a + fg_g as u32 * a) / 255;
                let bb = (bbg_b as u32 * inv_a + fg_b as u32 * a) / 255;
                buffer[bold_idx] = rgb_to_u32(br as u8, bg_val as u8, bb as u8);
            }
        }
    }
}

fn draw_underline(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, fg: u32) {
    let line_y = y + CELL_HEIGHT - 1;
    if line_y >= height {
        return;
    }
    let row_start = line_y * width;
    for px in x..(x + CELL_WIDTH).min(width) {
        buffer[row_start + px] = fg;
    }
}

fn draw_strikethrough(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    fg: u32,
) {
    let line_y = y + CELL_HEIGHT / 2;
    if line_y >= height {
        return;
    }
    let row_start = line_y * width;
    for px in x..(x + CELL_WIDTH).min(width) {
        buffer[row_start + px] = fg;
    }
}

fn draw_cursor(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize) {
    for glyph_y in 0..CELL_HEIGHT {
        let pixel_y = y + glyph_y;
        if pixel_y >= height {
            break;
        }
        for glyph_x in 0..CELL_WIDTH {
            let pixel_x = x + glyph_x;
            if pixel_x >= width {
                break;
            }
            let index = pixel_y * width + pixel_x;
            buffer[index] ^= 0x00FF_FFFF;
        }
    }
}

fn join_pump_thread_with_timeout(handle: JoinHandle<()>, thread_label: &'static str) {
    let deadline = Instant::now() + SHUTDOWN_JOIN_TIMEOUT;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(SHUTDOWN_JOIN_POLL_INTERVAL);
    }

    if handle.is_finished() {
        if let Err(join_error) = handle.join() {
            warn!(?join_error, thread_label, "GUI shutdown thread join failed");
        }
        return;
    }

    // One final immediate check reduces false timeout logs in the race window
    // where the worker finishes right after the bounded polling loop exits.
    if handle.is_finished() {
        if let Err(join_error) = handle.join() {
            warn!(?join_error, thread_label, "GUI shutdown thread join failed");
        }
        return;
    }

    warn!(
        thread_label,
        timeout_ms = SHUTDOWN_JOIN_TIMEOUT.as_millis(),
        "GUI shutdown thread join timed out; detaching thread to avoid shutdown hang"
    );
}

fn is_runtime_palette_shortcut(event: &WinitKeyEvent, modifiers: ModifiersState) -> bool {
    is_runtime_palette_shortcut_key(event.logical_key.as_ref(), modifiers)
}

fn is_runtime_palette_shortcut_key(key: Key<&str>, modifiers: ModifiersState) -> bool {
    if !modifiers.shift_key() || !(modifiers.control_key() || modifiers.super_key()) {
        return false;
    }

    match key {
        Key::Character(text) => text.eq_ignore_ascii_case("p"),
        _ => false,
    }
}

fn runtime_palette_action_for_winit_key(
    key: Key<&str>,
    diagnostics_enabled: bool,
) -> Option<RuntimePaletteAction> {
    match key {
        Key::Named(NamedKey::Escape) => Some(RuntimePaletteAction::Close),
        Key::Character("1") => Some(RuntimePaletteAction::ApplyCommand("mode cpu")),
        Key::Character("2") => Some(RuntimePaletteAction::ApplyCommand("mode gpu")),
        Key::Character("3") => Some(RuntimePaletteAction::ApplyCommand("mode auto")),
        Key::Character(text) if text.eq_ignore_ascii_case("d") => {
            if diagnostics_enabled {
                Some(RuntimePaletteAction::ApplyCommand("debug off"))
            } else {
                Some(RuntimePaletteAction::ApplyCommand("debug on"))
            }
        }
        Key::Character(text) if text.eq_ignore_ascii_case("i") => {
            Some(RuntimePaletteAction::ShowInfo)
        }
        _ => None,
    }
}

fn dispatch_runtime_palette_command(
    ui_runtime: &mut UiRuntime,
    settings: &mut SettingsService,
    input: &str,
) -> Result<String> {
    match settings.apply_palette_command(input) {
        SettingsPaletteApplyOutcome::Applied {
            command, current, ..
        } => {
            apply_palette_settings_command_to_ui_runtime(ui_runtime, command)?;
            Ok(runtime_palette_status_line(
                command,
                current.mode,
                current.debug_mode,
                ui_runtime.active_render_path(),
            ))
        }
        SettingsPaletteApplyOutcome::Noop { command, state, .. } => {
            apply_palette_settings_command_to_ui_runtime(ui_runtime, command)?;
            Ok(runtime_palette_status_line(
                command,
                state.mode,
                state.debug_mode,
                ui_runtime.active_render_path(),
            ))
        }
        SettingsPaletteApplyOutcome::Rejected { reason, .. } => {
            warn!(?reason, input = input, "runtime palette command rejected");
            Ok(format!(
                "[palette] rejected input={input} reason={reason:?}"
            ))
        }
    }
}

fn apply_palette_settings_command_to_ui_runtime(
    ui_runtime: &mut UiRuntime,
    command: SettingsCommand,
) -> Result<()> {
    if let SettingsCommand::SetMode(mode) = command {
        let _ = ui_runtime
            .handle_command(UiRuntimeCommand::SetRenderMode(mode))
            .context("failed to dispatch UiRuntimeCommand::SetRenderMode from runtime palette")?;
    }
    Ok(())
}

fn runtime_palette_status_line(
    command: SettingsCommand,
    mode: RenderMode,
    diagnostics_enabled: bool,
    active_render_path: ActiveRenderPath,
) -> String {
    match command {
        SettingsCommand::SetMode(_) => format!(
            "[palette] mode={} active-path={}",
            render_mode_token(mode),
            active_render_path_token(active_render_path),
        ),
        SettingsCommand::SetDebugMode(_) => format!(
            "[palette] diagnostics={} mode={} active-path={}",
            on_off_token(diagnostics_enabled),
            render_mode_token(mode),
            active_render_path_token(active_render_path),
        ),
        SettingsCommand::SetShellTarget(_)
        | SettingsCommand::SetShellAutoInit(_)
        | SettingsCommand::SetRenderCadencePolicy(_)
        | SettingsCommand::SetTheme(_)
        | SettingsCommand::SetRuntimeProfile(_) => {
            format!("[palette] saved (restart required) input={command:?}")
        }
    }
}

fn runtime_palette_info_line(ui_runtime: &UiRuntime, settings: &SettingsService) -> String {
    format!(
        "[palette] info mode={} active-path={} diagnostics={}",
        render_mode_token(ui_runtime.render_mode()),
        active_render_path_token(ui_runtime.active_render_path()),
        on_off_token(settings.state().debug_mode),
    )
}

fn active_render_path_token(path: ActiveRenderPath) -> &'static str {
    match path {
        ActiveRenderPath::Cpu => "cpu",
        ActiveRenderPath::Gpu => "gpu",
    }
}

fn sample_monitor_refresh_rate_millihz(window_control: Option<&dyn WindowControl>) -> Option<u32> {
    let window_control = window_control?;

    match window_control.current_monitor_timing() {
        Ok(MonitorTiming {
            refresh_rate_millihz,
            ..
        }) => refresh_rate_millihz,
        Err(error) => {
            warn!(
                error = %error,
                "failed to sample monitor timing via window control"
            );
            None
        }
    }
}

fn cadence_resync_command_for_monitor_event(
    monitor_event: MonitorAffectingWindowEvent,
    sampled_refresh_rate_millihz: Option<u32>,
) -> UiRuntimeCommand {
    let refresh_rate_millihz = sampled_refresh_rate_millihz.unwrap_or(0);
    match monitor_event {
        MonitorAffectingWindowEvent::Moved | MonitorAffectingWindowEvent::ScaleFactorChanged => {
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz,
            }
        }
        MonitorAffectingWindowEvent::Resized => UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz,
        },
    }
}

fn monitor_affecting_event_token(event: MonitorAffectingWindowEvent) -> &'static str {
    match event {
        MonitorAffectingWindowEvent::Moved => "moved",
        MonitorAffectingWindowEvent::Resized => "resized",
        MonitorAffectingWindowEvent::ScaleFactorChanged => "scale-factor-changed",
    }
}

fn gpu_auto_fallback_correlation_id(
    transition_sequence: u64,
    gpu_failure_sequence: u64,
) -> CorrelationId {
    CorrelationId::new(format!(
        "gpu-auto-fallback-transition-{transition_sequence}-failure-{gpu_failure_sequence}"
    ))
}

fn emit_gpu_auto_fallback_observability(
    diagnostics: &DiagnosticsSink,
    transition_sequence: u64,
    gpu_failure_sequence: u64,
    render_attempt_sequence: u64,
    failure_kind: GpuFailureKind,
    observed_at_millis: u64,
) -> (Event, String) {
    let correlation_id =
        gpu_auto_fallback_correlation_id(transition_sequence, gpu_failure_sequence);
    let diagnostics_message = format!(
        "gpu auto-fallback applied transition-seq={transition_sequence} failure-seq={gpu_failure_sequence} render-attempt-seq={render_attempt_sequence} failure-kind={failure_kind:?} observed-ms={observed_at_millis}"
    );
    let event = diagnostics
        .with_correlation(correlation_id.clone())
        .emit_kind(EventKind::RenderModeTransition, diagnostics_message);
    let notice = format!(
        "[runtime] gpu auto-fallback transition-seq={transition_sequence} failure-seq={gpu_failure_sequence} render-attempt-seq={render_attempt_sequence} failure={failure_kind:?} observed-ms={observed_at_millis} correlation-id={}",
        correlation_id.as_str()
    );
    (event, notice)
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

fn is_local_shutdown_key(event: &WinitKeyEvent, modifiers: ModifiersState) -> bool {
    if !modifiers.control_key() {
        return false;
    }

    match event.logical_key.as_ref() {
        Key::Character(text) => text.eq_ignore_ascii_case("q"),
        _ => false,
    }
}

fn encode_winit_key_event(key: &Key, modifiers: ModifiersState) -> Option<Vec<u8>> {
    let mod_param = xterm_modifier_param(
        modifiers.shift_key(),
        modifiers.alt_key(),
        modifiers.control_key(),
    );
    let has_mod = mod_param > 1;

    match key.as_ref() {
        Key::Named(NamedKey::Enter) => Some(vec![b'\r']),
        Key::Named(NamedKey::Tab) if modifiers.shift_key() => Some(b"\x1b[Z".to_vec()),
        Key::Named(NamedKey::Tab) => Some(vec![b'\t']),
        Key::Named(NamedKey::Escape) => Some(vec![0x1b]),
        Key::Named(NamedKey::Backspace) if modifiers.alt_key() => Some(b"\x1b\x7f".to_vec()),
        Key::Named(NamedKey::Backspace) => Some(vec![0x7f]),
        Key::Named(NamedKey::ArrowUp) => Some(csi_modified(b'A', mod_param, has_mod)),
        Key::Named(NamedKey::ArrowDown) => Some(csi_modified(b'B', mod_param, has_mod)),
        Key::Named(NamedKey::ArrowRight) => Some(csi_modified(b'C', mod_param, has_mod)),
        Key::Named(NamedKey::ArrowLeft) => Some(csi_modified(b'D', mod_param, has_mod)),
        Key::Named(NamedKey::Home) => Some(csi_modified(b'H', mod_param, has_mod)),
        Key::Named(NamedKey::End) => Some(csi_modified(b'F', mod_param, has_mod)),
        Key::Named(NamedKey::Delete) => Some(tilde_modified(3, mod_param, has_mod)),
        Key::Named(NamedKey::Insert) => Some(tilde_modified(2, mod_param, has_mod)),
        Key::Named(NamedKey::PageUp) => Some(tilde_modified(5, mod_param, has_mod)),
        Key::Named(NamedKey::PageDown) => Some(tilde_modified(6, mod_param, has_mod)),
        Key::Named(NamedKey::F1) => Some(fkey_ss3_modified(b'P', mod_param, has_mod)),
        Key::Named(NamedKey::F2) => Some(fkey_ss3_modified(b'Q', mod_param, has_mod)),
        Key::Named(NamedKey::F3) => Some(fkey_ss3_modified(b'R', mod_param, has_mod)),
        Key::Named(NamedKey::F4) => Some(fkey_ss3_modified(b'S', mod_param, has_mod)),
        Key::Named(NamedKey::F5) => Some(tilde_modified(15, mod_param, has_mod)),
        Key::Named(NamedKey::F6) => Some(tilde_modified(17, mod_param, has_mod)),
        Key::Named(NamedKey::F7) => Some(tilde_modified(18, mod_param, has_mod)),
        Key::Named(NamedKey::F8) => Some(tilde_modified(19, mod_param, has_mod)),
        Key::Named(NamedKey::F9) => Some(tilde_modified(20, mod_param, has_mod)),
        Key::Named(NamedKey::F10) => Some(tilde_modified(21, mod_param, has_mod)),
        Key::Named(NamedKey::F11) => Some(tilde_modified(23, mod_param, has_mod)),
        Key::Named(NamedKey::F12) => Some(tilde_modified(24, mod_param, has_mod)),
        Key::Character(text) if modifiers.control_key() => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            encode_ctrl_letter(ch).map(|code| vec![code])
        }
        _ => None,
    }
}

fn is_paste_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    let is_v = match key.as_ref() {
        Key::Character(text) => text.eq_ignore_ascii_case("v"),
        _ => false,
    };
    if !is_v {
        return false;
    }

    // macOS: Cmd+V, Linux: Ctrl+Shift+V
    #[cfg(target_os = "macos")]
    {
        modifiers.super_key()
    }
    #[cfg(not(target_os = "macos"))]
    {
        modifiers.control_key() && modifiers.shift_key()
    }
}

fn read_clipboard_text_for_paste(clipboard: &dyn ClipboardAdapter) -> Option<String> {
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Some(text),
        Ok(_) | Err(_) => {
            debug!("clipboard paste: empty or unavailable");
            None
        }
    }
}

fn cap_paste_text(text: &str) -> &str {
    if text.len() <= CLIPBOARD_PASTE_CAP_BYTES {
        text
    } else {
        let mut end = CLIPBOARD_PASTE_CAP_BYTES;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    }
}

fn cap_terminal_geometry(raw_cols: usize, raw_rows: usize) -> (u16, u16) {
    let cols = raw_cols.clamp(1, MAX_VIEWPORT_COLS);
    let mut rows = raw_rows.clamp(1, MAX_VIEWPORT_ROWS);

    if cols.saturating_mul(rows) > MAX_VIEWPORT_CELLS {
        rows = (MAX_VIEWPORT_CELLS / cols.max(1)).max(1);
    }

    (cols as u16, rows as u16)
}

fn cap_framebuffer_extent(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    let mut width = size.width.clamp(1, MAX_FRAMEBUFFER_WIDTH);
    let mut height = size.height.clamp(1, MAX_FRAMEBUFFER_HEIGHT);

    let pixels = u64::from(width) * u64::from(height);
    if pixels > MAX_FRAMEBUFFER_PIXELS {
        let scale = ((MAX_FRAMEBUFFER_PIXELS as f64) / (pixels as f64)).sqrt();
        width = ((width as f64 * scale).floor() as u32).clamp(1, MAX_FRAMEBUFFER_WIDTH);
        height = ((height as f64 * scale).floor() as u32).clamp(1, MAX_FRAMEBUFFER_HEIGHT);

        while u64::from(width) * u64::from(height) > MAX_FRAMEBUFFER_PIXELS {
            if width >= height && width > 1 {
                width = width.saturating_sub(1);
            } else if height > 1 {
                height = height.saturating_sub(1);
            } else {
                break;
            }
        }
    }

    PhysicalSize::new(width, height)
}

fn deferred_gpu_init_backoff(attempt: u8) -> Duration {
    let exponent = attempt.saturating_sub(1).min(3);
    let multiplier = 1_u32 << exponent;
    DEFERRED_GPU_INIT_MIN_BACKOFF
        .saturating_mul(multiplier)
        .min(DEFERRED_GPU_INIT_MAX_BACKOFF)
}

fn should_flush_output_batch(current_batch_len: usize, incoming_chunk_len: usize) -> bool {
    current_batch_len > 0
        && current_batch_len.saturating_add(incoming_chunk_len) > OUTPUT_BATCH_MAX_BYTES
}

fn output_drain_budget(snapshot: OutputQueueSnapshot) -> OutputDrainBudget {
    if snapshot.queued_bytes >= OUTPUT_DRAIN_CRITICAL_QUEUE_BYTES
        || snapshot.queued_chunks >= OUTPUT_DRAIN_CRITICAL_QUEUE_CHUNKS
    {
        return OutputDrainBudget {
            pressure: OutputDrainPressure::Critical,
            max_bytes_per_tick: OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_CRITICAL_MAX_LATENCY,
        };
    }

    if snapshot.queued_bytes >= OUTPUT_DRAIN_ELEVATED_QUEUE_BYTES
        || snapshot.queued_chunks >= OUTPUT_DRAIN_ELEVATED_QUEUE_CHUNKS
    {
        return OutputDrainBudget {
            pressure: OutputDrainPressure::Elevated,
            max_bytes_per_tick: OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
            max_latency: OUTPUT_DRAIN_ELEVATED_MAX_LATENCY,
        };
    }

    OutputDrainBudget {
        pressure: OutputDrainPressure::Normal,
        max_bytes_per_tick: OUTPUT_DRAIN_MAX_BYTES_PER_TICK,
        max_latency: OUTPUT_DRAIN_MAX_LATENCY,
    }
}

fn output_drain_budget_exhausted(
    drained_bytes: usize,
    elapsed: Duration,
    budget: OutputDrainBudget,
) -> bool {
    drained_bytes >= budget.max_bytes_per_tick || elapsed >= budget.max_latency
}

fn child_exit_drain_timed_out(started_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(started_at) >= CHILD_EXIT_DRAIN_MAX_WAIT
}

fn terminal_feed_chunks(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.chunks(MAX_FEED_BYTES_PER_CALL)
}

/// Generates a 32x32 RGBA programmatic icon: dark background with a cyan terminal cursor.
fn load_app_icon() -> Option<Icon> {
    let img = match image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png) {
        Ok(img) => img,
        Err(e) => {
            warn!(error = ?e, "failed to decode embedded LOGO.png");
            return None;
        }
    };
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    match Icon::from_rgba(rgba.into_raw(), width, height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            warn!(error = ?e, "failed to construct window icon from RGBA data");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CHILD_EXIT_DRAIN_MAX_WAIT, CLIPBOARD_PASTE_CAP_BYTES, DEFAULT_FG, DEFAULT_FG_U32,
        GpuFailureHandling, MAX_FEED_BYTES_PER_CALL, MAX_FRAMEBUFFER_HEIGHT,
        MAX_FRAMEBUFFER_PIXELS, MAX_FRAMEBUFFER_WIDTH, MAX_VIEWPORT_CELLS, MAX_VIEWPORT_COLS,
        MAX_VIEWPORT_ROWS, MonitorAffectingWindowEvent, OUTPUT_BATCH_MAX_BYTES,
        OUTPUT_DRAIN_CRITICAL_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_ELEVATED_MAX_BYTES_PER_TICK,
        OUTPUT_DRAIN_MAX_BYTES_PER_TICK, OUTPUT_DRAIN_MAX_LATENCY, OutputDrainBudget,
        OutputDrainPressure, OutputQueueSnapshot, PtyBoundaryPolicyDecision,
        cadence_resync_command_for_monitor_event, cap_framebuffer_extent, cap_paste_text,
        cap_terminal_geometry, child_exit_drain_timed_out, classify_pty_boundary_failure,
        deferred_gpu_init_backoff, dispatch_gpu_failure_command, dispatch_runtime_palette_command,
        emit_gpu_auto_fallback_observability, encode_winit_key_event, grid,
        is_runtime_palette_shortcut_key, output_drain_budget, output_drain_budget_exhausted,
        read_clipboard_text_for_paste, resolve_cell_colors, sample_monitor_refresh_rate_millihz,
        should_flush_output_batch, terminal_feed_chunks,
    };
    use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
    use rldyourterm_foundation::api::{
        clipboard::ClipboardAdapter,
        common::{ContractResult, MonitorTiming},
        window::{WindowControl, WindowEvent as FoundationWindowEvent},
    };
    use rldyourterm_foundation::error::{
        ClipboardFailureCode, ClipboardOperation, FoundationError, Recoverability,
        WindowFailureCode, WindowOperation,
    };
    use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
    use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
    use rldyourterm_settings::SettingsService;
    use rldyourterm_ui::{UiBootstrapConfig, UiRuntime, UiRuntimeCommand};
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

        fn clipboard_text(&self) -> ContractResult<String> {
            Ok(String::new())
        }

        fn set_clipboard_text(&self, _text: &str) -> ContractResult<()> {
            Ok(())
        }

        fn close(&self) -> ContractResult<()> {
            Ok(())
        }

        fn poll_events(&self) -> ContractResult<Vec<FoundationWindowEvent>> {
            Ok(Vec::new())
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
    fn output_batch_flush_policy_only_triggers_on_overflow_with_existing_batch() {
        assert!(!should_flush_output_batch(0, 32));
        assert!(!should_flush_output_batch(128, 256));
        assert!(should_flush_output_batch(OUTPUT_BATCH_MAX_BYTES - 64, 128));
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
        let default_fg = grid::color_to_u32(grid::Color::Default, DEFAULT_FG);
        assert_eq!(default_fg, DEFAULT_FG_U32);
    }

    #[test]
    fn color_to_u32_indexed_looks_up_palette() {
        let red = grid::color_to_u32(grid::Color::Indexed(1), DEFAULT_FG);
        assert_eq!(red, grid::ANSI_PALETTE[1]);
    }

    #[test]
    fn color_to_u32_rgb_constructs_correctly() {
        let c = grid::color_to_u32(grid::Color::Rgb(0xFF, 0x80, 0x00), DEFAULT_FG);
        assert_eq!(c, 0x00FF_8000);
    }

    #[test]
    fn resolve_cell_colors_inverse_swaps_fg_bg() {
        let attrs = grid::Attrs {
            fg: grid::Color::Indexed(1),
            bg: grid::Color::Indexed(2),
            inverse: true,
            ..grid::Attrs::default()
        };
        let (fg, bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, grid::ANSI_PALETTE[2]);
        assert_eq!(bg, grid::ANSI_PALETTE[1]);
    }

    #[test]
    fn resolve_cell_colors_dim_halves_fg() {
        let attrs = grid::Attrs {
            fg: grid::Color::Rgb(200, 100, 50),
            dim: true,
            ..grid::Attrs::default()
        };
        let (fg, _bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, super::rgb_to_u32(100, 50, 25));
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
