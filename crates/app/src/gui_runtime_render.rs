// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::*;

#[derive(Debug)]
struct GpuFailureRouteContext {
    gpu_failure_sequence: u64,
    failure_kind: GpuFailureKind,
    observed_at_millis: u64,
    render_attempt_sequence: Option<u64>,
    retry_log_message: Option<&'static str>,
    fallback_log_message: &'static str,
    ignored_log_message: Option<&'static str>,
    fatal_message: String,
    emit_fatal_diagnostics: bool,
}

impl GuiRuntimeApp {
    pub(super) fn persist_gpu_pipeline_cache(&mut self) {
        if let Some(cache_dir) = &self.gpu_cache_dir {
            self.gpu_renderer.save_pipeline_cache(cache_dir);
        }
    }

    pub(super) fn try_deferred_gpu_init(&mut self, event_loop: &ActiveEventLoop) {
        if self.control.ui_runtime.active_render_path() == ActiveRenderPath::Cpu {
            self.control.render_backend.clear_deferred_gpu_init();
            return;
        }
        if let Some(retry_at) = self.control.render_backend.deferred_retry_deadline()
            && Instant::now() < retry_at
        {
            return;
        }

        let Some(window) = self.window.window.clone() else {
            return;
        };
        let size = cap_framebuffer_extent(window.inner_size());

        debug!("deferred GPU init: dropping softbuffer for Wayland surface exclusivity");
        self.window.surface = None;
        self.window.last_softbuffer_size = None;
        self.window.context = None;

        let attempt = self.control.render_backend.begin_deferred_attempt();
        debug!("deferred GPU init: attempting GPU initialization");
        match self.gpu_renderer.initialize(
            window,
            size.width,
            size.height,
            self.gpu_cache_dir.as_deref(),
        ) {
            Ok(()) => {
                self.control.render_backend.mark_deferred_ready();
                info!("GPU backend initialized successfully");
                self.terminal.grid.mark_all_dirty();
                self.queue_redraw();
            }
            Err(error) => {
                self.control
                    .render_backend
                    .record_deferred_failure_attempt(attempt);
                let gpu_failure_sequence = self.control.render_backend.next_gpu_failure_sequence();
                let remaining = DEFERRED_GPU_INIT_RETRY_BUDGET.saturating_sub(attempt);
                warn!(
                    error = ?error,
                    attempt,
                    retry_budget = DEFERRED_GPU_INIT_RETRY_BUDGET,
                    retries_remaining = remaining,
                    mode = ?self.control.ui_runtime.render_mode(),
                    active_path = ?self.control.ui_runtime.active_render_path(),
                    "deferred GPU init failed"
                );

                if attempt < DEFERRED_GPU_INIT_RETRY_BUDGET {
                    let backoff = deferred_gpu_init_backoff(attempt);
                    self.control
                        .render_backend
                        .schedule_deferred_retry(attempt, backoff);
                    self.queue_redraw();
                    return;
                }

                self.control.render_backend.mark_deferred_exhausted(attempt);
                let observed_at_millis = self
                    .started_at
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                let failure_kind = GpuFailureKind::DeviceLost;
                let fatal_message = format!(
                    "forced GPU mode initialization failed after {} attempts: {:?}",
                    attempt, error
                );
                let handling = dispatch_gpu_failure_command(
                    &mut self.control.ui_runtime,
                    failure_kind,
                    observed_at_millis,
                );
                let outcome = handling.and_then(|(_, handling)| {
                    self.route_gpu_failure_handling(
                        handling,
                        GpuFailureRouteContext {
                            gpu_failure_sequence,
                            failure_kind,
                            observed_at_millis,
                            render_attempt_sequence: None,
                            retry_log_message: None,
                            fallback_log_message: "deferred GPU init exhausted retry budget; applying deterministic CPU fallback",
                            ignored_log_message: None,
                            fatal_message,
                            emit_fatal_diagnostics: true,
                        },
                    )
                });
                if let Err(error) = outcome {
                    self.fatal_error = Some(error);
                    event_loop.exit();
                }
            }
        }
    }

    pub(super) fn sync_deferred_gpu_init_state(&mut self) {
        let target_path = self.control.ui_runtime.active_render_path();
        match self
            .control
            .render_backend
            .sync_with_target_path(target_path, self.gpu_renderer.is_initialized())
        {
            BackendSyncAction::ReleaseGpuBackend => {
                self.gpu_renderer.release_backend();
            }
            BackendSyncAction::None => {}
        }
    }

    pub(super) fn is_gpu_lane_ready(&self) -> bool {
        self.control.render_backend.is_gpu_lane_ready(
            self.control.ui_runtime.active_render_path(),
            self.gpu_renderer.is_initialized(),
        )
    }

    fn apply_gpu_auto_fallback(
        &mut self,
        transition_sequence: u64,
        gpu_failure_sequence: u64,
        failure_kind: GpuFailureKind,
        observed_at_millis: u64,
        log_message: &'static str,
    ) {
        self.gpu_renderer.release_backend();
        self.sync_deferred_gpu_init_state();
        // Force full redraw on CPU path: GPU previously cleared dirty flags,
        // so the CPU softbuffer has no valid persisted content.
        self.terminal.grid.mark_all_dirty();
        let (diagnostics_event, fallback_notice) = emit_gpu_auto_fallback_observability(
            &self.control.diagnostics,
            transition_sequence,
            gpu_failure_sequence,
            self.control
                .render_backend
                .current_render_attempt_sequence(),
            failure_kind,
            observed_at_millis,
        );
        warn!(
            transition_sequence,
            gpu_failure_sequence,
            render_attempt_sequence = self.control.render_backend.current_render_attempt_sequence(),
            diagnostics_event_id = %diagnostics_event.event_id,
            diagnostics_correlation = ?diagnostics_event.correlation_id,
            mode = ?self.control.ui_runtime.render_mode(),
            active_path = ?self.control.ui_runtime.active_render_path(),
            "{log_message}"
        );
        self.emit_runtime_notice(&fallback_notice);
    }

    fn route_gpu_failure_handling(
        &mut self,
        handling: GpuFailureHandling,
        context: GpuFailureRouteContext,
    ) -> Result<()> {
        match handling {
            GpuFailureHandling::RetryScheduled {
                failure_streak,
                retry_budget_remaining,
            } => {
                if let Some(message) = context.retry_log_message {
                    if let Some(render_attempt_sequence) = context.render_attempt_sequence {
                        warn!(
                            gpu_failure_sequence = context.gpu_failure_sequence,
                            render_attempt_sequence,
                            failure_kind = ?context.failure_kind,
                            failure_streak,
                            retry_budget_remaining,
                            mode = ?self.control.ui_runtime.render_mode(),
                            active_path = ?self.control.ui_runtime.active_render_path(),
                            "{message}"
                        );
                    } else {
                        warn!(
                            gpu_failure_sequence = context.gpu_failure_sequence,
                            failure_kind = ?context.failure_kind,
                            failure_streak,
                            retry_budget_remaining,
                            mode = ?self.control.ui_runtime.render_mode(),
                            active_path = ?self.control.ui_runtime.active_render_path(),
                            "{message}"
                        );
                    }
                }
                self.queue_redraw();
                Ok(())
            }
            GpuFailureHandling::FallbackToCpu {
                transition_sequence,
            } => {
                self.apply_gpu_auto_fallback(
                    transition_sequence,
                    context.gpu_failure_sequence,
                    context.failure_kind,
                    context.observed_at_millis,
                    context.fallback_log_message,
                );
                Ok(())
            }
            GpuFailureHandling::FatalForcedGpu => {
                if context.emit_fatal_diagnostics {
                    self.control
                        .diagnostics
                        .emit_kind(EventKind::SessionError, context.fatal_message.clone());
                }
                Err(anyhow!(context.fatal_message))
            }
            GpuFailureHandling::Ignored => {
                if let Some(message) = context.ignored_log_message {
                    trace!(
                        gpu_failure_sequence = context.gpu_failure_sequence,
                        failure_kind = ?context.failure_kind,
                        mode = ?self.control.ui_runtime.render_mode(),
                        active_path = ?self.control.ui_runtime.active_render_path(),
                        "{message}"
                    );
                }
                Ok(())
            }
        }
    }

    pub(super) fn draw_frame(&mut self) -> Result<()> {
        let render_attempt_sequence = self.control.render_backend.begin_render_attempt();

        trace!(
            render_path = ?self.control.ui_runtime.active_render_path(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            render_attempt_sequence,
            "draw_frame: begin"
        );

        if self.is_gpu_lane_ready() {
            let dirty_rows = self.terminal.grid.dirty_rows();
            let scroll_count = self.terminal.grid.scroll_count();
            let (sel_start, sel_end) = self.selection_flat_range();
            match self.gpu_renderer.render_frame(
                &self.terminal,
                dirty_rows,
                scroll_count,
                self.frame.blink_visible,
                self.interaction.viewport_offset,
                sel_start,
                sel_end,
            ) {
                Ok(()) => {
                    self.terminal.grid.clear_dirty_rows();
                    let _ = self
                        .control
                        .ui_runtime
                        .handle_command(UiRuntimeCommand::GpuFramePresented)
                        .context("failed to dispatch UiRuntimeCommand::GpuFramePresented")?;
                    trace!("draw_frame: presented (GPU)");
                    return Ok(());
                }
                Err(error) => {
                    let gpu_failure_sequence =
                        self.control.render_backend.next_gpu_failure_sequence();
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
                        mode = ?self.control.ui_runtime.render_mode(),
                        active_path = ?self.control.ui_runtime.active_render_path(),
                        "gpu render failed; routing through ui runtime command path"
                    );

                    let (receipt, handling) = dispatch_gpu_failure_command(
                        &mut self.control.ui_runtime,
                        failure_kind,
                        observed_at_millis,
                    )?;
                    if let Err(error) = self.control.diagnostics.emit_runtime_command_receipt(
                        None,
                        RuntimeCommandSourceKind::GpuFailureHandler,
                        None,
                        &receipt,
                    ) {
                        warn!(
                            error = ?error,
                            failure_kind = ?failure_kind,
                            observed_at_millis,
                            "failed to emit typed GPU failure command diagnostics"
                        );
                    }
                    self.route_gpu_failure_handling(
                        handling,
                        GpuFailureRouteContext {
                            gpu_failure_sequence,
                            failure_kind,
                            observed_at_millis,
                            render_attempt_sequence: Some(render_attempt_sequence),
                            retry_log_message: Some(
                                "gpu retry scheduled; session remains active",
                            ),
                            fallback_log_message:
                                "gpu failure applied deterministic cpu fallback; session remains active",
                            ignored_log_message: Some(
                                "draw_frame: gpu failure handling ignored (already on CPU path)",
                            ),
                            fatal_message: format!(
                                "forced gpu mode render failure: kind={failure_kind:?} observed_at_millis={observed_at_millis} render_attempt_sequence={render_attempt_sequence} gpu_failure_sequence={gpu_failure_sequence}"
                            ),
                            emit_fatal_diagnostics: false,
                        },
                    )?;
                }
            }
        }

        self.draw_cpu_frame()
    }

    fn draw_cpu_frame(&mut self) -> Result<()> {
        let width = self.window.window_size.width;
        let height = self.window.window_size.height;
        if width == 0 || height == 0 {
            debug!(width, height, "draw_frame: skipped, zero window dimensions");
            return Ok(());
        }

        let (sel_start, sel_end) = self.selection_flat_range();

        // Lazily create softbuffer surface on first CPU render (e.g. after GPU fallback).
        // Cannot create at bootstrap when GPU surface already owns the Wayland buffer queue.
        self.ensure_softbuffer_surface()
            .context("failed to initialize softbuffer for CPU render")?;

        let surface = self
            .window
            .surface
            .as_mut()
            .ok_or_else(|| anyhow!("softbuffer surface is not initialized"))?;

        let nz_width = NonZeroU32::new(width).ok_or_else(|| anyhow!("zero width is invalid"))?;
        let nz_height = NonZeroU32::new(height).ok_or_else(|| anyhow!("zero height is invalid"))?;
        let target_size = PhysicalSize::new(width, height);
        if self.window.last_softbuffer_size != Some(target_size) {
            surface
                .resize(nz_width, nz_height)
                .map_err(|error| anyhow!("failed to resize softbuffer surface: {error}"))?;
            self.window.last_softbuffer_size = Some(target_size);
        }

        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!("failed to acquire softbuffer frame: {error}"))?;
        let framebuffer_age = buffer.age();
        render_terminal_buffer(
            &mut buffer,
            width as usize,
            height as usize,
            &mut self.terminal,
            &mut self.glyph_cache,
            framebuffer_age,
            &self.frame.previous_cpu_damage_rows,
            self.frame.last_rendered_cursor_row,
            &mut self.frame.current_cpu_damage_rows_scratch,
            &mut self.frame.repaint_rows_scratch,
            &mut self.frame.persisted_cpu_damage_rows_scratch,
            self.frame.blink_visible,
            self.interaction.viewport_offset,
            sel_start,
            sel_end,
        );
        std::mem::swap(
            &mut self.frame.previous_cpu_damage_rows,
            &mut self.frame.persisted_cpu_damage_rows_scratch,
        );
        self.frame.last_rendered_cursor_row = Some(self.terminal.cursor.row);
        buffer
            .present()
            .map_err(|error| anyhow!("failed to present GUI frame: {error}"))?;
        trace!("draw_frame: presented (CPU)");
        Ok(())
    }
}
