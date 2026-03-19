// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;

impl ApplicationHandler<GuiEvent> for GuiRuntimeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        debug!(
            window_exists = self.window.has_window(),
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
                    self.fatal_error =
                        Some(self.force_fatal_pty_boundary_failure(*boundary, message));
                    event_loop.exit();
                    return;
                }
                if *boundary == SessionBoundary::PtyRead {
                    if self.exit_code.is_some() {
                        self.begin_child_exit_drain(event_loop);
                        return;
                    }
                    match self.resolve_live_pty_read_failure(
                        message,
                        "failed to poll PTY after reader boundary failure",
                    ) {
                        Ok(PtyReadFailureResolution::ChildExited(code)) => {
                            self.exit_code = Some(code);
                            info!(
                                exit_code = code,
                                "reader boundary reported after child exit; draining remaining output"
                            );
                            self.begin_child_exit_drain(event_loop);
                            return;
                        }
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
        if Some(window_id) != self.window.window_id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => self.handle_close_requested(event_loop),
            WindowEvent::RedrawRequested => {
                self.frame.redraw_in_flight = false;
                // Synchronized output (mode 2026): defer rendering while BSU
                // is active. The pending redraw will be serviced when the
                // application sends ESU (CSI ? 2026 l) and queue_redraw fires.
                if self.terminal.synchronized_output_enabled() {
                    return;
                }
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
                if let Some(window) = self.window.window_ref() {
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
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.handle_text_preedit(&text),
            WindowEvent::Ime(Ime::Disabled) => self.handle_ime_disabled(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.interaction.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.handle_cursor_moved(position, event_loop);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_input(state, button, event_loop);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_mouse_wheel(delta, event_loop);
            }
            WindowEvent::Focused(focused) => {
                debug!(focused, "window focus changed");
                if self.terminal.focus_reporting_enabled() {
                    let seq = if focused { b"\x1b[I" } else { b"\x1b[O" };
                    let _ = self.write_pty_payload(
                        seq.as_slice(),
                        event_loop,
                        "failed to write focus event to PTY",
                    );
                }
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
        if self.control.render_backend.deferred_gpu_init_pending() {
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
        if self.control.render_backend.deferred_gpu_init_pending()
            && let Some(retry_at) = self.control.render_backend.deferred_retry_deadline()
            && Instant::now() < retry_at
        {
            event_loop.set_control_flow(ControlFlow::WaitUntil(retry_at));
            return;
        }

        if self.frame.last_blink_toggle.elapsed() >= BLINK_TOGGLE_INTERVAL {
            self.frame.blink_visible = !self.frame.blink_visible;
            self.frame.last_blink_toggle = Instant::now();
            self.terminal.grid.mark_all_dirty();
            self.queue_redraw();
        }

        self.request_redraw_if_needed();
        let wait_policy = self.control.render_backend.wait_policy(
            self.control.ui_runtime.active_render_path(),
            self.gpu_renderer.is_initialized(),
            self.frame.redraw_pending,
            self.control.ui_runtime.cadence().frame_interval(),
        );

        trace!(
            render_path = ?self.control.ui_runtime.active_render_path(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            wait_policy = ?wait_policy,
            "about_to_wait: selecting control flow"
        );
        event_loop.set_control_flow(wait_policy.control_flow(Instant::now()));
    }
}

pub(super) fn child_exit_drain_timed_out(started_at: Instant, now: Instant) -> bool {
    shared_child_exit_drain_timed_out(started_at, now, CHILD_EXIT_DRAIN_MAX_WAIT)
}

pub(super) fn resolve_gpu_cache_dir() -> Option<PathBuf> {
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
