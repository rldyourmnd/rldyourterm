// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use rldyourterm_foundation::api::window::{
    MonitorTiming, WindowConfig, WindowControl, WindowFactory,
};
use rldyourterm_foundation::error::{
    FoundationError, FoundationResult, Recoverability, WindowFailureCode, WindowOperation,
};
use rldyourterm_foundation::window::{MonitorTimingReading, WindowHealth};
use winit::dpi::{LogicalSize, PhysicalSize, Size};
use winit::monitor::MonitorHandle;
use winit::window::Window;

#[derive(Debug, Default)]
struct WindowRuntimeState {
    title: Option<String>,
    closed: bool,
}

pub struct PlatformWindowControl {
    window: Mutex<Option<Arc<Window>>>,
    cached_timing: Mutex<Option<MonitorTiming>>,
    state: Mutex<WindowRuntimeState>,
}

impl fmt::Debug for PlatformWindowControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let window_attached = self
            .window
            .lock()
            .map(|window| window.is_some())
            .unwrap_or(false);
        let cached_timing = self
            .cached_timing
            .lock()
            .map(|cached| cached.clone())
            .ok()
            .flatten();

        match self.state.lock() {
            Ok(state) => f
                .debug_struct("PlatformWindowControl")
                .field("window_attached", &window_attached)
                .field("cached_timing", &cached_timing)
                .field("title", &state.title)
                .field("closed", &state.closed)
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
            window: Mutex::new(window),
            cached_timing: Mutex::new(None),
            state: Mutex::new(WindowRuntimeState::default()),
        }
    }

    fn apply_config(&self, config: &WindowConfig) -> FoundationResult<()> {
        self.set_title_internal(&config.title)?;
        if let Some(window) = self.clone_window(WindowOperation::SetTitle)? {
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
        if let Some(window) = self.clone_window(WindowOperation::QueryMonitorTiming)? {
            if let Some(monitor) = window.current_monitor() {
                let current_timing = Self::timing_from_monitor(&monitor);
                if current_timing.refresh_rate_millihz.is_some() {
                    self.write_cached_timing(Some(current_timing.clone()))?;
                    return Ok(MonitorTimingReading::from_current_monitor(current_timing));
                }

                if let Some(primary_monitor) = window.primary_monitor() {
                    let primary_timing = Self::timing_from_monitor(&primary_monitor);
                    if primary_timing.refresh_rate_millihz.is_some() {
                        self.write_cached_timing(Some(primary_timing.clone()))?;
                        return Ok(MonitorTimingReading::from_primary_monitor_fallback(
                            primary_timing,
                        ));
                    }
                }

                if let Some(cached_timing) = self.read_cached_timing()? {
                    let merged = MonitorTiming {
                        monitor_name: current_timing.monitor_name,
                        refresh_rate_millihz: cached_timing.refresh_rate_millihz,
                    };
                    return Ok(MonitorTimingReading::from_cached_fallback(merged));
                }

                return Ok(MonitorTimingReading::from_current_monitor(current_timing));
            }

            if let Some(monitor) = window.primary_monitor() {
                return self.resolve_from_monitor(monitor, false);
            }
        }

        if let Some(cached_timing) = self.read_cached_timing()? {
            return Ok(MonitorTimingReading::from_cached_fallback(cached_timing));
        }

        Ok(MonitorTimingReading::unavailable())
    }

    fn resolve_from_monitor(
        &self,
        monitor: MonitorHandle,
        is_current: bool,
    ) -> FoundationResult<MonitorTimingReading> {
        let timing = Self::timing_from_monitor(&monitor);
        if timing.refresh_rate_millihz.is_some() {
            self.write_cached_timing(Some(timing.clone()))?;
            return Ok(if is_current {
                MonitorTimingReading::from_current_monitor(timing)
            } else {
                MonitorTimingReading::from_primary_monitor_fallback(timing)
            });
        }

        if let Some(cached) = self.read_cached_timing()? {
            let merged = MonitorTiming {
                monitor_name: timing.monitor_name,
                refresh_rate_millihz: cached.refresh_rate_millihz,
            };
            return Ok(MonitorTimingReading::from_cached_fallback(merged));
        }

        Ok(if is_current {
            MonitorTimingReading::from_current_monitor(timing)
        } else {
            MonitorTimingReading::from_primary_monitor_fallback(timing)
        })
    }

    fn timing_from_monitor(monitor: &MonitorHandle) -> MonitorTiming {
        MonitorTiming {
            monitor_name: monitor.name(),
            refresh_rate_millihz: monitor.refresh_rate_millihertz(),
        }
    }

    fn lock_state(
        &self,
        operation: WindowOperation,
    ) -> FoundationResult<MutexGuard<'_, WindowRuntimeState>> {
        self.state.lock().map_err(|_| {
            FoundationError::window(
                operation,
                WindowFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "window runtime state lock poisoned",
                None,
            )
        })
    }

    fn clone_window(&self, operation: WindowOperation) -> FoundationResult<Option<Arc<Window>>> {
        self.window
            .lock()
            .map(|window| window.as_ref().map(Arc::clone))
            .map_err(|_| {
                FoundationError::window(
                    operation,
                    WindowFailureCode::BoundaryFault,
                    Recoverability::Fatal,
                    "window handle lock poisoned",
                    None,
                )
            })
    }

    fn clear_window(&self, operation: WindowOperation) -> FoundationResult<()> {
        self.window
            .lock()
            .map(|mut window| {
                *window = None;
            })
            .map_err(|_| {
                FoundationError::window(
                    operation,
                    WindowFailureCode::BoundaryFault,
                    Recoverability::Fatal,
                    "window handle lock poisoned",
                    None,
                )
            })
    }

    fn set_title_internal(&self, title: &str) -> FoundationResult<()> {
        {
            let mut state = self.lock_state(WindowOperation::SetTitle)?;
            if state.closed {
                return Err(FoundationError::window(
                    WindowOperation::SetTitle,
                    WindowFailureCode::InvalidWindowState,
                    Recoverability::Degrade,
                    "set_title called after window close",
                    None,
                ));
            }

            if state.title.as_deref() == Some(title) {
                return Ok(());
            }
            state.title = Some(title.to_string());
        }

        if let Some(window) = self.clone_window(WindowOperation::SetTitle)? {
            window.set_title(title);
        } else {
            tracing::debug!("set_title called on detached window control");
        }
        Ok(())
    }

    fn read_cached_timing(&self) -> FoundationResult<Option<MonitorTiming>> {
        self.cached_timing
            .lock()
            .map(|cached| cached.clone())
            .map_err(|_| {
                FoundationError::window(
                    WindowOperation::QueryMonitorTiming,
                    WindowFailureCode::BoundaryFault,
                    Recoverability::Fatal,
                    "monitor timing cache lock poisoned",
                    None,
                )
            })
    }

    fn write_cached_timing(&self, timing: Option<MonitorTiming>) -> FoundationResult<()> {
        self.cached_timing
            .lock()
            .map(|mut cached| {
                *cached = timing;
            })
            .map_err(|_| {
                FoundationError::window(
                    WindowOperation::QueryMonitorTiming,
                    WindowFailureCode::BoundaryFault,
                    Recoverability::Fatal,
                    "monitor timing cache lock poisoned",
                    None,
                )
            })
    }
}

impl WindowControl for PlatformWindowControl {
    fn request_redraw(&self) -> FoundationResult<()> {
        {
            let state = self.lock_state(WindowOperation::RequestRedraw)?;
            if state.closed {
                return Err(FoundationError::window(
                    WindowOperation::RequestRedraw,
                    WindowFailureCode::InvalidWindowState,
                    Recoverability::Degrade,
                    "request_redraw called after window close",
                    None,
                ));
            }
        }

        if let Some(window) = self.clone_window(WindowOperation::RequestRedraw)? {
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
        {
            let mut state = self.lock_state(WindowOperation::Close)?;
            if state.closed {
                return Ok(());
            }
            state.closed = true;
        }
        self.clear_window(WindowOperation::Close)
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
