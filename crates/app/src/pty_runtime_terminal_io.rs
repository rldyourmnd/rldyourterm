use super::*;

pub(super) fn handle_terminal_event_disconnect(
    pty: &dyn PtyIo,
    exit_code: &mut Option<i32>,
    requested_local_exit: &mut bool,
    context: &'static str,
) -> Result<()> {
    match pty
        .try_wait()
        .context(format!("failed to poll PTY after {context}"))?
    {
        Some(code) => {
            *exit_code = Some(code);
            Ok(())
        }
        None => {
            warn!(
                disconnect_context = context,
                "terminal input stream disconnected while PTY child is still running; closing PTY"
            );
            pty.close()
                .context(format!("failed to close PTY after {context}"))?;
            *requested_local_exit = true;
            exit_code.get_or_insert(0);
            Ok(())
        }
    }
}

pub(super) fn handle_pty_io_failure(
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

pub(super) fn handle_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
    detail: &str,
) -> Result<()> {
    match apply_pty_boundary_failure(session_policy, boundary, detail)? {
        BoundaryFailureOutcome::Continue {
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
            write_runtime_palette_line(&runtime_boundary_notice(
                boundary,
                attempt,
                remaining_budget,
                detail,
            ));
            Ok(())
        }
        BoundaryFailureOutcome::Fatal { reason } => Err(anyhow!(
            "fatal PTY boundary failure boundary={} reason={} detail={detail}",
            session_boundary_token(boundary),
            fatal_boundary_reason_token(reason),
        )),
    }
}

pub(super) fn mark_pty_boundary_recovered(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
) -> Result<()> {
    let Some(recovery) = shared_mark_pty_boundary_recovered(session_policy, boundary)? else {
        return Ok(());
    };

    info!(
        boundary = session_boundary_token(boundary),
        from = recovery.from.as_str(),
        to = recovery.to.as_str(),
        "PTY boundary recovered; TTY runtime returned to running state"
    );
    write_runtime_palette_line(&recovery.notice);
    Ok(())
}

pub(super) fn write_runtime_palette_line(line: &str) {
    let mut stdout = io::stdout();
    let mut payload = String::from("\r\n");
    payload.push_str(line);
    payload.push_str("\r\n");
    if let Err(error) = write_all_and_flush(&mut stdout, payload.as_bytes()) {
        warn!(error = %error, "failed to write runtime palette output");
    }
}

#[cfg(test)]
pub(super) fn dispatch_runtime_palette_command(
    settings: &mut SettingsService,
    input: &str,
) -> crate::runtime_shared::palette::RuntimePaletteDispatchResult {
    crate::runtime_shared::palette::dispatch_runtime_palette_command(settings, input, None)
}
