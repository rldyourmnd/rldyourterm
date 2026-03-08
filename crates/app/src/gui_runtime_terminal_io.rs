use super::*;

impl GuiRuntimeApp {
    pub(super) fn toggle_palette(&mut self) {
        let decision = toggle_runtime_palette(self.palette_open);
        self.palette_open = decision.next_open;
        if let Some(notice) = decision.notice {
            self.emit_runtime_notice(&notice);
        }
    }

    pub(super) fn handle_palette_action(&mut self, event: &WinitKeyEvent) -> Result<bool> {
        let diagnostics_enabled = self.settings.state().debug_mode;
        let decision = handle_runtime_palette_key_input(
            self.palette_open,
            runtime_key_from_winit_borrowed(event.logical_key.as_ref()),
            &mut self.settings,
            RuntimePaletteView {
                mode: self.ui_runtime.render_mode(),
                diagnostics_enabled,
                active_render_path: Some(self.ui_runtime.active_render_path()),
            },
        );
        if !decision.consumed {
            return Ok(false);
        }
        self.palette_open = decision.next_open;

        if let Some(dispatch) = decision.dispatch {
            let result_line = if let Some(command) = dispatch.command {
                apply_palette_settings_command_to_ui_runtime(&mut self.ui_runtime, command)?;
                self.sync_deferred_gpu_init_state();
                shared_runtime_palette_status_line(
                    command,
                    self.settings.state().mode,
                    self.settings.state().debug_mode,
                    Some(self.ui_runtime.active_render_path()),
                )
            } else {
                dispatch.message
            };
            self.emit_runtime_notice(&result_line);
        } else if let Some(notice) = decision.notice {
            self.emit_runtime_notice(&notice);
        }

        Ok(true)
    }

    pub(super) fn handle_keyboard_input(
        &mut self,
        event: &WinitKeyEvent,
        event_loop: &ActiveEventLoop,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }

        if is_local_shutdown_key_winit(event, self.modifiers) {
            self.emit_close_intent();
            self.exit_code.get_or_insert(0);
            event_loop.exit();
            return;
        }

        if is_runtime_palette_shortcut_winit(event.logical_key.as_ref(), self.modifiers) {
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

        let bytes = shared_encode_winit_key_event(event, self.modifiers);

        if let Some(ref bytes) = bytes {
            trace!(key = ?event.logical_key, len = bytes.len(), "keyboard input to PTY");
            let _ =
                self.write_pty_payload(bytes, event_loop, "failed to write keyboard input to PTY");
        }
    }

    pub(super) fn handle_text_commit(&mut self, text: &str, event_loop: &ActiveEventLoop) {
        warn!(
            len = text.len(),
            "IME commit received unexpectedly (IME should be disabled)"
        );
        if text.is_empty() {
            return;
        }

        let _ = self.write_pty_payload(
            text.as_bytes(),
            event_loop,
            "failed to write IME text to PTY",
        );
    }

    pub(super) fn handle_pty_io_error(
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

    pub(super) fn handle_pty_boundary_failure(
        &mut self,
        boundary: SessionBoundary,
        detail: &str,
    ) -> Result<PtyBoundaryLoopAction> {
        match apply_pty_boundary_failure(&mut self.session_policy, boundary, detail)? {
            BoundaryFailureOutcome::Continue {
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
                self.emit_runtime_notice(&runtime_boundary_notice(
                    boundary,
                    attempt,
                    remaining_budget,
                    detail,
                ));
                Ok(PtyBoundaryLoopAction::Continue)
            }
            BoundaryFailureOutcome::Fatal { reason } => Err(anyhow!(
                "fatal PTY boundary failure boundary={} reason={} detail={detail}",
                session_boundary_token(boundary),
                fatal_boundary_reason_token(reason),
            )),
        }
    }

    pub(super) fn mark_pty_boundary_recovered(&mut self, boundary: SessionBoundary) -> Result<()> {
        let Some(recovery) =
            shared_mark_pty_boundary_recovered(&mut self.session_policy, boundary)?
        else {
            return Ok(());
        };

        info!(
            boundary = session_boundary_token(boundary),
            from = recovery.from.as_str(),
            to = recovery.to.as_str(),
            "PTY boundary recovered; GUI runtime returned to running state"
        );
        self.emit_runtime_notice(&recovery.notice);
        Ok(())
    }

    pub(super) fn write_pty_chunk(
        &mut self,
        chunk: &[u8],
        event_loop: &ActiveEventLoop,
        error_context: &'static str,
    ) -> PtyWriteOutcome {
        if chunk.is_empty() {
            return PtyWriteOutcome::Written;
        }

        match write_all_and_flush(&mut *self.writer, chunk) {
            Ok(()) => PtyWriteOutcome::Written,
            Err(error) => {
                match self.handle_pty_io_error(SessionBoundary::PtyWrite, error, error_context) {
                    Ok(PtyBoundaryLoopAction::Continue) => PtyWriteOutcome::RecoverableFailure,
                    Ok(PtyBoundaryLoopAction::ExitLoop) => {
                        event_loop.exit();
                        PtyWriteOutcome::ExitLoop
                    }
                    Err(policy_error) => {
                        self.fatal_error = Some(policy_error);
                        event_loop.exit();
                        PtyWriteOutcome::ExitLoop
                    }
                }
            }
        }
    }

    pub(super) fn finish_pty_write(&mut self, event_loop: &ActiveEventLoop) -> bool {
        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyWrite) {
            self.fatal_error = Some(error);
            event_loop.exit();
            return false;
        }
        true
    }

    pub(super) fn write_pty_payload(
        &mut self,
        payload: &[u8],
        event_loop: &ActiveEventLoop,
        error_context: &'static str,
    ) -> bool {
        match self.write_pty_chunk(payload, event_loop, error_context) {
            PtyWriteOutcome::Written => self.finish_pty_write(event_loop),
            PtyWriteOutcome::RecoverableFailure | PtyWriteOutcome::ExitLoop => false,
        }
    }

    pub(super) fn write_pty_segments<'a, I>(
        &mut self,
        segments: I,
        event_loop: &ActiveEventLoop,
        error_context: &'static str,
    ) -> bool
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        let mut wrote_any = false;
        for segment in segments {
            match self.write_pty_chunk(segment, event_loop, error_context) {
                PtyWriteOutcome::Written => {
                    wrote_any = wrote_any || !segment.is_empty();
                }
                PtyWriteOutcome::RecoverableFailure | PtyWriteOutcome::ExitLoop => return false,
            }
        }
        !wrote_any || self.finish_pty_write(event_loop)
    }

    pub(super) fn handle_clipboard_paste(&mut self, event_loop: &ActiveEventLoop) {
        let Some(text) = read_clipboard_text_for_paste(self.clipboard.as_ref()) else {
            return;
        };
        debug!(bytes = text.len(), "clipboard paste");
        let text = cap_paste_text(&text);
        if self.terminal.bracketed_paste_enabled() {
            let _ = self.write_pty_segments(
                [
                    b"\x1b[200~".as_slice(),
                    text.as_bytes(),
                    b"\x1b[201~".as_slice(),
                ],
                event_loop,
                "failed to write clipboard paste to PTY",
            );
        } else {
            let _ = self.write_pty_payload(
                text.as_bytes(),
                event_loop,
                "failed to write clipboard paste to PTY",
            );
        }
    }
}

#[cfg(test)]
pub(super) fn dispatch_runtime_palette_command(
    ui_runtime: &mut UiRuntime,
    settings: &mut SettingsService,
    input: &str,
) -> Result<String> {
    let mut result = crate::runtime_shared::palette::dispatch_runtime_palette_command(
        settings,
        input,
        Some(ui_runtime.active_render_path()),
    );
    if let Some(command) = result.command {
        apply_palette_settings_command_to_ui_runtime(ui_runtime, command)?;
        result.message = shared_runtime_palette_status_line(
            command,
            settings.state().mode,
            settings.state().debug_mode,
            Some(ui_runtime.active_render_path()),
        );
    }
    Ok(result.message)
}

pub(super) fn apply_palette_settings_command_to_ui_runtime(
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

fn is_paste_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    let is_v = match key.as_ref() {
        Key::Character(text) => text.eq_ignore_ascii_case("v"),
        _ => false,
    };
    if !is_v {
        return false;
    }

    #[cfg(target_os = "macos")]
    {
        modifiers.super_key()
    }
    #[cfg(not(target_os = "macos"))]
    {
        modifiers.control_key() && modifiers.shift_key()
    }
}

pub(super) fn read_clipboard_text_for_paste(clipboard: &dyn ClipboardAdapter) -> Option<String> {
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Some(text),
        Ok(_) | Err(_) => {
            debug!("clipboard paste: empty or unavailable");
            None
        }
    }
}

pub(super) fn cap_paste_text(text: &str) -> &str {
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
