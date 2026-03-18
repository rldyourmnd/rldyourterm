// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct EventPollController {
    min_timeout: Duration,
    max_timeout: Duration,
    next_timeout: Duration,
}

impl EventPollController {
    pub(super) fn from_config(config: TtyRuntimeConfig) -> Self {
        let (min_timeout, max_timeout) =
            derive_poll_timeouts(config.initial_mode, config.refresh_rate_millihz);
        Self {
            min_timeout,
            max_timeout,
            next_timeout: min_timeout,
        }
    }

    pub(super) fn next_timeout(&self) -> Duration {
        self.next_timeout
    }

    pub(super) fn on_terminal_event(&mut self) {
        self.next_timeout = self.min_timeout;
    }

    pub(super) fn on_idle_poll(&mut self) {
        self.next_timeout = self
            .next_timeout
            .checked_mul(2)
            .unwrap_or(self.max_timeout)
            .min(self.max_timeout);
    }

    pub(super) fn bounds_millis(&self) -> (u128, u128) {
        (self.min_timeout.as_millis(), self.max_timeout.as_millis())
    }
}

pub(super) struct RawModeGuard;

impl RawModeGuard {
    pub(super) fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable terminal raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

pub(super) fn ensure_tty_stdio_is_terminal() -> Result<()> {
    let stdin_is_terminal = io::stdin().is_terminal();
    let stdout_is_terminal = io::stdout().is_terminal();
    if stdin_is_terminal && stdout_is_terminal {
        return Ok(());
    }

    Err(anyhow!(tty_stdio_requirement_message(
        stdin_is_terminal,
        stdout_is_terminal
    )))
}

pub(super) fn tty_stdio_requirement_message(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> String {
    format!(
        "TTY interactive runtime requires terminal stdin/stdout (stdin_is_terminal={} stdout_is_terminal={})",
        stdin_is_terminal, stdout_is_terminal,
    )
}

pub(super) fn ensure_single_window(window_count: u8) -> Result<()> {
    if window_count != SINGLE_WINDOW_BASELINE {
        return Err(anyhow!(
            "tty runtime requires single-window mode; required_window_count={SINGLE_WINDOW_BASELINE} requested_window_count={window_count}"
        ));
    }
    Ok(())
}

pub(super) fn derive_poll_timeouts(
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

pub(super) fn frame_budget_millis(refresh_rate_millihz: u32) -> u64 {
    shared_frame_budget_millis(refresh_rate_millihz)
}

pub(super) fn current_pty_size() -> PtySize {
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

pub(super) fn is_press_like(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}
