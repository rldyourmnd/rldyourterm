use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use font8x8::{BASIC_FONTS, UnicodeFonts};
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
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::window::{Window, WindowId};

const DEFAULT_GUI_WIDTH: u32 = 1280;
const DEFAULT_GUI_HEIGHT: u32 = 800;
const DEFAULT_COLS: u16 = 120;
const DEFAULT_ROWS: u16 = 32;
const MAX_SCROLLBACK_LINES: usize = 50_000;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
#[cfg(target_os = "macos")]
const MACOS_FORCE_FOCUS_ENV: &str = "RLDYOURTERM_GUI_FORCE_FOCUS";

const CELL_WIDTH: usize = 8;
const CELL_HEIGHT: usize = 16;
const TAB_WIDTH: usize = 4;

const COLOR_BG: u32 = 0x0014_1b1f;
const COLOR_FG: u32 = 0x00d8_d8d8;
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
    ui_runtime: UiRuntime,
    gpu_renderer: GpuRenderer,
    started_at: Instant,
    render_attempt_sequence: u64,
    gpu_failure_sequence: u64,
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,

    window: Option<Rc<Window>>,
    window_id: Option<WindowId>,
    _context: Option<SoftbufferContext<Rc<Window>>>,
    surface: Option<SoftbufferSurface<Rc<Window>, Rc<Window>>>,
    window_size: PhysicalSize<u32>,
    terminal: TerminalBuffer,
    settings: SettingsService,
    modifiers: ModifiersState,
    palette_open: bool,
    redraw_pending: bool,

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
            scrollback_cap: MAX_SCROLLBACK_LINES,
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
            ui_runtime,
            gpu_renderer: GpuRenderer::default(),
            started_at: Instant::now(),
            render_attempt_sequence: 0,
            gpu_failure_sequence: 0,
            initial_mode,
            refresh_rate_millihz,
            window: None,
            window_id: None,
            _context: None,
            surface: None,
            window_size: PhysicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT),
            terminal: TerminalBuffer::new(MAX_SCROLLBACK_LINES),
            settings: SettingsService::default(),
            modifiers: ModifiersState::default(),
            palette_open: false,
            redraw_pending: true,
            exit_code: None,
            fatal_error: None,
        })
    }

    fn bootstrap_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }

        let title = format!(
            "rldyourterm GUI MVP [{:?}] {} mHz",
            self.initial_mode, self.refresh_rate_millihz
        );

        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(DEFAULT_GUI_WIDTH, DEFAULT_GUI_HEIGHT));
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .context("failed to create GUI window")?,
        );

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
        #[cfg(target_os = "macos")]
        if let Some(window) = self.window.as_ref() {
            let force_focus = std::env::var(MACOS_FORCE_FOCUS_ENV)
                .ok()
                .map(|value| {
                    let normalized = value.trim().to_ascii_lowercase();
                    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
                })
                .unwrap_or(false);

            window.set_visible(true);
            if force_focus {
                window.focus_window();
            }

            info!(
                env_flag = MACOS_FORCE_FOCUS_ENV,
                force_focus, "applied macOS startup visibility handshake"
            );
        }
    }

    fn queue_redraw(&mut self) {
        self.redraw_pending = true;
    }

    fn emit_runtime_notice(&mut self, message: &str) {
        let mut line = String::from("\r\n");
        line.push_str(message);
        line.push_str("\r\n");
        self.terminal.push_bytes(line.as_bytes());
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

        self.terminal.set_columns(cols as usize);

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

        if let Some(bytes) = encode_winit_key_event(event, self.modifiers)
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
            match self
                .pty
                .try_wait()
                .context("failed to poll PTY after disconnecting GUI I/O failure")?
            {
                Some(code) => {
                    self.exit_code = Some(code);
                    info!(
                        boundary = session_boundary_token(boundary),
                        code, "PTY child already exited after disconnecting GUI I/O failure"
                    );
                    return Ok(PtyBoundaryLoopAction::ExitLoop);
                }
                None => {}
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

    fn draw_frame(&mut self) -> Result<()> {
        self.render_attempt_sequence = self.render_attempt_sequence.saturating_add(1);
        let render_attempt_sequence = self.render_attempt_sequence;

        if self.ui_runtime.active_render_path() == ActiveRenderPath::Gpu {
            match self.gpu_renderer.render() {
                Ok(()) => {
                    let _ = self
                        .ui_runtime
                        .handle_command(UiRuntimeCommand::GpuFramePresented)
                        .context("failed to dispatch UiRuntimeCommand::GpuFramePresented")?;
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
                            warn!(
                                gpu_failure_sequence,
                                render_attempt_sequence,
                                transition_sequence,
                                mode = ?self.ui_runtime.render_mode(),
                                active_path = ?self.ui_runtime.active_render_path(),
                                "gpu failure applied deterministic cpu fallback; session remains active"
                            );
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
        render_terminal(&mut buffer, width as usize, height as usize, &self.terminal);
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
                self.terminal.push_bytes(&data);
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
            WindowEvent::Resized(size) => {
                self.window_size = size;
                self.update_viewport_geometry(event_loop);
                self.queue_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_ref() {
                    self.window_size = window.inner_size();
                    self.update_viewport_geometry(event_loop);
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.request_redraw_if_needed();
    }
}

fn render_terminal(buffer: &mut [u32], width: usize, height: usize, terminal: &TerminalBuffer) {
    buffer.fill(COLOR_BG);

    if width == 0 || height == 0 {
        return;
    }

    let rows = (height / CELL_HEIGHT).max(1);
    let cols = (width / CELL_WIDTH).max(1);
    let visible_line_count = terminal.visible_line_count(rows);
    let top_row = rows.saturating_sub(visible_line_count);

    for (row_offset, line) in terminal.visible_lines(rows).enumerate() {
        draw_line(buffer, width, height, top_row + row_offset, cols, line);
    }
}

fn draw_line(buffer: &mut [u32], width: usize, height: usize, row: usize, cols: usize, text: &str) {
    let base_y = row * CELL_HEIGHT;
    if base_y >= height {
        return;
    }

    for (col, ch) in text.chars().take(cols).enumerate() {
        draw_char(buffer, width, height, col * CELL_WIDTH, base_y, ch);
    }
}

fn draw_char(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, ch: char) {
    let glyph = BASIC_FONTS
        .get(ch)
        .or_else(|| BASIC_FONTS.get('?'))
        .unwrap_or([0; 8]);

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
            buffer[index] = COLOR_FG;

            let pixel_y2 = pixel_y + 1;
            if pixel_y2 < height {
                let index2 = pixel_y2 * width + pixel_x;
                buffer[index2] = COLOR_FG;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    None,
    Esc,
    Csi,
}

#[derive(Debug)]
struct TerminalBuffer {
    lines: VecDeque<String>,
    max_lines: usize,
    cols: usize,
    escape_state: EscapeState,
}

impl TerminalBuffer {
    fn new(max_lines: usize) -> Self {
        let mut lines = VecDeque::new();
        lines.push_back(String::new());

        Self {
            lines,
            max_lines: max_lines.max(1),
            cols: DEFAULT_COLS as usize,
            escape_state: EscapeState::None,
        }
    }

    fn set_columns(&mut self, cols: usize) {
        self.cols = cols.max(1);
    }

    fn visible_line_count(&self, rows: usize) -> usize {
        rows.max(1).min(self.lines.len())
    }

    fn visible_lines(&self, rows: usize) -> impl Iterator<Item = &str> + '_ {
        let count = self.visible_line_count(rows);
        let start = self.lines.len().saturating_sub(count);
        self.lines.iter().skip(start).map(String::as_str)
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.consume_escape(byte) {
                continue;
            }

            match byte {
                b'\x1b' => self.escape_state = EscapeState::Esc,
                b'\r' => {
                    self.current_line_mut().clear();
                }
                b'\n' => self.new_line(),
                0x08 | 0x7f => {
                    self.current_line_mut().pop();
                }
                b'\t' => {
                    for _ in 0..TAB_WIDTH {
                        self.push_char(' ');
                    }
                }
                0x20..=0x7e => self.push_char(byte as char),
                _ if byte >= 0x80 => self.push_char('�'),
                _ => {}
            }
        }
    }

    fn consume_escape(&mut self, byte: u8) -> bool {
        match self.escape_state {
            EscapeState::None => false,
            EscapeState::Esc => {
                self.escape_state = if byte == b'[' {
                    EscapeState::Csi
                } else {
                    EscapeState::None
                };
                true
            }
            EscapeState::Csi => {
                if (0x40..=0x7e).contains(&byte) {
                    self.escape_state = EscapeState::None;
                }
                true
            }
        }
    }

    fn push_char(&mut self, ch: char) {
        let line = self.current_line_mut();
        line.push(ch);
        if line.chars().count() >= self.cols {
            self.new_line();
        }
    }

    fn new_line(&mut self) {
        self.lines.push_back(String::new());
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    fn current_line_mut(&mut self) -> &mut String {
        if self.lines.is_empty() {
            self.lines.push_back(String::new());
        }
        self.lines
            .back_mut()
            .expect("terminal buffer must contain current line")
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
        Key::Character(text) if text == "1" => Some(RuntimePaletteAction::ApplyCommand("mode cpu")),
        Key::Character(text) if text == "2" => Some(RuntimePaletteAction::ApplyCommand("mode gpu")),
        Key::Character(text) if text == "3" => {
            Some(RuntimePaletteAction::ApplyCommand("mode auto"))
        }
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

fn encode_winit_key_event(event: &WinitKeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
    match event.logical_key.as_ref() {
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

#[cfg(test)]
mod tests {
    use super::{
        GpuFailureHandling, PtyBoundaryPolicyDecision, TerminalBuffer,
        classify_pty_boundary_failure, dispatch_gpu_failure_command,
        dispatch_runtime_palette_command, encode_ctrl_letter, is_runtime_palette_shortcut_key,
    };
    use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
    use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
    use rldyourterm_settings::SettingsService;
    use rldyourterm_ui::{UiBootstrapConfig, UiRuntime};
    use winit::keyboard::{Key, ModifiersState};

    #[test]
    fn terminal_buffer_keeps_recent_lines() {
        let mut buffer = TerminalBuffer::new(3);
        buffer.set_columns(80);
        buffer.push_bytes(b"one\ntwo\nthree\nfour\n");

        let lines: Vec<&str> = buffer.visible_lines(3).collect();
        assert_eq!(lines, ["three", "four", ""]);
    }

    #[test]
    fn terminal_buffer_strips_ansi_sequences() {
        let mut buffer = TerminalBuffer::new(10);
        buffer.set_columns(80);
        buffer.push_bytes(b"\x1b[31mred\x1b[0m\n");

        let lines: Vec<&str> = buffer.visible_lines(2).collect();
        assert_eq!(lines, ["red", ""]);
    }

    #[test]
    fn terminal_buffer_clamps_zero_max_lines() {
        let mut buffer = TerminalBuffer::new(0);
        buffer.set_columns(80);
        buffer.push_bytes(b"one\ntwo\n");

        let lines: Vec<&str> = buffer.visible_lines(8).collect();
        assert_eq!(lines, [""]);
    }

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
}
