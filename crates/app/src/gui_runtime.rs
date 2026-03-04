use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use font8x8::{
    BASIC_FONTS, BLOCK_FONTS, BOX_FONTS, GREEK_FONTS, HIRAGANA_FONTS, LATIN_FONTS, MISC_FONTS,
    UnicodeFonts,
};
use rldyourterm_core::grid::{self, CELL_HEIGHT, CELL_WIDTH};
use rldyourterm_core::state::TerminalState;
use rldyourterm_diagnostics::{CorrelationId, DiagnosticsSink, Event, EventKind};
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_render_gpu::GpuRenderer;
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::{
    FatalBoundaryReason, SessionBoundary, SessionController, SessionState, SessionTransitionOutcome,
};
use rldyourterm_settings::{SettingsCommand, SettingsPaletteApplyOutcome, SettingsService};
use rldyourterm_ui::{UiBootstrapConfig, UiCommandOutcome, UiRuntime, UiRuntimeCommand};
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};
use tracing::{info, warn};
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

const DEFAULT_GUI_WIDTH: u32 = 1280;
const DEFAULT_GUI_HEIGHT: u32 = 800;
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
use rldyourterm_ui::DEFAULT_SCROLLBACK_CAP;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const DEFAULT_BG_U32: u32 = rgb_to_u32(DEFAULT_BG.0, DEFAULT_BG.1, DEFAULT_BG.2);
#[cfg(test)]
const DEFAULT_FG_U32: u32 = rgb_to_u32(DEFAULT_FG.0, DEFAULT_FG.1, DEFAULT_FG.2);
const RUNTIME_PALETTE_HELP_LINE: &str =
    "[palette] 1:mode cpu 2:mode gpu 3:mode auto d:diagnostics toggle i:info Esc:close";
const RUNTIME_PALETTE_CLOSED_LINE: &str = "[palette] closed";

#[derive(Debug)]
enum GuiEvent {
    Output(Vec<u8>),
    Exited(i32),
    PtyFailure {
        boundary: SessionBoundary,
        message: String,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PtyBoundaryPolicyDecision {
    Continue { attempt: u8, remaining_budget: u8 },
    Fatal { reason: FatalBoundaryReason },
}

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
        other => Err(anyhow!(
            "unexpected UI outcome for GPU failure command: {other:?}"
        )),
    }
}

pub fn run_interactive_gui_pty(
    shell_executable: &str,
    shell_args: &[String],
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
    window_count: u8,
) -> Result<i32> {
    if window_count != 1 {
        return Err(anyhow!(
            "gui runtime requires single-window mode; got window_count={window_count}"
        ));
    }

    let (pty, writer, reader) = spawn_pty(shell_executable, shell_args)?;

    let event_loop = build_gui_event_loop()?;
    let proxy = event_loop.create_proxy();

    let reader_pump = spawn_reader_pump(reader, proxy.clone());
    let wait_pump = spawn_wait_pump(Arc::clone(&pty), proxy.clone());

    let mut app = GuiRuntimeApp::new(
        pty,
        writer,
        reader_pump,
        wait_pump,
        initial_mode,
        refresh_rate_millihz,
        window_count,
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

    app.shutdown();

    if let Some(error) = app.fatal_error.take() {
        return Err(error);
    }

    Ok(app.exit_code.unwrap_or(0))
}

fn build_gui_event_loop() -> Result<EventLoop<GuiEvent>> {
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
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: Vec::new(),
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
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    if proxy
                        .send_event(GuiEvent::Output(buffer[..read_bytes].to_vec()))
                        .is_err()
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
    pty: Arc<dyn PtyIo>,
    writer: Box<dyn Write + Send>,
    reader_pump: Option<JoinHandle<()>>,
    wait_pump: Option<JoinHandle<()>>,
    session_policy: SessionController,
    diagnostics: DiagnosticsSink,
    ui_runtime: UiRuntime,
    gpu_renderer: GpuRenderer,
    started_at: Instant,
    render_attempt_sequence: u64,
    gpu_failure_sequence: u64,
    initial_mode: RenderMode,

    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    _context: Option<SoftbufferContext<Arc<Window>>>,
    surface: Option<SoftbufferSurface<Arc<Window>, Arc<Window>>>,
    window_size: PhysicalSize<u32>,
    terminal: TerminalState,
    settings: SettingsService,
    modifiers: ModifiersState,
    palette_open: bool,
    redraw_pending: bool,
    last_rendered_cursor_row: Option<u16>,

    exit_code: Option<i32>,
    fatal_error: Option<anyhow::Error>,
}

impl GuiRuntimeApp {
    fn new(
        pty: Arc<dyn PtyIo>,
        writer: Box<dyn Write + Send>,
        reader_pump: JoinHandle<()>,
        wait_pump: JoinHandle<()>,
        initial_mode: RenderMode,
        refresh_rate_millihz: u32,
        window_count: u8,
    ) -> Result<Self> {
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
            pty,
            writer,
            reader_pump: Some(reader_pump),
            wait_pump: Some(wait_pump),
            session_policy,
            diagnostics: DiagnosticsSink::default(),
            ui_runtime,
            gpu_renderer: GpuRenderer::default(),
            started_at: Instant::now(),
            render_attempt_sequence: 0,
            gpu_failure_sequence: 0,
            initial_mode,
            window: None,
            window_id: None,
            _context: None,
            surface: None,
            window_size: PhysicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT),
            terminal: TerminalState::new(DEFAULT_COLS, DEFAULT_ROWS, DEFAULT_SCROLLBACK_CAP),
            settings: SettingsService::default(),
            modifiers: ModifiersState::default(),
            palette_open: false,
            redraw_pending: true,
            last_rendered_cursor_row: None,
            exit_code: None,
            fatal_error: None,
        })
    }

    fn bootstrap_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        let mut attributes = Window::default_attributes()
            .with_title("rldyourterm")
            .with_inner_size(LogicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT))
            .with_visible(true)
            .with_active(true)
            .with_window_icon(generate_app_icon());

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

        // Try GPU initialization if not CPU-only mode
        if self.initial_mode != RenderMode::Cpu {
            let w = window.inner_size().width;
            let h = window.inner_size().height;
            match self.gpu_renderer.initialize(window.clone(), w, h) {
                Ok(()) => info!("GPU backend initialized successfully"),
                Err(e) => warn!(error = ?e, "GPU init failed, will use CPU fallback"),
            }
        }

        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;

        self.window_size = window.inner_size();
        self.window_id = Some(window.id());
        self._context = Some(context);
        self.surface = Some(surface);
        self.window = Some(window);
        self.apply_startup_visibility_handshake();

        self.update_viewport_geometry(event_loop);
        self.queue_redraw();
        Ok(())
    }

    fn shutdown(&mut self) {
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

    fn apply_startup_visibility_handshake(&self) {
        if let Some(window) = self.window.as_ref() {
            window.set_visible(true);
            window.focus_window();
            window.request_redraw();
            info!("applied startup visibility handshake");
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }

    fn emit_runtime_notice(&mut self, message: &str) {
        let mut line = String::from("\r\n");
        line.push_str(message);
        line.push_str("\r\n");
        let _events = self.terminal.feed(line.as_bytes());
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
                self.palette_open = false;
                self.emit_runtime_notice(&result_line);
            }
        }

        Ok(true)
    }

    fn request_redraw_if_needed(&mut self) {
        if !self.redraw_pending {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
            self.redraw_pending = false;
        }
    }

    fn update_viewport_geometry(&mut self, event_loop: &ActiveEventLoop) {
        let cols = ((self.window_size.width as usize) / CELL_WIDTH)
            .max(1)
            .min(u16::MAX as usize) as u16;
        let rows = ((self.window_size.height as usize) / CELL_HEIGHT)
            .max(1)
            .min(u16::MAX as usize) as u16;

        self.terminal.resize(cols, rows);

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

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyResize) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn handle_close_requested(&mut self, event_loop: &ActiveEventLoop) {
        self.exit_code.get_or_insert(0);
        event_loop.exit();
    }

    fn handle_monitor_affecting_event(&mut self, monitor_event: MonitorAffectingWindowEvent) {
        let sampled_refresh_rate_millihz = self
            .window
            .as_ref()
            .and_then(|window| sample_monitor_refresh_rate_millihz(window));
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

        if let Some(bytes) = encode_winit_key_event(&event.logical_key, self.modifiers)
            && let Err(error) = write_all_and_flush(&mut *self.writer, &bytes)
        {
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

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn handle_text_commit(&mut self, text: &str, event_loop: &ActiveEventLoop) {
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
        if is_disconnect_error(&error) {
            if let Some(code) = self
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
        let text = match arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            Ok(text) if !text.is_empty() => text,
            _ => return,
        };
        // Paste size cap: 64 KB
        let text = if text.len() > 64 * 1024 {
            &text[..64 * 1024]
        } else {
            &text
        };
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

    fn draw_frame(&mut self) -> Result<()> {
        self.render_attempt_sequence = self.render_attempt_sequence.saturating_add(1);
        let render_attempt_sequence = self.render_attempt_sequence;

        if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
            && self.gpu_renderer.is_initialized()
        {
            match self.gpu_renderer.render_frame(&self.terminal) {
                Ok(()) => {
                    let _ = self
                        .ui_runtime
                        .handle_command(UiRuntimeCommand::GpuFramePresented)
                        .context("failed to dispatch UiRuntimeCommand::GpuFramePresented")?;
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
                        }
                        GpuFailureHandling::FatalForcedGpu => {
                            return Err(anyhow!(
                                "forced gpu mode render failure: kind={failure_kind:?} observed_at_millis={observed_at_millis} render_attempt_sequence={render_attempt_sequence} gpu_failure_sequence={gpu_failure_sequence}"
                            ));
                        }
                        GpuFailureHandling::Ignored => {}
                    }
                }
            }
        }

        let width = self.window_size.width;
        let height = self.window_size.height;
        if width == 0 || height == 0 {
            return Ok(());
        }

        let surface = self
            .surface
            .as_mut()
            .ok_or_else(|| anyhow!("softbuffer surface is not initialized"))?;

        let nz_width = NonZeroU32::new(width).ok_or_else(|| anyhow!("zero width is invalid"))?;
        let nz_height = NonZeroU32::new(height).ok_or_else(|| anyhow!("zero height is invalid"))?;
        surface
            .resize(nz_width, nz_height)
            .map_err(|error| anyhow!("failed to resize softbuffer surface: {error}"))?;

        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!("failed to acquire softbuffer frame: {error}"))?;
        render_terminal(
            &mut buffer,
            width as usize,
            height as usize,
            &mut self.terminal,
            self.last_rendered_cursor_row,
        );
        self.last_rendered_cursor_row = Some(self.terminal.cursor.row);
        buffer
            .present()
            .map_err(|error| anyhow!("failed to present GUI frame: {error}"))?;
        Ok(())
    }
}

fn classify_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
) -> Result<PtyBoundaryPolicyDecision> {
    let transition = session_policy
        .handle_boundary_failure(boundary)
        .map_err(|error| {
            anyhow!(
                "failed to apply PTY boundary policy boundary={}: {error}",
                session_boundary_token(boundary)
            )
        })?;

    match transition.outcome {
        SessionTransitionOutcome::RecoverableBoundary {
            attempt,
            remaining_budget,
            ..
        } => Ok(PtyBoundaryPolicyDecision::Continue {
            attempt,
            remaining_budget,
        }),
        SessionTransitionOutcome::FatalBoundary { reason, .. } => {
            Ok(PtyBoundaryPolicyDecision::Fatal { reason })
        }
        other => Err(anyhow!(
            "unexpected session transition for boundary={} outcome={other:?}",
            session_boundary_token(boundary),
        )),
    }
}

impl ApplicationHandler<GuiEvent> for GuiRuntimeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.bootstrap_window(event_loop) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: GuiEvent) {
        match event {
            GuiEvent::Output(data) => {
                let _events = self.terminal.feed(&data);
                let title = self.terminal.window_title();
                if !title.is_empty() {
                    if let Some(window) = self.window.as_ref() {
                        window.set_title(title);
                    }
                }
                if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyRead) {
                    self.fatal_error = Some(error);
                    event_loop.exit();
                    return;
                }
                self.queue_redraw();
            }
            GuiEvent::Exited(code) => {
                self.exit_code = Some(code);
                event_loop.exit();
            }
            GuiEvent::PtyFailure { boundary, message } => {
                match self.handle_pty_boundary_failure(boundary, &message) {
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
                if let Err(error) = self.draw_frame() {
                    self.fatal_error = Some(error);
                    event_loop.exit();
                }
            }
            WindowEvent::Moved(_) => {
                self.handle_monitor_affecting_event(MonitorAffectingWindowEvent::Moved);
            }
            WindowEvent::Resized(size) => {
                self.window_size = size;
                if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
                    && self.gpu_renderer.is_initialized()
                {
                    self.gpu_renderer.resize(size.width, size.height);
                }
                self.update_viewport_geometry(event_loop);
                self.handle_monitor_affecting_event(MonitorAffectingWindowEvent::Resized);
                self.queue_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    self.window_size = window.inner_size();
                    if self.gpu_renderer.is_initialized() {
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
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.request_redraw_if_needed();

        if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu
            && self.gpu_renderer.is_initialized()
        {
            // GPU: VSync drives frame pacing via PresentMode::AutoVsync.
            // Wait sleeps until the next event (PTY data, input, resize).
            // PTY proxy wakes the loop via EventLoopProxy — no busy-spin needed.
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            // CPU: software timer for frame pacing (no VSync via softbuffer).
            let cadence = self.ui_runtime.cadence();
            match cadence.frame_interval() {
                Some(interval) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + interval));
                }
                None => {
                    // No cadence available (headless, VNC, or monitor detection failed).
                    // Wait for events to avoid busy-spin.
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
        }
    }
}

fn render_terminal(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    terminal: &mut TerminalState,
    prev_cursor_row: Option<u16>,
) {
    if width == 0 || height == 0 {
        return;
    }

    let grid_rows = terminal.grid.height() as usize;
    let grid_cols = terminal.grid.width() as usize;
    let visible_rows = (height / CELL_HEIGHT).max(1).min(grid_rows);
    let visible_cols = (width / CELL_WIDTH).max(1).min(grid_cols);

    // Collect dirty rows and ensure cursor rows are included
    let mut dirty = terminal.grid.take_dirty_rows();

    // Always include current cursor row for fresh cursor rendering
    let cursor_row = terminal.cursor.row;
    if (cursor_row as usize) < visible_rows && !dirty.contains(&cursor_row) {
        dirty.push(cursor_row);
    }

    // Include previous cursor row to erase old cursor overlay
    if let Some(prev) = prev_cursor_row {
        if prev != cursor_row && (prev as usize) < visible_rows && !dirty.contains(&prev) {
            dirty.push(prev);
        }
    }

    if dirty.is_empty() {
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
                    draw_char_colored(
                        buffer,
                        width,
                        height,
                        x,
                        base_y,
                        cell.ch,
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
}

fn resolve_cell_colors(attrs: &rldyourterm_core::grid::Attrs) -> (u32, u32) {
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

fn lookup_glyph(ch: char) -> [u8; 8] {
    BASIC_FONTS
        .get(ch)
        .or_else(|| LATIN_FONTS.get(ch))
        .or_else(|| BLOCK_FONTS.get(ch))
        .or_else(|| BOX_FONTS.get(ch))
        .or_else(|| GREEK_FONTS.get(ch))
        .or_else(|| HIRAGANA_FONTS.get(ch))
        .or_else(|| MISC_FONTS.get(ch))
        .unwrap_or([0; 8])
}

#[allow(clippy::too_many_arguments)]
fn draw_char_colored(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    ch: char,
    fg: u32,
    bold: bool,
) {
    let glyph = lookup_glyph(ch);

    for (glyph_y, row_bits) in glyph.iter().enumerate() {
        for glyph_x in 0..8 {
            if (row_bits >> glyph_x) & 1 == 0 {
                continue;
            }

            let pixel_x = x + glyph_x;
            let pixel_y = y + glyph_y * 2;
            if pixel_x >= width || pixel_y >= height {
                continue;
            }

            let index = pixel_y * width + pixel_x;
            buffer[index] = fg;

            let pixel_y2 = pixel_y + 1;
            if pixel_y2 < height {
                let index2 = pixel_y2 * width + pixel_x;
                buffer[index2] = fg;
            }

            // Bold via double-strike (1px right shift)
            if bold {
                let bold_x = pixel_x + 1;
                if bold_x < width {
                    buffer[pixel_y * width + bold_x] = fg;
                    if pixel_y2 < height {
                        buffer[pixel_y2 * width + bold_x] = fg;
                    }
                }
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
        _ => format!("[palette] command-applied input={command:?}"),
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

fn render_mode_token(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Cpu => "cpu",
        RenderMode::Gpu => "gpu",
        RenderMode::Auto => "auto",
    }
}

fn active_render_path_token(path: ActiveRenderPath) -> &'static str {
    match path {
        ActiveRenderPath::Cpu => "cpu",
        ActiveRenderPath::Gpu => "gpu",
    }
}

fn on_off_token(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn sample_monitor_refresh_rate_millihz(window: &Window) -> Option<u32> {
    window
        .current_monitor()
        .and_then(|monitor| monitor.refresh_rate_millihertz())
        .or_else(|| {
            window
                .primary_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
        })
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
        .emit_kind(EventKind::ResourceWarning, diagnostics_message);
    let notice = format!(
        "[runtime] gpu auto-fallback transition-seq={transition_sequence} failure-seq={gpu_failure_sequence} render-attempt-seq={render_attempt_sequence} failure={failure_kind:?} observed-ms={observed_at_millis} correlation-id={}",
        correlation_id.as_str()
    );
    (event, notice)
}

fn session_boundary_token(boundary: SessionBoundary) -> &'static str {
    match boundary {
        SessionBoundary::StartupSpawn => "startup-spawn",
        SessionBoundary::PtyRead => "pty-read",
        SessionBoundary::PtyWrite => "pty-write",
        SessionBoundary::PtyResize => "pty-resize",
        SessionBoundary::PtyWait => "pty-wait",
        SessionBoundary::PtyWriterAcquire => "pty-writer-acquire",
        SessionBoundary::Stop => "stop",
    }
}

fn fatal_boundary_reason_token(reason: FatalBoundaryReason) -> &'static str {
    match reason {
        FatalBoundaryReason::BoundaryFatal => "boundary-fatal",
        FatalBoundaryReason::RecoverableBudgetExhausted => "recoverable-budget-exhausted",
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
    match key.as_ref() {
        Key::Named(NamedKey::Enter) => Some(vec![b'\r']),
        Key::Named(NamedKey::Tab) => Some(vec![b'\t']),
        Key::Named(NamedKey::Escape) => Some(vec![0x1b]),
        Key::Named(NamedKey::Backspace) => Some(vec![0x7f]),
        Key::Named(NamedKey::ArrowUp) => Some(b"\x1b[A".to_vec()),
        Key::Named(NamedKey::ArrowDown) => Some(b"\x1b[B".to_vec()),
        Key::Named(NamedKey::ArrowRight) => Some(b"\x1b[C".to_vec()),
        Key::Named(NamedKey::ArrowLeft) => Some(b"\x1b[D".to_vec()),
        Key::Named(NamedKey::Home) => Some(b"\x1b[H".to_vec()),
        Key::Named(NamedKey::End) => Some(b"\x1b[F".to_vec()),
        Key::Named(NamedKey::Delete) => Some(b"\x1b[3~".to_vec()),
        Key::Named(NamedKey::Insert) => Some(b"\x1b[2~".to_vec()),
        Key::Named(NamedKey::PageUp) => Some(b"\x1b[5~".to_vec()),
        Key::Named(NamedKey::PageDown) => Some(b"\x1b[6~".to_vec()),
        Key::Named(NamedKey::F1) => Some(b"\x1bOP".to_vec()),
        Key::Named(NamedKey::F2) => Some(b"\x1bOQ".to_vec()),
        Key::Named(NamedKey::F3) => Some(b"\x1bOR".to_vec()),
        Key::Named(NamedKey::F4) => Some(b"\x1bOS".to_vec()),
        Key::Named(NamedKey::F5) => Some(b"\x1b[15~".to_vec()),
        Key::Named(NamedKey::F6) => Some(b"\x1b[17~".to_vec()),
        Key::Named(NamedKey::F7) => Some(b"\x1b[18~".to_vec()),
        Key::Named(NamedKey::F8) => Some(b"\x1b[19~".to_vec()),
        Key::Named(NamedKey::F9) => Some(b"\x1b[20~".to_vec()),
        Key::Named(NamedKey::F10) => Some(b"\x1b[21~".to_vec()),
        Key::Named(NamedKey::F11) => Some(b"\x1b[23~".to_vec()),
        Key::Named(NamedKey::F12) => Some(b"\x1b[24~".to_vec()),
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

fn encode_ctrl_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) - b'a' + 1)
    } else {
        None
    }
}

fn write_all_and_flush(writer: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    writer.flush()
}

fn is_disconnect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
    )
}

/// Generates a 32x32 RGBA programmatic icon: dark background with a cyan terminal cursor.
fn generate_app_icon() -> Option<Icon> {
    const SIZE: u32 = 32;
    const BG: [u8; 4] = [0x14, 0x1b, 0x1f, 0xFF];
    const CURSOR: [u8; 4] = [0x00, 0xd4, 0xaa, 0xFF];
    const GLOW: [u8; 4] = [0x00, 0x6a, 0x55, 0x60];

    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let offset = ((y * SIZE + x) * 4) as usize;
            // Cursor block: centered vertical rectangle (cols 12-19, rows 6-25)
            let is_cursor = (12..=19).contains(&x) && (6..=25).contains(&y);
            // Glow: 1px border around cursor
            let is_glow = !is_cursor && (11..=20).contains(&x) && (5..=26).contains(&y);

            let color = if is_cursor {
                CURSOR
            } else if is_glow {
                GLOW
            } else {
                BG
            };
            rgba[offset..offset + 4].copy_from_slice(&color);
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_FG, DEFAULT_FG_U32, GpuFailureHandling, MonitorAffectingWindowEvent,
        PtyBoundaryPolicyDecision, cadence_resync_command_for_monitor_event,
        classify_pty_boundary_failure, dispatch_gpu_failure_command,
        dispatch_runtime_palette_command, emit_gpu_auto_fallback_observability, encode_ctrl_letter,
        encode_winit_key_event, grid, is_runtime_palette_shortcut_key, resolve_cell_colors,
    };
    use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
    use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
    use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
    use rldyourterm_settings::SettingsService;
    use rldyourterm_ui::{UiBootstrapConfig, UiRuntime, UiRuntimeCommand};
    use winit::keyboard::{Key, ModifiersState};

    #[test]
    fn ctrl_letter_encoding_matches_ascii_control_range() {
        assert_eq!(encode_ctrl_letter('a'), Some(0x01));
        assert_eq!(encode_ctrl_letter('c'), Some(0x03));
        assert_eq!(encode_ctrl_letter('z'), Some(0x1a));
        assert_eq!(encode_ctrl_letter('1'), None);
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

        assert_eq!(event.kind, EventKind::ResourceWarning);
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
        let default_fg = grid::color_to_u32(rldyourterm_core::grid::Color::Default, DEFAULT_FG);
        assert_eq!(default_fg, DEFAULT_FG_U32);
    }

    #[test]
    fn color_to_u32_indexed_looks_up_palette() {
        let red = grid::color_to_u32(rldyourterm_core::grid::Color::Indexed(1), DEFAULT_FG);
        assert_eq!(red, rldyourterm_core::grid::ANSI_PALETTE[1]);
    }

    #[test]
    fn color_to_u32_rgb_constructs_correctly() {
        let c = grid::color_to_u32(
            rldyourterm_core::grid::Color::Rgb(0xFF, 0x80, 0x00),
            DEFAULT_FG,
        );
        assert_eq!(c, 0x00FF_8000);
    }

    #[test]
    fn resolve_cell_colors_inverse_swaps_fg_bg() {
        let attrs = rldyourterm_core::grid::Attrs {
            fg: rldyourterm_core::grid::Color::Indexed(1),
            bg: rldyourterm_core::grid::Color::Indexed(2),
            inverse: true,
            ..rldyourterm_core::grid::Attrs::default()
        };
        let (fg, bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, rldyourterm_core::grid::ANSI_PALETTE[2]);
        assert_eq!(bg, rldyourterm_core::grid::ANSI_PALETTE[1]);
    }

    #[test]
    fn resolve_cell_colors_dim_halves_fg() {
        let attrs = rldyourterm_core::grid::Attrs {
            fg: rldyourterm_core::grid::Color::Rgb(200, 100, 50),
            dim: true,
            ..rldyourterm_core::grid::Attrs::default()
        };
        let (fg, _bg) = resolve_cell_colors(&attrs);
        assert_eq!(fg, super::rgb_to_u32(100, 50, 25));
    }

    #[test]
    fn encode_f_keys() {
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
    }
}
