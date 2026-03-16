// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

#[path = "pty_runtime_control.rs"]
mod control;
#[path = "pty_runtime_output.rs"]
mod output;
#[path = "pty_runtime_terminal_io.rs"]
mod terminal_io;

use std::io::{self, BufWriter, ErrorKind, IsTerminal, Read, Write};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use self::control::{
    EventPollController, RawModeGuard, current_pty_size, ensure_single_window,
    ensure_tty_stdio_is_terminal, is_press_like,
};
#[cfg(test)]
use self::control::{derive_poll_timeouts, frame_budget_millis, tty_stdio_requirement_message};
#[cfg(test)]
use self::output::{is_stdout_disconnect_error, should_flush_read_pump};
use self::output::{join_thread_with_timeout, spawn_read_pump};
#[cfg(test)]
use self::terminal_io::dispatch_runtime_palette_command;
use self::terminal_io::{
    handle_pty_boundary_failure, handle_pty_io_failure, handle_terminal_event_disconnect,
    mark_pty_boundary_recovered, write_runtime_palette_line,
};
use crate::runtime_shared::display::{fatal_boundary_reason_token, session_boundary_token};
use crate::runtime_shared::input::{
    encode_crossterm_key_event, is_local_shutdown_key_crossterm,
    is_runtime_palette_shortcut_crossterm, runtime_key_event_from_crossterm,
};
use crate::runtime_shared::io::{is_disconnect_error, write_all_and_flush};
use crate::runtime_shared::palette::{
    RuntimePaletteView, handle_runtime_palette_key_input, toggle_runtime_palette,
};
use crate::runtime_shared::pty_boundary::{
    BoundaryFailureOutcome, PtyReadFailureResolution, apply_pty_boundary_failure,
    fatal_pty_boundary_failure, mark_pty_boundary_recovered as shared_mark_pty_boundary_recovered,
    resolve_live_pty_read_failure, runtime_boundary_notice,
};
use crate::runtime_shared::runtime_config::frame_budget_millis as shared_frame_budget_millis;
use crate::runtime_shared::shutdown::{
    JoinThreadOutcome, SHUTDOWN_JOIN_POLL_INTERVAL, SHUTDOWN_JOIN_TIMEOUT,
    join_thread_with_timeout as shared_join_thread_with_timeout,
};
use crate::runtime_shared::spawn_env::ai_cli_spawn_env_overrides;
use anyhow::{Context, Result, anyhow};
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal;
use rldyourterm_foundation::api::pty::{PtyFactory, PtyIo, PtySize, PtySpawnConfig};
use rldyourterm_foundation_platform::pty::PlatformPtyFactory;
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{SessionBoundary, SessionController};
use rldyourterm_settings::{SettingsCommand, SettingsService};
use rldyourterm_ui::SINGLE_WINDOW_BASELINE;
use tracing::{info, warn};

const MIN_EVENT_POLL_TIMEOUT_MILLIS: u64 = 1;
const MAX_EVENT_POLL_TIMEOUT_MILLIS: u64 = 200;
const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const READ_PUMP_FLUSH_INTERVAL: Duration = Duration::from_millis(4);
const READ_PUMP_FLUSH_MAX_BYTES: usize = 32 * 1024;
const READ_PUMP_SIGNAL_STDOUT_DISCONNECTED: &str = "stdout-disconnected";

#[derive(Debug, Clone, Copy)]
pub struct TtyRuntimeConfig {
    pub initial_mode: RenderMode,
    pub refresh_rate_millihz: u32,
    pub window_count: u8,
}

pub fn run_interactive_pty(
    shell_executable: &str,
    shell_args: &[String],
    runtime_config: TtyRuntimeConfig,
) -> Result<i32> {
    ensure_single_window(runtime_config.window_count)?;
    ensure_tty_stdio_is_terminal()?;
    let _raw_mode_guard = RawModeGuard::new()?;

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
    let spawn_env = ai_cli_spawn_env_overrides();
    debug_assert!(spawn_env.iter().all(|(key, _)| !key.trim().is_empty()));
    info!(
        env_overrides = spawn_env.len(),
        "applying default AI CLI spawn environment overrides for TTY runtime"
    );
    let spawn_config = PtySpawnConfig {
        shell_command: shell_executable.to_owned(),
        args: shell_args.to_vec(),
        cwd: None,
        env: spawn_env,
        size: initial_size,
    };

    let factory = PlatformPtyFactory;
    let pty = factory
        .spawn(spawn_config)
        .context("failed to spawn interactive PTY")?;
    let reader = pty.take_reader().context("failed to acquire PTY reader")?;
    let mut writer = pty.take_writer().context("failed to acquire PTY writer")?;
    let (read_pump, read_pump_failures) = spawn_read_pump(reader);
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
                let detail = format!("failed to poll PTY child status: {error}");
                fatal_error = Some(fatal_pty_boundary_failure(
                    &mut session_policy,
                    SessionBoundary::PtyWait,
                    &detail,
                ));
                break;
            }
        }

        match read_pump_failures.try_recv() {
            Ok(detail) => {
                if detail == READ_PUMP_SIGNAL_STDOUT_DISCONNECTED {
                    info!(
                        "TTY stdout consumer disconnected; closing PTY session without fatal escalation"
                    );
                    if let Err(error) = pty
                        .close()
                        .context("failed to close PTY after stdout disconnect")
                    {
                        fatal_error = Some(error);
                    } else {
                        requested_local_exit = true;
                        exit_code.get_or_insert(0);
                    }
                    break;
                }
                let detail = format!("TTY read pump failure detail={detail}");
                match resolve_live_pty_read_failure(
                    &*pty,
                    &mut session_policy,
                    &detail,
                    "failed to poll PTY after read pump failure",
                ) {
                    Ok(PtyReadFailureResolution::ChildExited(code)) => {
                        exit_code = Some(code);
                        info!(
                            exit_code = code,
                            "TTY read pump failure observed after child exit; stopping without fatal escalation"
                        );
                    }
                    Err(policy_error) => fatal_error = Some(policy_error),
                }
                break;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) if read_pump.is_finished() => {
                if requested_local_exit {
                    exit_code.get_or_insert(0);
                    break;
                }
                match resolve_live_pty_read_failure(
                    &*pty,
                    &mut session_policy,
                    "PTY read pump terminated unexpectedly while child is running",
                    "failed to poll PTY after read pump disconnect",
                ) {
                    Ok(PtyReadFailureResolution::ChildExited(code)) => exit_code = Some(code),
                    Err(policy_error) => fatal_error = Some(policy_error),
                }
                break;
            }
            Err(TryRecvError::Disconnected) => {}
        }

        let has_event = match event::poll(poll_controller.next_timeout()) {
            Ok(value) => value,
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                continue;
            }
            Err(error) if is_disconnect_error(&error) => {
                match handle_terminal_event_disconnect(
                    &*pty,
                    &mut exit_code,
                    &mut requested_local_exit,
                    "terminal event poll disconnect",
                ) {
                    Ok(()) => {}
                    Err(wait_error) => {
                        fatal_error = Some(wait_error);
                    }
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
                match handle_terminal_event_disconnect(
                    &*pty,
                    &mut exit_code,
                    &mut requested_local_exit,
                    "terminal event read disconnect",
                ) {
                    Ok(()) => {}
                    Err(wait_error) => {
                        fatal_error = Some(wait_error);
                    }
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
                if is_local_shutdown_key_crossterm(key_event) {
                    requested_local_exit = true;
                    break;
                }

                if is_runtime_palette_shortcut_crossterm(key_event) {
                    let decision = toggle_runtime_palette(palette_open);
                    palette_open = decision.next_open;
                    if let Some(notice) = decision.notice {
                        write_runtime_palette_line(&notice);
                    }
                    continue;
                }

                if palette_open {
                    let diagnostics_enabled = settings.state().debug_mode;
                    let decision = handle_runtime_palette_key_input(
                        palette_open,
                        runtime_key_event_from_crossterm(key_event).map(|event| event.key),
                        &mut settings,
                        RuntimePaletteView {
                            mode: active_mode,
                            diagnostics_enabled,
                            active_render_path: None,
                        },
                    );
                    palette_open = decision.next_open;

                    if let Some(dispatch) = decision.dispatch {
                        if let Some(updated_mode) = dispatch.updated_mode {
                            active_mode = updated_mode;
                            poll_controller = EventPollController::from_config(TtyRuntimeConfig {
                                initial_mode: active_mode,
                                refresh_rate_millihz: runtime_config.refresh_rate_millihz,
                                window_count: runtime_config.window_count,
                            });
                        }
                        if let Some(notice) = decision.notice {
                            write_runtime_palette_line(&notice);
                        } else {
                            write_runtime_palette_line(&dispatch.message);
                        }
                    } else if let Some(notice) = decision.notice {
                        write_runtime_palette_line(&notice);
                    }
                    continue;
                }

                let modes = crate::runtime_shared::input::TerminalModeFlags::default();
                if let Some(bytes) = encode_crossterm_key_event(key_event, modes) {
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
            Err(error) => {
                let detail = format!("failed while waiting for PTY child exit: {error}");
                fatal_error = Some(fatal_pty_boundary_failure(
                    &mut session_policy,
                    SessionBoundary::PtyWait,
                    &detail,
                ));
            }
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

#[cfg(test)]
mod tests;
