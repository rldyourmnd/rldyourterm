// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ViewportGeometry {
    pub(super) cols: u16,
    pub(super) rows: u16,
    pub(super) pixel_width: u16,
    pub(super) pixel_height: u16,
}

impl GuiRuntimeApp {
    pub(super) fn bootstrap_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.window.is_some() {
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
        // Terminal emulators rely solely on wl_keyboard for input.
        window.set_ime_allowed(false);

        let window_control = PlatformWindowFactory::from_winit_window(window.clone())
            .init(FoundationWindowConfig {
                title: "rldyourterm".to_owned(),
                width: DEFAULT_GUI_WIDTH,
                height: DEFAULT_GUI_HEIGHT,
                min_width: 1,
                min_height: 1,
                high_dpi: true,
            })
            .context("failed to initialize foundation window control from winit window")?;

        // GPU initialization is deferred so the event loop can process time-sensitive
        // terminal queries before potentially blocking GPU init. Start with softbuffer
        // for immediate CPU rendering; the deferred path will drop it before GPU init.
        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;
        self.window.context = Some(context);
        self.window.surface = Some(surface);
        debug!(
            gpu_deferred = self.control.render_backend.deferred_gpu_init_pending(),
            "bootstrap: softbuffer context created, GPU init deferred to event loop"
        );

        self.window.window_size = cap_framebuffer_extent(window.inner_size());
        self.window.window_id = Some(window.id());
        self.window.window_control = Some(window_control);
        self.window.window = Some(window);

        debug!("bootstrap: updating viewport geometry");
        self.update_viewport_geometry(event_loop);

        debug!("bootstrap: drawing initial frame");
        self.draw_frame()
            .context("failed to draw initial frame during bootstrap")?;

        debug!("bootstrap: applying visibility handshake");
        self.apply_post_draw_visibility_handshake();
        debug!("bootstrap: complete");
        Ok(())
    }

    /// Drop order matters: surface before context, context before window,
    /// GPU backend before the window itself.
    pub(super) fn release_window_resources(&mut self) {
        debug!(
            window_exists = self.window.window.is_some(),
            gpu_initialized = self.gpu_renderer.is_initialized(),
            has_surface = self.window.surface.is_some(),
            "releasing window resources"
        );
        self.window.surface = None;
        self.window.last_softbuffer_size = None;
        self.window.context = None;
        self.gpu_renderer = GpuRenderer::default();
        self.window.window_control = None;
        self.window.window_id = None;
        self.window.window = None;
        self.frame.redraw_in_flight = false;
        self.sync_deferred_gpu_init_state();
        debug!("window resources released");
    }

    pub(super) fn ensure_softbuffer_surface(&mut self) -> Result<()> {
        if self.window.surface.is_some() {
            return Ok(());
        }
        let window = self
            .window
            .window
            .as_ref()
            .ok_or_else(|| anyhow!("no window for softbuffer initialization"))?;
        let context = SoftbufferContext::new(window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer context: {error}"))?;
        let surface = SoftbufferSurface::new(&context, window.clone())
            .map_err(|error| anyhow!("failed to create softbuffer surface: {error}"))?;
        self.window.context = Some(context);
        self.window.surface = Some(surface);
        info!("lazily initialized softbuffer surface for CPU fallback");
        Ok(())
    }

    pub(super) fn apply_post_draw_visibility_handshake(&self) {
        if let Some(window) = self.window.window.as_ref() {
            window.set_visible(true);
            window.focus_window();
            let _ = self.request_window_redraw();
            info!("applied visibility handshake after first frame commit");
        }
    }

    pub(super) fn apply_window_extent_change(
        &mut self,
        event_loop: &ActiveEventLoop,
        raw_size: PhysicalSize<u32>,
        zero_size_message: &'static str,
        clamped_message: &'static str,
        monitor_event: MonitorAffectingWindowEvent,
    ) {
        if raw_size.width == 0 || raw_size.height == 0 {
            debug!(
                width = raw_size.width,
                height = raw_size.height,
                "{zero_size_message}"
            );
            return;
        }

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
                "{clamped_message}"
            );
        }

        self.window.window_size = capped_size;
        if self.is_gpu_lane_ready() {
            self.gpu_renderer.resize(
                self.window.window_size.width,
                self.window.window_size.height,
            );
        }
        self.update_viewport_geometry(event_loop);
        self.handle_monitor_affecting_event(monitor_event);
        self.queue_redraw();
    }

    pub(super) fn update_viewport_geometry(&mut self, event_loop: &ActiveEventLoop) {
        let raw_cols = ((self.window.window_size.width as usize) / CELL_WIDTH).max(1);
        let raw_rows = ((self.window.window_size.height as usize) / CELL_HEIGHT).max(1);
        let (cols, rows) = cap_terminal_geometry(raw_cols, raw_rows);
        let pixel_width = self.window.window_size.width.min(u16::MAX as u32) as u16;
        let pixel_height = self.window.window_size.height.min(u16::MAX as u32) as u16;
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

        if !viewport_geometry_changed(
            self.window.viewport_geometry,
            ViewportGeometry {
                cols,
                rows,
                pixel_width,
                pixel_height,
            },
        ) {
            trace!(
                cols,
                rows, pixel_width, pixel_height, "viewport: skipped (geometry unchanged)"
            );
            return;
        }

        debug!(
            cols,
            rows,
            pixel_width,
            pixel_height,
            width = self.window.window_size.width,
            height = self.window.window_size.height,
            "viewport: resizing"
        );

        self.terminal.resize(cols, rows);

        if let Err(error) = self.pty.resize(PtySize {
            cols,
            rows,
            pixel_width,
            pixel_height,
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
        self.window.viewport_geometry = ViewportGeometry {
            cols,
            rows,
            pixel_width,
            pixel_height,
        };

        if let Err(error) = self.mark_pty_boundary_recovered(SessionBoundary::PtyResize) {
            self.fatal_error = Some(error);
            event_loop.exit();
        }
    }

    pub(super) fn handle_close_requested(&mut self, event_loop: &ActiveEventLoop) {
        debug!("window close requested by user or compositor");
        self.emit_close_intent();
        self.exit_code.get_or_insert(0);
        event_loop.exit();
    }

    pub(super) fn handle_monitor_affecting_event(
        &mut self,
        monitor_event: MonitorAffectingWindowEvent,
    ) {
        let sampled_refresh_rate_millihz =
            sample_monitor_refresh_rate_millihz(self.window.window_control.as_deref());
        let command =
            cadence_resync_command_for_monitor_event(monitor_event, sampled_refresh_rate_millihz);

        match self.control.ui_runtime.handle_command(command) {
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
                    if let Err(error) = self.control.diagnostics.emit_runtime_command_receipt(
                        None,
                        RuntimeCommandSourceKind::MonitorEvent,
                        None,
                        &receipt,
                    ) {
                        warn!(
                            monitor_event = monitor_affecting_event_token(monitor_event),
                            sampled_refresh_rate_millihz =
                                sampled_refresh_rate_millihz.unwrap_or(0),
                            error = ?error,
                            "failed to emit typed cadence resync diagnostics"
                        );
                    }
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

    pub(super) fn request_window_redraw(&self) -> bool {
        if let Some(window_control) = self.window.window_control.as_ref() {
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

    pub(super) fn set_window_title(&self, title: &str) {
        if let Some(window_control) = self.window.window_control.as_ref() {
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

    pub(super) fn emit_close_intent(&self) {
        if let Some(window_control) = self.window.window_control.as_ref()
            && let Err(error) = window_control.close()
        {
            warn!(
                error = %error,
                "failed to propagate close intent via window control"
            );
        }
    }
}

pub(super) fn sample_monitor_refresh_rate_millihz(
    window_control: Option<&dyn WindowControl>,
) -> Option<u32> {
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

pub(super) fn cadence_resync_command_for_monitor_event(
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

pub(super) fn cap_terminal_geometry(raw_cols: usize, raw_rows: usize) -> (u16, u16) {
    let cols = raw_cols.clamp(1, MAX_VIEWPORT_COLS);
    let mut rows = raw_rows.clamp(1, MAX_VIEWPORT_ROWS);

    if cols.saturating_mul(rows) > MAX_VIEWPORT_CELLS {
        rows = (MAX_VIEWPORT_CELLS / cols.max(1)).max(1);
    }

    (cols as u16, rows as u16)
}

pub(super) fn viewport_geometry_changed(
    previous: ViewportGeometry,
    next: ViewportGeometry,
) -> bool {
    previous.cols != next.cols
        || previous.rows != next.rows
        || previous.pixel_width != next.pixel_width
        || previous.pixel_height != next.pixel_height
}

pub(super) fn cap_framebuffer_extent(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
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
