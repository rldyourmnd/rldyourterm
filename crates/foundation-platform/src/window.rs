// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use rldyourterm_foundation::api::window::{
    MonitorTiming, WindowConfig, WindowControl, WindowFactory,
};
use rldyourterm_foundation::error::{
    FoundationError, FoundationResult, Recoverability, WindowFailureCode, WindowOperation,
};
use rldyourterm_foundation::window::{MonitorTimingReading, WindowHealth};
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};
use winit::dpi::{LogicalSize, PhysicalSize, Size};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use winit::platform::startup_notify::{
    EventLoopExtStartupNotify, WindowAttributesExtStartupNotify,
};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use winit::platform::wayland::WindowAttributesExtWayland;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use winit::platform::x11::WindowAttributesExtX11;
use winit::window::{Icon, Window, WindowAttributes, WindowId};

pub type PlatformSoftbufferContext = SoftbufferContext<Arc<Window>>;
pub type PlatformSoftbufferSurface = SoftbufferSurface<Arc<Window>, Arc<Window>>;

#[derive(Debug, Default)]
struct WindowRuntimeState {
    title: Option<String>,
    closed: bool,
}

#[derive(Default)]
struct PlatformWindowInner {
    window: Option<Arc<Window>>,
    cached_timing: Option<MonitorTiming>,
    state: WindowRuntimeState,
}

pub struct PlatformWindowControl {
    inner: Mutex<PlatformWindowInner>,
}

pub struct PlatformWindowHost {
    window: Arc<Window>,
    control: Box<dyn WindowControl>,
    surface: Option<PlatformSoftbufferSurface>,
    context: Option<PlatformSoftbufferContext>,
}

impl fmt::Debug for PlatformWindowHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlatformWindowHost")
            .field("window_id", &self.window.id())
            .field("has_cpu_surface", &self.surface.is_some())
            .finish()
    }
}

impl fmt::Debug for PlatformWindowControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.lock() {
            Ok(inner) => f
                .debug_struct("PlatformWindowControl")
                .field("window_attached", &inner.window.is_some())
                .field("cached_timing", &inner.cached_timing)
                .field("title", &inner.state.title)
                .field("closed", &inner.state.closed)
                .finish(),
            Err(_) => f
                .debug_struct("PlatformWindowControl")
                .field("state", &"poisoned")
                .finish(),
        }
    }
}

impl Default for PlatformWindowControl {
    fn default() -> Self {
        Self::new(None)
    }
}

impl PlatformWindowControl {
    pub fn detached() -> Self {
        Self::default()
    }

    pub fn from_winit_window(window: Arc<Window>) -> Self {
        Self::new(Some(window))
    }

    fn new(window: Option<Arc<Window>>) -> Self {
        Self {
            inner: Mutex::new(PlatformWindowInner {
                window,
                cached_timing: None,
                state: WindowRuntimeState::default(),
            }),
        }
    }

    fn lock_inner(
        &self,
        operation: WindowOperation,
    ) -> FoundationResult<MutexGuard<'_, PlatformWindowInner>> {
        self.inner.lock().map_err(|_| {
            FoundationError::window(
                operation,
                WindowFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "window control lock poisoned",
                None,
            )
        })
    }

    fn apply_config(&self, config: &WindowConfig) -> FoundationResult<()> {
        let mut inner = self.lock_inner(WindowOperation::SetTitle)?;
        if inner.state.closed {
            return Err(FoundationError::window(
                WindowOperation::SetTitle,
                WindowFailureCode::InvalidWindowState,
                Recoverability::Degrade,
                "set_title called after window close",
                None,
            ));
        }

        inner.state.title = Some(config.title.clone());

        if let Some(window) = inner.window.as_ref() {
            window.set_title(&config.title);
            window.set_min_inner_size(window_size(
                config.min_width,
                config.min_height,
                config.high_dpi,
            ));
            if let Some(size) = window_size(config.width, config.height, config.high_dpi) {
                let _ = window.request_inner_size(size);
            }
        }
        Ok(())
    }

    pub fn monitor_timing_reading(&self) -> FoundationResult<MonitorTimingReading> {
        let reading = self.resolve_monitor_timing()?;
        if reading.health == WindowHealth::Degraded {
            tracing::debug!(
                source = ?reading.source,
                has_timing = reading.timing.is_some(),
                "window monitor timing resolved with degraded health"
            );
        }
        Ok(reading)
    }

    fn resolve_monitor_timing(&self) -> FoundationResult<MonitorTimingReading> {
        let mut inner = self.lock_inner(WindowOperation::QueryMonitorTiming)?;

        if let Some(window) = inner.window.as_ref() {
            if let Some(monitor) = window.current_monitor() {
                let current_timing = Self::timing_from_monitor(&monitor);
                if current_timing.refresh_rate_millihz.is_some() {
                    inner.cached_timing = Some(current_timing.clone());
                    return Ok(MonitorTimingReading::from_current_monitor(current_timing));
                }

                if let Some(primary_monitor) = window.primary_monitor() {
                    let primary_timing = Self::timing_from_monitor(&primary_monitor);
                    if primary_timing.refresh_rate_millihz.is_some() {
                        inner.cached_timing = Some(primary_timing.clone());
                        return Ok(MonitorTimingReading::from_primary_monitor_fallback(
                            primary_timing,
                        ));
                    }
                }

                if let Some(cached_timing) = inner.cached_timing.as_ref() {
                    let merged = MonitorTiming {
                        monitor_name: current_timing.monitor_name,
                        refresh_rate_millihz: cached_timing.refresh_rate_millihz,
                    };
                    return Ok(MonitorTimingReading::from_cached_fallback(merged));
                }

                return Ok(MonitorTimingReading::from_current_monitor(current_timing));
            }

            if let Some(monitor) = window.primary_monitor() {
                return Ok(Self::resolve_from_monitor_inner(&mut inner, monitor, false));
            }
        }

        if let Some(cached_timing) = inner.cached_timing.as_ref() {
            return Ok(MonitorTimingReading::from_cached_fallback(
                cached_timing.clone(),
            ));
        }

        Ok(MonitorTimingReading::unavailable())
    }

    fn resolve_from_monitor_inner(
        inner: &mut PlatformWindowInner,
        monitor: MonitorHandle,
        is_current: bool,
    ) -> MonitorTimingReading {
        let timing = Self::timing_from_monitor(&monitor);
        if timing.refresh_rate_millihz.is_some() {
            inner.cached_timing = Some(timing.clone());
            return if is_current {
                MonitorTimingReading::from_current_monitor(timing)
            } else {
                MonitorTimingReading::from_primary_monitor_fallback(timing)
            };
        }

        if let Some(cached) = inner.cached_timing.as_ref() {
            let merged = MonitorTiming {
                monitor_name: timing.monitor_name,
                refresh_rate_millihz: cached.refresh_rate_millihz,
            };
            return MonitorTimingReading::from_cached_fallback(merged);
        }

        if is_current {
            MonitorTimingReading::from_current_monitor(timing)
        } else {
            MonitorTimingReading::from_primary_monitor_fallback(timing)
        }
    }

    fn timing_from_monitor(monitor: &MonitorHandle) -> MonitorTiming {
        MonitorTiming {
            monitor_name: monitor.name(),
            refresh_rate_millihz: monitor.refresh_rate_millihertz(),
        }
    }

    fn set_title_internal(&self, title: &str) -> FoundationResult<()> {
        let mut inner = self.lock_inner(WindowOperation::SetTitle)?;
        if inner.state.closed {
            return Err(FoundationError::window(
                WindowOperation::SetTitle,
                WindowFailureCode::InvalidWindowState,
                Recoverability::Degrade,
                "set_title called after window close",
                None,
            ));
        }

        if inner.state.title.as_deref() == Some(title) {
            return Ok(());
        }
        inner.state.title = Some(title.to_string());

        if let Some(window) = inner.window.as_ref() {
            window.set_title(title);
        } else {
            tracing::debug!("set_title called on detached window control");
        }
        Ok(())
    }
}

impl PlatformWindowHost {
    pub fn create_gui_window(
        event_loop: &ActiveEventLoop,
        config: WindowConfig,
        application_id: &str,
        icon: Option<Icon>,
    ) -> FoundationResult<Self> {
        let attributes = build_gui_window_attributes(event_loop, &config, application_id, icon);
        let window = Arc::new(event_loop.create_window(attributes).map_err(|error| {
            FoundationError::window(
                WindowOperation::CreateWindow,
                WindowFailureCode::EventLoopUnavailable,
                Recoverability::Fatal,
                format!("failed to create GUI window: {error}"),
                None,
            )
        })?);
        window.set_ime_allowed(false);

        let control = PlatformWindowFactory::from_winit_window(window.clone()).init(config)?;
        let (context, surface) = Self::create_cpu_surface_lane(&window)?;

        Ok(Self {
            window,
            control,
            surface: Some(surface),
            context: Some(context),
        })
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn cloned_window(&self) -> Arc<Window> {
        Arc::clone(&self.window)
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn control(&self) -> &dyn WindowControl {
        self.control.as_ref()
    }

    pub fn has_cpu_surface(&self) -> bool {
        self.surface.is_some()
    }

    pub fn ensure_cpu_surface(&mut self) -> FoundationResult<()> {
        if self.surface.is_some() {
            return Ok(());
        }

        let (context, surface) = Self::create_cpu_surface_lane(&self.window)?;
        self.surface = Some(surface);
        self.context = Some(context);
        Ok(())
    }

    pub fn drop_cpu_surface_lane(&mut self) {
        self.surface = None;
        self.context = None;
    }

    pub fn surface_mut(&mut self) -> Option<&mut PlatformSoftbufferSurface> {
        self.surface.as_mut()
    }

    fn create_cpu_surface_lane(
        window: &Arc<Window>,
    ) -> FoundationResult<(PlatformSoftbufferContext, PlatformSoftbufferSurface)> {
        let context = SoftbufferContext::new(window.clone()).map_err(|error| {
            FoundationError::window(
                WindowOperation::InitializeSurface,
                WindowFailureCode::Unsupported,
                Recoverability::Degrade,
                format!("failed to create softbuffer context: {error}"),
                None,
            )
        })?;
        let surface = SoftbufferSurface::new(&context, window.clone()).map_err(|error| {
            FoundationError::window(
                WindowOperation::InitializeSurface,
                WindowFailureCode::Unsupported,
                Recoverability::Degrade,
                format!("failed to create softbuffer surface: {error}"),
                None,
            )
        })?;
        Ok((context, surface))
    }
}

impl WindowControl for PlatformWindowControl {
    fn request_redraw(&self) -> FoundationResult<()> {
        let inner = self.lock_inner(WindowOperation::RequestRedraw)?;
        if inner.state.closed {
            return Err(FoundationError::window(
                WindowOperation::RequestRedraw,
                WindowFailureCode::InvalidWindowState,
                Recoverability::Degrade,
                "request_redraw called after window close",
                None,
            ));
        }

        if let Some(window) = inner.window.as_ref() {
            window.request_redraw();
        } else {
            tracing::debug!("request_redraw called on detached window control");
        }
        Ok(())
    }

    fn set_title(&self, title: &str) -> FoundationResult<()> {
        self.set_title_internal(title)
    }

    fn current_monitor_timing(&self) -> FoundationResult<MonitorTiming> {
        Ok(self
            .monitor_timing_reading()?
            .timing
            .unwrap_or(MonitorTiming {
                monitor_name: None,
                refresh_rate_millihz: None,
            }))
    }

    fn close(&self) -> FoundationResult<()> {
        let mut inner = self.lock_inner(WindowOperation::Close)?;
        if inner.state.closed {
            return Ok(());
        }
        inner.state.closed = true;
        inner.window = None;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct PlatformWindowFactory {
    window: Option<Arc<Window>>,
}

impl PlatformWindowFactory {
    pub fn detached() -> Self {
        Self::default()
    }

    pub fn from_winit_window(window: Arc<Window>) -> Self {
        Self {
            window: Some(window),
        }
    }
}

impl WindowFactory for PlatformWindowFactory {
    fn init(&self, config: WindowConfig) -> FoundationResult<Box<dyn WindowControl>> {
        let control = PlatformWindowControl::new(self.window.as_ref().map(Arc::clone));
        control.apply_config(&config)?;
        Ok(Box::new(control))
    }
}

fn build_gui_window_attributes(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
    application_id: &str,
    icon: Option<Icon>,
) -> WindowAttributes {
    #[allow(unused_mut)]
    let mut attributes = Window::default_attributes()
        .with_title(config.title.clone())
        .with_visible(true)
        .with_active(true);

    if let Some(size) = window_size(config.width, config.height, config.high_dpi) {
        attributes = attributes.with_inner_size(size);
    }
    if let Some(icon) = icon {
        attributes = attributes.with_window_icon(Some(icon));
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        attributes =
            WindowAttributesExtWayland::with_name(attributes, application_id, application_id);
        attributes = WindowAttributesExtX11::with_name(attributes, application_id, application_id);
        if let Some(token) = event_loop.read_token_from_env() {
            attributes = attributes.with_activation_token(token);
        }
    }

    attributes
}

fn window_size(width: u32, height: u32, high_dpi: bool) -> Option<Size> {
    if width == 0 || height == 0 {
        return None;
    }

    if high_dpi {
        Some(Size::Logical(LogicalSize::new(
            f64::from(width),
            f64::from(height),
        )))
    } else {
        Some(Size::Physical(PhysicalSize::new(width, height)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_is_idempotent_and_blocks_followup_redraw_requests() {
        let control = PlatformWindowControl::detached();

        control.close().expect("first close");
        control.close().expect("second close");

        let error = control
            .request_redraw()
            .expect_err("request redraw must fail after close");
        match error {
            FoundationError::Window { code, .. } => {
                assert_eq!(code, WindowFailureCode::InvalidWindowState);
            }
            other => panic!("expected window error, got {other:?}"),
        }
    }

    #[test]
    fn apply_config_uses_high_dpi_as_logical_size_policy() {
        assert_eq!(
            window_size(640, 480, true),
            Some(Size::Logical(LogicalSize::new(640.0, 480.0)))
        );
        assert_eq!(
            window_size(640, 480, false),
            Some(Size::Physical(PhysicalSize::new(640, 480)))
        );
        assert_eq!(window_size(0, 480, true), None);
        assert_eq!(window_size(640, 0, false), None);
    }

    #[test]
    fn detached_control_reports_unavailable_monitor_timing() {
        let control = PlatformWindowControl::detached();

        assert_eq!(
            control
                .current_monitor_timing()
                .expect("timing must resolve"),
            MonitorTiming {
                monitor_name: None,
                refresh_rate_millihz: None,
            }
        );
    }
}
