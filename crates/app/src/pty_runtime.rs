use std::io::{self, ErrorKind, Read, Write};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::shared::{
    PtyBoundaryPolicyDecision, classify_pty_boundary_failure, csi_modified, encode_ctrl_letter,
    fatal_boundary_reason_token, fkey_ss3_modified, is_disconnect_error, on_off_token,
    render_mode_token, session_boundary_token, tilde_modified, write_all_and_flush,
    xterm_modifier_param,
};
use anyhow::{Context, Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{SessionBoundary, SessionController, SessionState};
use rldyourterm_settings::{SettingsCommand, SettingsPaletteApplyOutcome, SettingsService};
use rldyourterm_ui::SINGLE_WINDOW_BASELINE;
use tracing::{info, warn};

const DEFAULT_REFRESH_RATE_MILLIHZ: u32 = 60_000;
const MIN_EVENT_POLL_TIMEOUT_MILLIS: u64 = 1;
const MAX_EVENT_POLL_TIMEOUT_MILLIS: u64 = 200;
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const SHUTDOWN_JOIN_TIMEOUT: Duration = Duration::from_millis(750);
const SHUTDOWN_JOIN_POLL_INTERVAL: Duration = Duration::from_millis(10);
const RUNTIME_PALETTE_HELP_LINE: &str =
    "[palette] 1:mode cpu 2:mode gpu 3:mode auto d:diagnostics toggle i:info Esc:close";
const RUNTIME_PALETTE_CLOSED_LINE: &str = "[palette] closed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimePaletteAction {
    ApplyCommand(&'static str),
    ShowInfo,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimePaletteDispatchResult {
    message: String,
    updated_mode: Option<RenderMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinThreadOutcome {
    Joined,
    Panicked,
    TimedOut,
}

#[derive(Debug, Clone, Copy)]
pub struct TtyRuntimeConfig {
    pub initial_mode: RenderMode,
    pub refresh_rate_millihz: u32,
    pub window_count: u8,
}

#[derive(Debug, Clone, Copy)]
struct EventPollController {
    min_timeout: Duration,
    max_timeout: Duration,
    next_timeout: Duration,
}

impl EventPollController {
    fn from_config(config: TtyRuntimeConfig) -> Self {
        let (min_timeout, max_timeout) =
            derive_poll_timeouts(config.initial_mode, config.refresh_rate_millihz);
        Self {
            min_timeout,
            max_timeout,
            next_timeout: min_timeout,
        }
    }

    fn next_timeout(&self) -> Duration {
        self.next_timeout
    }

    fn on_terminal_event(&mut self) {
        self.next_timeout = self.min_timeout;
    }

    fn on_idle_poll(&mut self) {
        self.next_timeout = self
            .next_timeout
            .checked_mul(2)
            .unwrap_or(self.max_timeout)
            .min(self.max_timeout);
    }

    fn bounds_millis(&self) -> (u128, u128) {
        (self.min_timeout.as_millis(), self.max_timeout.as_millis())
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable terminal raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub fn run_interactive_pty(
    shell_executable: &str,
    shell_args: &[String],
    runtime_config: TtyRuntimeConfig,
) -> Result<i32> {
    ensure_single_window(runtime_config.window_count)?;

    let mut poll_controller = EventPollController::from_config(runtime_config);
    let (poll_timeout_min_ms, poll_timeout_max_ms) = poll_controller.bounds_millis();
    info!(
        mode = ?runtime_config.initial_mode,
        refresh_rate_millihz = runtime_config.refresh_rate_millihz,
        windows = runtime_config.window_count,
        single_window_required = SINGLE_WINDOW_BASELINE,
        single_window_enforced = true,
        poll_timeout_min_ms,
        poll_timeout_max_ms,
        "starting TTY runtime"
    );

    let initial_size = current_pty_size();
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: Vec::new(),
        size: initial_size,
    };

    let factory = PlatformPtyFactory;
    let pty = factory
        .spawn(spawn_config)
        .context("failed to spawn interactive PTY")?;
    let reader = pty.take_reader().context("failed to acquire PTY reader")?;
    let mut writer = pty.take_writer().context("failed to acquire PTY writer")?;

    let _raw_mode_guard = RawModeGuard::new()?;
    let read_pump = spawn_read_pump(reader);
    let mut session_policy = SessionController::new();
    session_policy
        .mark_running()
        .context("failed to initialize TTY session boundary policy")?;
    let mut settings = SettingsService::default();
    let _ = settings.apply(SettingsCommand::SetMode(runtime_config.initial_mode));
    let mut active_mode = settings.state().mode;
    let mut palette_open = false;

    let mut exit_code: Option<i32> = None;
    let mut requested_local_exit = false;
    let mut fatal_error: Option<anyhow::Error> = None;

    loop {
        match pty.try_wait().context("failed to poll PTY child status") {
            Ok(Some(code)) => {
                exit_code = Some(code);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                fatal_error = Some(error);
                break;
            }
        }

        let has_event = match event::poll(poll_controller.next_timeout()) {
            Ok(value) => value,
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                continue;
            }
            Err(error) if is_disconnect_error(&error) => {
                match pty
                    .try_wait()
                    .context("failed to poll PTY after terminal event poll disconnect")
                {
                    Ok(Some(code)) => exit_code = Some(code),
                    Ok(None) => {}
                    Err(wait_error) => fatal_error = Some(wait_error),
                }
                break;
            }
            Err(error) => {
                fatal_error =
                    Some(anyhow::Error::new(error).context("failed to poll terminal events"));
                break;
            }
        };
        if !has_event {
            poll_controller.on_idle_poll();
            continue;
        }

        let terminal_event = match event::read() {
            Ok(value) => value,
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                continue;
            }
            Err(error) if is_disconnect_error(&error) => {
                match pty
                    .try_wait()
                    .context("failed to poll PTY after terminal event read disconnect")
                {
                    Ok(Some(code)) => exit_code = Some(code),
                    Ok(None) => {}
                    Err(wait_error) => fatal_error = Some(wait_error),
                }
                break;
            }
            Err(error) => {
                fatal_error =
                    Some(anyhow::Error::new(error).context("failed to read terminal event"));
                break;
            }
        };
        poll_controller.on_terminal_event();

        match terminal_event {
            Event::Key(key_event) if is_press_like(key_event.kind) => {
                if is_local_shutdown_key(key_event) {
                    requested_local_exit = true;
                    break;
                }

                if is_runtime_palette_shortcut(key_event) {
                    palette_open = !palette_open;
                    if palette_open {
                        write_runtime_palette_line(RUNTIME_PALETTE_HELP_LINE);
                    } else {
                        write_runtime_palette_line(RUNTIME_PALETTE_CLOSED_LINE);
                    }
                    continue;
                }

                if palette_open {
                    if let Some(action) =
                        runtime_palette_action_for_key_event(key_event, settings.state().debug_mode)
                    {
                        match action {
                            RuntimePaletteAction::Close => {
                                palette_open = false;
                                write_runtime_palette_line(RUNTIME_PALETTE_CLOSED_LINE);
                            }
                            RuntimePaletteAction::ShowInfo => {
                                let info_line = runtime_palette_info_line(&settings, active_mode);
                                write_runtime_palette_line(&info_line);
                            }
                            RuntimePaletteAction::ApplyCommand(input) => {
                                let dispatch =
                                    dispatch_runtime_palette_command(&mut settings, input);
                                if let Some(updated_mode) = dispatch.updated_mode {
                                    active_mode = updated_mode;
                                    poll_controller =
                                        EventPollController::from_config(TtyRuntimeConfig {
                                            initial_mode: active_mode,
                                            refresh_rate_millihz: runtime_config
                                                .refresh_rate_millihz,
                                            window_count: runtime_config.window_count,
                                        });
                                }
                                palette_open = false;
                                write_runtime_palette_line(&dispatch.message);
                            }
                        }
                    }
                    continue;
                }

                if let Some(bytes) = encode_key_event(key_event) {
                    if let Err(error) = write_all_and_flush(&mut *writer, &bytes) {
                        match handle_pty_io_failure(
                            &mut session_policy,
                            &*pty,
                            SessionBoundary::PtyWrite,
                            error,
                            "failed to write key event to PTY",
                        ) {
                            Ok(Some(code)) => {
                                exit_code = Some(code);
                                break;
                            }
                            Ok(None) => continue,
                            Err(policy_error) => {
                                fatal_error = Some(policy_error);
                                break;
                            }
                        }
                    }

                    if let Err(error) =
                        mark_pty_boundary_recovered(&mut session_policy, SessionBoundary::PtyWrite)
                    {
                        fatal_error = Some(error);
                        break;
                    }
                }
            }
            Event::Resize(cols, rows) => {
                let resized = PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                };
                if let Err(error) = pty.resize(resized) {
                    let detail = format!("failed to resize PTY: {error}");
                    if let Err(policy_error) = handle_pty_boundary_failure(
                        &mut session_policy,
                        SessionBoundary::PtyResize,
                        &detail,
                    ) {
                        fatal_error = Some(policy_error);
                        break;
                    }
                    continue;
                }

                if let Err(error) =
                    mark_pty_boundary_recovered(&mut session_policy, SessionBoundary::PtyResize)
                {
                    fatal_error = Some(error);
                    break;
                }
            }
            _ => {}
        }
    }

    if requested_local_exit {
        exit_code.get_or_insert(0);
    } else if exit_code.is_none() && fatal_error.is_none() {
        match pty
            .wait()
            .context("failed while waiting for PTY child exit")
        {
            Ok(code) => exit_code = Some(code),
            Err(error) => fatal_error = Some(error),
        }
    }

    if let Err(error) = pty.close().context("failed to close PTY session")
        && fatal_error.is_none()
    {
        fatal_error = Some(error);
    }

    match join_thread_with_timeout(
        read_pump,
        SHUTDOWN_JOIN_TIMEOUT,
        SHUTDOWN_JOIN_POLL_INTERVAL,
        "read_pump",
    ) {
        JoinThreadOutcome::Joined => {}
        JoinThreadOutcome::Panicked => {}
        JoinThreadOutcome::TimedOut => {
            warn!(
                timeout_ms = SHUTDOWN_JOIN_TIMEOUT.as_millis(),
                "PTY read pump join timed out; detaching thread to avoid shutdown hang"
            );
        }
    }

    if let Some(error) = fatal_error {
        return Err(error);
    }
    Ok(exit_code.unwrap_or(0))
}

fn handle_pty_io_failure(
    session_policy: &mut SessionController,
    pty: &dyn PtyIo,
    boundary: SessionBoundary,
    error: io::Error,
    error_context: &'static str,
) -> Result<Option<i32>> {
    if is_disconnect_error(&error)
        && let Some(code) = pty
            .try_wait()
            .context("failed to poll PTY after disconnecting I/O failure")?
    {
        info!(
            boundary = session_boundary_token(boundary),
            code, "PTY child already exited after disconnecting I/O failure"
        );
        return Ok(Some(code));
    }

    let detail = format!("{error_context}: {error}");
    handle_pty_boundary_failure(session_policy, boundary, &detail).map(|_| None)
}

fn handle_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
    detail: &str,
) -> Result<()> {
    match classify_pty_boundary_failure(session_policy, boundary)? {
        PtyBoundaryPolicyDecision::Continue {
            attempt,
            remaining_budget,
        } => {
            warn!(
                boundary = session_boundary_token(boundary),
                attempt,
                remaining_budget,
                state = session_policy.state().as_str(),
                detail,
                "recoverable PTY boundary failure in TTY runtime; continuing in degraded mode"
            );
            write_runtime_boundary_line(boundary, attempt, remaining_budget, detail);
            Ok(())
        }
        PtyBoundaryPolicyDecision::Fatal { reason } => Err(anyhow!(
            "fatal PTY boundary failure boundary={} reason={} detail={detail}",
            session_boundary_token(boundary),
            fatal_boundary_reason_token(reason),
        )),
    }
}

fn mark_pty_boundary_recovered(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
) -> Result<()> {
    if session_policy.state() != SessionState::Degraded {
        return Ok(());
    }

    let transition = session_policy.mark_running().map_err(|error| {
        anyhow!(
            "failed to mark PTY boundary recovery boundary={}: {error}",
            session_boundary_token(boundary),
        )
    })?;

    info!(
        boundary = session_boundary_token(boundary),
        from = transition.from.as_str(),
        to = transition.to.as_str(),
        "PTY boundary recovered; TTY runtime returned to running state"
    );
    write_runtime_boundary_recovered_line(boundary);
    Ok(())
}

fn ensure_single_window(window_count: u8) -> Result<()> {
    if window_count != SINGLE_WINDOW_BASELINE {
        return Err(anyhow!(
            "tty runtime requires single-window mode; required_window_count={SINGLE_WINDOW_BASELINE} requested_window_count={window_count}"
        ));
    }
    Ok(())
}

fn derive_poll_timeouts(
    initial_mode: RenderMode,
    refresh_rate_millihz: u32,
) -> (Duration, Duration) {
    let frame_budget_millis = frame_budget_millis(refresh_rate_millihz);
    let min_timeout_millis = frame_budget_millis
        .saturating_div(2)
        .clamp(MIN_EVENT_POLL_TIMEOUT_MILLIS, 16);

    let mode_multiplier: u64 = match initial_mode {
        RenderMode::Cpu => 12,
        RenderMode::Gpu => 8,
        RenderMode::Auto => 10,
    };

    let max_timeout_millis = frame_budget_millis
        .saturating_mul(mode_multiplier)
        .clamp(min_timeout_millis, MAX_EVENT_POLL_TIMEOUT_MILLIS);

    (
        Duration::from_millis(min_timeout_millis),
        Duration::from_millis(max_timeout_millis),
    )
}

fn frame_budget_millis(refresh_rate_millihz: u32) -> u64 {
    let sanitized_refresh_rate = match refresh_rate_millihz {
        0 => DEFAULT_REFRESH_RATE_MILLIHZ,
        value => value,
    };

    let frame_nanos = 1_000_000_000_000_u64 / u64::from(sanitized_refresh_rate);
    let rounded_up_millis = frame_nanos.div_ceil(1_000_000);
    rounded_up_millis.max(1)
}

fn current_pty_size() -> PtySize {
    match terminal::size() {
        Ok((cols, rows)) => PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        },
        Err(error) => {
            warn!(?error, "failed to read terminal size; using 80x24 default");
            PtySize {
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
                pixel_width: 0,
                pixel_height: 0,
            }
        }
    }
}

fn spawn_read_pump(mut reader: Box<dyn Read + Send>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut buffer = [0_u8; 65536];

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read_bytes) => {
                    if stdout.write_all(&buffer[..read_bytes]).is_err() {
                        break;
                    }
                    if stdout.flush().is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => {
                    eprintln!("PTY read error: {error}");
                    break;
                }
            }
        }
    })
}

fn join_thread_with_timeout(
    handle: JoinHandle<()>,
    timeout: Duration,
    poll_interval: Duration,
    thread_label: &'static str,
) -> JoinThreadOutcome {
    let poll_interval = poll_interval.max(Duration::from_millis(1));
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(poll_interval);
    }

    if handle.is_finished() {
        return match handle.join() {
            Ok(()) => JoinThreadOutcome::Joined,
            Err(join_error) => {
                warn!(?join_error, thread_label, "TTY shutdown thread join failed");
                JoinThreadOutcome::Panicked
            }
        };
    }

    // One final immediate check reduces false timeout logs in the race window
    // where the worker finishes right after the bounded polling loop exits.
    if handle.is_finished() {
        return match handle.join() {
            Ok(()) => JoinThreadOutcome::Joined,
            Err(join_error) => {
                warn!(?join_error, thread_label, "TTY shutdown thread join failed");
                JoinThreadOutcome::Panicked
            }
        };
    }

    JoinThreadOutcome::TimedOut
}

fn is_press_like(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn write_runtime_palette_line(line: &str) {
    let mut stdout = io::stdout();
    let mut payload = String::from("\r\n");
    payload.push_str(line);
    payload.push_str("\r\n");
    if let Err(error) = write_all_and_flush(&mut stdout, payload.as_bytes()) {
        warn!(error = %error, "failed to write runtime palette output");
    }
}

fn write_runtime_boundary_line(
    boundary: SessionBoundary,
    attempt: u8,
    remaining_budget: u8,
    detail: &str,
) {
    write_runtime_palette_line(&format!(
        "[runtime] recoverable pty-boundary={} attempt={} remaining-budget={} detail={detail}",
        session_boundary_token(boundary),
        attempt,
        remaining_budget,
    ));
}

fn write_runtime_boundary_recovered_line(boundary: SessionBoundary) {
    write_runtime_palette_line(&format!(
        "[runtime] recovered pty-boundary={}",
        session_boundary_token(boundary)
    ));
}

fn is_runtime_palette_shortcut(key_event: KeyEvent) -> bool {
    if !key_event.modifiers.contains(KeyModifiers::SHIFT) {
        return false;
    }
    if !(key_event.modifiers.contains(KeyModifiers::CONTROL)
        || key_event.modifiers.contains(KeyModifiers::SUPER))
    {
        return false;
    }

    matches!(key_event.code, KeyCode::Char('p') | KeyCode::Char('P'))
}

fn runtime_palette_action_for_key_event(
    key_event: KeyEvent,
    diagnostics_enabled: bool,
) -> Option<RuntimePaletteAction> {
    match key_event.code {
        KeyCode::Esc => Some(RuntimePaletteAction::Close),
        KeyCode::Char('1') => Some(RuntimePaletteAction::ApplyCommand("mode cpu")),
        KeyCode::Char('2') => Some(RuntimePaletteAction::ApplyCommand("mode gpu")),
        KeyCode::Char('3') => Some(RuntimePaletteAction::ApplyCommand("mode auto")),
        KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'d') => {
            if diagnostics_enabled {
                Some(RuntimePaletteAction::ApplyCommand("debug off"))
            } else {
                Some(RuntimePaletteAction::ApplyCommand("debug on"))
            }
        }
        KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'i') => Some(RuntimePaletteAction::ShowInfo),
        _ => None,
    }
}

fn dispatch_runtime_palette_command(
    settings: &mut SettingsService,
    input: &str,
) -> RuntimePaletteDispatchResult {
    match settings.apply_palette_command(input) {
        SettingsPaletteApplyOutcome::Applied {
            command, current, ..
        } => runtime_palette_dispatch_result(command, current.mode, current.debug_mode),
        SettingsPaletteApplyOutcome::Noop { command, state, .. } => {
            runtime_palette_dispatch_result(command, state.mode, state.debug_mode)
        }
        SettingsPaletteApplyOutcome::Rejected { reason, .. } => {
            warn!(?reason, input = input, "runtime palette command rejected");
            RuntimePaletteDispatchResult {
                message: format!("[palette] rejected input={input} reason={reason:?}"),
                updated_mode: None,
            }
        }
    }
}

fn runtime_palette_dispatch_result(
    command: SettingsCommand,
    mode: RenderMode,
    diagnostics_enabled: bool,
) -> RuntimePaletteDispatchResult {
    match command {
        SettingsCommand::SetMode(_) => RuntimePaletteDispatchResult {
            message: format!(
                "[palette] mode={} diagnostics={}",
                render_mode_token(mode),
                on_off_token(diagnostics_enabled),
            ),
            updated_mode: Some(mode),
        },
        SettingsCommand::SetDebugMode(_) => RuntimePaletteDispatchResult {
            message: format!(
                "[palette] diagnostics={} mode={}",
                on_off_token(diagnostics_enabled),
                render_mode_token(mode),
            ),
            updated_mode: None,
        },
        SettingsCommand::SetShellTarget(_)
        | SettingsCommand::SetShellAutoInit(_)
        | SettingsCommand::SetRenderCadencePolicy(_)
        | SettingsCommand::SetTheme(_)
        | SettingsCommand::SetRuntimeProfile(_) => RuntimePaletteDispatchResult {
            message: format!("[palette] saved (restart required) input={command:?}"),
            updated_mode: None,
        },
    }
}

fn runtime_palette_info_line(settings: &SettingsService, active_mode: RenderMode) -> String {
    format!(
        "[palette] info mode={} diagnostics={}",
        render_mode_token(active_mode),
        on_off_token(settings.state().debug_mode),
    )
}

fn is_local_shutdown_key(key_event: KeyEvent) -> bool {
    key_event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key_event.code, KeyCode::Char('q') | KeyCode::Char('Q'))
}

fn encode_key_event(key_event: KeyEvent) -> Option<Vec<u8>> {
    let mods = key_event.modifiers;
    let mod_param = xterm_modifier_param(
        mods.contains(KeyModifiers::SHIFT),
        mods.contains(KeyModifiers::ALT),
        mods.contains(KeyModifiers::CONTROL),
    );
    let has_mod = mod_param > 1;

    match key_event.code {
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace if mods.contains(KeyModifiers::ALT) => Some(b"\x1b\x7f".to_vec()),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(csi_modified(b'A', mod_param, has_mod)),
        KeyCode::Down => Some(csi_modified(b'B', mod_param, has_mod)),
        KeyCode::Right => Some(csi_modified(b'C', mod_param, has_mod)),
        KeyCode::Left => Some(csi_modified(b'D', mod_param, has_mod)),
        KeyCode::Home => Some(csi_modified(b'H', mod_param, has_mod)),
        KeyCode::End => Some(csi_modified(b'F', mod_param, has_mod)),
        KeyCode::Delete => Some(tilde_modified(3, mod_param, has_mod)),
        KeyCode::Insert => Some(tilde_modified(2, mod_param, has_mod)),
        KeyCode::PageUp => Some(tilde_modified(5, mod_param, has_mod)),
        KeyCode::PageDown => Some(tilde_modified(6, mod_param, has_mod)),
        KeyCode::F(1) => Some(fkey_ss3_modified(b'P', mod_param, has_mod)),
        KeyCode::F(2) => Some(fkey_ss3_modified(b'Q', mod_param, has_mod)),
        KeyCode::F(3) => Some(fkey_ss3_modified(b'R', mod_param, has_mod)),
        KeyCode::F(4) => Some(fkey_ss3_modified(b'S', mod_param, has_mod)),
        KeyCode::F(5) => Some(tilde_modified(15, mod_param, has_mod)),
        KeyCode::F(6) => Some(tilde_modified(17, mod_param, has_mod)),
        KeyCode::F(7) => Some(tilde_modified(18, mod_param, has_mod)),
        KeyCode::F(8) => Some(tilde_modified(19, mod_param, has_mod)),
        KeyCode::F(9) => Some(tilde_modified(20, mod_param, has_mod)),
        KeyCode::F(10) => Some(tilde_modified(21, mod_param, has_mod)),
        KeyCode::F(11) => Some(tilde_modified(23, mod_param, has_mod)),
        KeyCode::F(12) => Some(tilde_modified(24, mod_param, has_mod)),
        KeyCode::Char(ch) if mods.contains(KeyModifiers::CONTROL) => {
            encode_ctrl_letter(ch).map(|code| vec![code])
        }
        KeyCode::Char(ch) if mods.contains(KeyModifiers::ALT) => {
            let mut b = vec![0x1b];
            b.extend_from_slice(ch.to_string().as_bytes());
            Some(b)
        }
        KeyCode::Char(ch) => Some(ch.to_string().into_bytes()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        JoinThreadOutcome, PtyBoundaryPolicyDecision, classify_pty_boundary_failure,
        derive_poll_timeouts, dispatch_runtime_palette_command, encode_key_event,
        ensure_single_window, frame_budget_millis, is_runtime_palette_shortcut,
        join_thread_with_timeout,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rldyourterm_services::render_mode::RenderMode;
    use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
    use rldyourterm_settings::SettingsService;
    use std::{thread, time::Duration};

    #[test]
    fn encodes_basic_control_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(vec![b'\r'])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(vec![0x7f])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(vec![b'\t'])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(vec![0x1b])
        );
    }

    #[test]
    fn encodes_navigation_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
            Some(b"\x1b[F".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
            Some(b"\x1b[3~".to_vec())
        );
    }

    #[test]
    fn key_event_uses_ctrl_letter_encoding() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(vec![0x03])
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(
                KeyCode::Char('Q'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Some(vec![0x11])
        );
    }

    #[test]
    fn enforces_single_window_invariant() {
        assert!(ensure_single_window(1).is_ok());

        let error = ensure_single_window(2).expect_err("window_count=2 must fail");
        assert!(error.to_string().contains("single-window mode"));
    }

    #[test]
    fn derives_adaptive_poll_timeouts_from_runtime_hints() {
        let (gpu_min, gpu_max) = derive_poll_timeouts(RenderMode::Gpu, 144_000);
        let (cpu_min, cpu_max) = derive_poll_timeouts(RenderMode::Cpu, 60_000);

        assert!(gpu_min <= cpu_min);
        assert!(gpu_max < cpu_max);
        assert!(gpu_min <= gpu_max);
        assert!(cpu_min <= cpu_max);
    }

    #[test]
    fn falls_back_to_default_refresh_for_zero_hint() {
        assert_eq!(frame_budget_millis(0), frame_budget_millis(60_000));
    }

    #[test]
    fn detects_palette_shortcut_with_ctrl_or_super_shift_p() {
        assert!(is_runtime_palette_shortcut(KeyEvent::new(
            KeyCode::Char('P'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )));
        assert!(is_runtime_palette_shortcut(KeyEvent::new(
            KeyCode::Char('P'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT
        )));
        assert!(!is_runtime_palette_shortcut(KeyEvent::new(
            KeyCode::Char('p'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn palette_dispatch_updates_mode_state() {
        let mut settings = SettingsService::default();

        let result = dispatch_runtime_palette_command(&mut settings, "mode gpu");

        assert_eq!(result.updated_mode, Some(RenderMode::Gpu));
        assert_eq!(settings.state().mode, RenderMode::Gpu);
        assert!(result.message.contains("mode=gpu"));
    }

    #[test]
    fn palette_dispatch_toggles_diagnostics_state() {
        let mut settings = SettingsService::default();

        let on_result = dispatch_runtime_palette_command(&mut settings, "debug on");
        assert!(settings.state().debug_mode);
        assert!(on_result.message.contains("diagnostics=on"));

        let off_result = dispatch_runtime_palette_command(&mut settings, "debug off");
        assert!(!settings.state().debug_mode);
        assert!(off_result.message.contains("diagnostics=off"));
    }

    #[test]
    fn pty_write_boundary_policy_stays_recoverable_with_remaining_budget() {
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
    fn pty_write_boundary_policy_escalates_after_budget_exhaustion() {
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
    fn encodes_ctrl_arrow_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL)),
            Some(b"\x1b[1;5D".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL)),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn encodes_shift_arrow_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)),
            Some(b"\x1b[1;2A".to_vec())
        );
    }

    #[test]
    fn encodes_alt_arrow_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)),
            Some(b"\x1b[1;3D".to_vec())
        );
    }

    #[test]
    fn encodes_f_keys_and_nav_keys() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
            Some(b"\x1b[15~".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Insert, KeyModifiers::NONE)),
            Some(b"\x1b[2~".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            Some(b"\x1b[6~".to_vec())
        );
    }

    #[test]
    fn encodes_backtab_and_alt_backspace() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
            Some(b"\x1b\x7f".to_vec())
        );
    }

    #[test]
    fn encodes_alt_char_with_esc_prefix() {
        assert_eq!(
            encode_key_event(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT)),
            Some(b"\x1bf".to_vec())
        );
    }

    #[test]
    fn join_thread_with_timeout_returns_joined_for_finished_thread() {
        let handle = thread::spawn(|| {});
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(100),
            Duration::from_millis(1),
            "test_joined",
        );
        assert_eq!(outcome, JoinThreadOutcome::Joined);
    }

    #[test]
    fn join_thread_with_timeout_returns_timed_out_for_busy_thread() {
        let handle = thread::spawn(|| thread::sleep(Duration::from_millis(50)));
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(1),
            Duration::from_millis(1),
            "test_timeout",
        );
        assert_eq!(outcome, JoinThreadOutcome::TimedOut);
        thread::sleep(Duration::from_millis(60));
    }

    #[test]
    fn join_thread_with_timeout_detects_panicking_thread() {
        let handle = thread::spawn(|| panic!("panic for test"));
        let outcome = join_thread_with_timeout(
            handle,
            Duration::from_millis(100),
            Duration::from_millis(1),
            "test_panic",
        );
        assert_eq!(outcome, JoinThreadOutcome::Panicked);
    }
}
