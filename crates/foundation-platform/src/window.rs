use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use rldyourterm_foundation::api::window::{
    MonitorTiming, WindowConfig, WindowControl, WindowEvent, WindowEventSink, WindowFactory,
};
use rldyourterm_foundation::error::{
    FoundationError, FoundationResult, Recoverability, WindowFailureCode, WindowOperation,
};
use rldyourterm_foundation::window::{MonitorTimingReading, WindowHealth};
use winit::event::WindowEvent as WinitWindowEvent;
use winit::monitor::MonitorHandle;
use winit::window::Window;

#[derive(Debug, Default)]
struct NoopWindowEventSink;

impl WindowEventSink for NoopWindowEventSink {
    fn on_event(&self, _event: WindowEvent) {}
}

#[derive(Debug, Default)]
struct WindowRuntimeState {
    title: Option<String>,
    redraw_pending: bool,
    redraw_event_queued: bool,
    closed: bool,
    last_position: Option<(i32, i32)>,
    last_size: Option<(u32, u32)>,
    last_scale_factor_bits: Option<u64>,
    last_display_refresh_timing: Option<Option<MonitorTiming>>,
    events: VecDeque<WindowEvent>,
}

pub struct PlatformWindowControl {
    window: Mutex<Option<Arc<Window>>>,
    cached_timing: Mutex<Option<MonitorTiming>>,
    clipboard_text: Mutex<String>,
    sink: Arc<dyn WindowEventSink>,
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
        let clipboard_len = self
            .clipboard_text
            .lock()
            .map(|text| text.len())
            .unwrap_or_default();

        match self.state.lock() {
            Ok(state) => f
                .debug_struct("PlatformWindowControl")
                .field("window_attached", &window_attached)
                .field("cached_timing", &cached_timing)
                .field("clipboard_len", &clipboard_len)
                .field("title", &state.title)
                .field("redraw_pending", &state.redraw_pending)
                .field("redraw_event_queued", &state.redraw_event_queued)
                .field("closed", &state.closed)
                .field("queued_events", &state.events.len())
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
        Self::with_sink(None, Arc::new(NoopWindowEventSink))
    }
}

impl PlatformWindowControl {
    pub fn detached() -> Self {
        Self::default()
    }

    pub fn from_winit_window(window: Arc<Window>) -> Self {
        Self::with_sink(Some(window), Arc::new(NoopWindowEventSink))
    }

    fn with_sink(window: Option<Arc<Window>>, sink: Arc<dyn WindowEventSink>) -> Self {
        Self {
            window: Mutex::new(window),
            cached_timing: Mutex::new(None),
            clipboard_text: Mutex::new(String::new()),
            sink,
            state: Mutex::new(WindowRuntimeState::default()),
        }
    }

    fn apply_config(&self, config: &WindowConfig) -> FoundationResult<()> {
        self.set_title_internal(&config.title)?;
        let _ = (
            config.width,
            config.height,
            config.min_width,
            config.min_height,
            config.high_dpi,
        );
        Ok(())
    }

    fn dispatch_events(&self, events: &[WindowEvent]) {
        for event in events {
            self.sink.on_event(event.clone());
        }
    }

    pub fn monitor_timing_on_window_event(
        &self,
        event: &WinitWindowEvent,
    ) -> FoundationResult<Option<MonitorTiming>> {
        // winit: redraw is the canonical OS/application redraw join point, and Moved can be
        // missing on Wayland, so cadence probes must include resize/scale/redraw transitions.
        let should_sample_timing = matches!(
            event,
            WinitWindowEvent::Moved(_)
                | WinitWindowEvent::Resized(_)
                | WinitWindowEvent::ScaleFactorChanged { .. }
                | WinitWindowEvent::RedrawRequested
        );

        if should_sample_timing {
            Ok(self.monitor_timing_reading()?.timing)
        } else {
            Ok(None)
        }
    }

    pub fn push_winit_event(&self, event: &WinitWindowEvent) -> FoundationResult<()> {
        let monitor_timing = self.monitor_timing_on_window_event(event)?;
        let mut dispatched_events = Vec::new();

        let mut state = self.lock_state(WindowOperation::PollSignals)?;
        let should_queue_display_refresh = match event {
            WinitWindowEvent::Moved(position) => {
                let moved_to = (position.x, position.y);
                if state.last_position != Some(moved_to) {
                    state.last_position = Some(moved_to);
                    Self::queue_event(
                        &mut state,
                        &mut dispatched_events,
                        WindowEvent::Moved {
                            x: position.x,
                            y: position.y,
                        },
                    );
                }
                true
            }
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let scale_factor_bits = scale_factor.to_bits();
                if state.last_scale_factor_bits != Some(scale_factor_bits) {
                    state.last_scale_factor_bits = Some(scale_factor_bits);
                    Self::queue_event(
                        &mut state,
                        &mut dispatched_events,
                        WindowEvent::ScaleFactorChanged {
                            scale: *scale_factor,
                        },
                    );
                }
                true
            }
            WinitWindowEvent::Resized(size) => {
                let resized_to = (size.width, size.height);
                if state.last_size != Some(resized_to) {
                    state.last_size = Some(resized_to);
                    Self::queue_event(
                        &mut state,
                        &mut dispatched_events,
                        WindowEvent::Resized {
                            width: size.width,
                            height: size.height,
                            cols: 0,
                            rows: 0,
                        },
                    );
                }
                true
            }
            WinitWindowEvent::RedrawRequested => {
                state.redraw_pending = false;
                Self::queue_redraw_event(&mut state, &mut dispatched_events);
                true
            }
            WinitWindowEvent::Focused(focused) => {
                Self::queue_event(
                    &mut state,
                    &mut dispatched_events,
                    WindowEvent::Focused(*focused),
                );
                false
            }
            WinitWindowEvent::CloseRequested => {
                Self::queue_event(
                    &mut state,
                    &mut dispatched_events,
                    WindowEvent::CloseRequested,
                );
                false
            }
            _ => false,
        };

        if should_queue_display_refresh {
            Self::queue_display_refresh_event(&mut state, &mut dispatched_events, monitor_timing);
        }
        drop(state);
        self.dispatch_events(&dispatched_events);
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

    fn queue_display_refresh_event(
        state: &mut WindowRuntimeState,
        dispatched_events: &mut Vec<WindowEvent>,
        timing: Option<MonitorTiming>,
    ) {
        if state.last_display_refresh_timing.as_ref() != Some(&timing) {
            state.last_display_refresh_timing = Some(timing.clone());
            let refresh_rate_millihz = timing
                .as_ref()
                .and_then(|current| current.refresh_rate_millihz);
            let monitor_name = timing
                .as_ref()
                .and_then(|current| current.monitor_name.clone());
            Self::queue_event(
                state,
                dispatched_events,
                WindowEvent::DisplayRefreshChanged {
                    refresh_rate_millihz,
                    monitor_name,
                },
            );
        }
    }

    fn queue_redraw_event(
        state: &mut WindowRuntimeState,
        dispatched_events: &mut Vec<WindowEvent>,
    ) {
        if state.redraw_event_queued {
            return;
        }
        state.redraw_event_queued = true;
        Self::queue_event(state, dispatched_events, WindowEvent::RedrawRequested);
    }

    fn queue_event(
        state: &mut WindowRuntimeState,
        dispatched_events: &mut Vec<WindowEvent>,
        event: WindowEvent,
    ) {
        state.events.push_back(event.clone());
        dispatched_events.push(event);
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

    fn lock_clipboard_text(
        &self,
        operation: WindowOperation,
    ) -> FoundationResult<MutexGuard<'_, String>> {
        self.clipboard_text.lock().map_err(|_| {
            FoundationError::window(
                operation,
                WindowFailureCode::BoundaryFault,
                Recoverability::Fatal,
                "window clipboard lock poisoned",
                None,
            )
        })
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
            let mut state = self.lock_state(WindowOperation::RequestRedraw)?;
            if state.closed {
                return Err(FoundationError::window(
                    WindowOperation::RequestRedraw,
                    WindowFailureCode::InvalidWindowState,
                    Recoverability::Degrade,
                    "request_redraw called after window close",
                    None,
                ));
            }

            if !state.redraw_pending && !state.redraw_event_queued {
                state.redraw_pending = true;
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

    fn clipboard_text(&self) -> FoundationResult<String> {
        self.lock_clipboard_text(WindowOperation::PollSignals)
            .map(|text| text.clone())
    }

    fn set_clipboard_text(&self, text: &str) -> FoundationResult<()> {
        self.lock_clipboard_text(WindowOperation::PollSignals)
            .map(|mut current| {
                *current = text.to_string();
            })
    }

    fn close(&self) -> FoundationResult<()> {
        let mut dispatched_events = Vec::new();
        {
            let mut state = self.lock_state(WindowOperation::Close)?;
            if state.closed {
                return Ok(());
            }

            state.closed = true;
            Self::queue_event(
                &mut state,
                &mut dispatched_events,
                WindowEvent::CloseRequested,
            );
        }
        self.clear_window(WindowOperation::Close)?;
        self.dispatch_events(&dispatched_events);
        Ok(())
    }

    fn poll_events(&self) -> FoundationResult<Vec<WindowEvent>> {
        let mut dispatched_events = Vec::new();
        let mut state = self.lock_state(WindowOperation::PollSignals)?;
        if state.redraw_pending && !state.redraw_event_queued {
            Self::queue_redraw_event(&mut state, &mut dispatched_events);
            state.redraw_pending = false;
        }

        let has_redraw_event = state
            .events
            .iter()
            .any(|event| matches!(event, WindowEvent::RedrawRequested));
        let events: Vec<_> = state.events.drain(..).collect();
        if has_redraw_event {
            state.redraw_event_queued = false;
        }
        drop(state);
        self.dispatch_events(&dispatched_events);
        Ok(events)
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
    fn init(
        &self,
        config: WindowConfig,
        sink: Box<dyn WindowEventSink>,
    ) -> FoundationResult<Box<dyn WindowControl>> {
        let control =
            PlatformWindowControl::with_sink(self.window.as_ref().map(Arc::clone), Arc::from(sink));
        control.apply_config(&config)?;
        Ok(Box::new(control))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn request_redraw_is_coalesced_until_polled() {
        let control = PlatformWindowControl::detached();

        control.request_redraw().expect("first redraw request");
        control.request_redraw().expect("second redraw request");

        let events = control.poll_events().expect("poll after redraw requests");
        assert_eq!(events, vec![WindowEvent::RedrawRequested]);
        assert!(control.poll_events().expect("poll idle").is_empty());
    }

    #[test]
    fn moved_event_emits_refresh_change_once_per_timing_sample() {
        let control = PlatformWindowControl::detached();

        control
            .push_winit_event(&WinitWindowEvent::Moved(PhysicalPosition::new(10, 20)))
            .expect("first moved event");
        assert_eq!(
            control.poll_events().expect("poll first moved"),
            vec![
                WindowEvent::Moved { x: 10, y: 20 },
                WindowEvent::DisplayRefreshChanged {
                    refresh_rate_millihz: None,
                    monitor_name: None,
                },
            ]
        );

        control
            .push_winit_event(&WinitWindowEvent::Moved(PhysicalPosition::new(11, 20)))
            .expect("second moved event");
        assert_eq!(
            control.poll_events().expect("poll second moved"),
            vec![WindowEvent::Moved { x: 11, y: 20 }]
        );

        control
            .push_winit_event(&WinitWindowEvent::Resized(PhysicalSize::new(1280, 720)))
            .expect("resize event");
        assert_eq!(
            control.poll_events().expect("poll resize"),
            vec![WindowEvent::Resized {
                width: 1280,
                height: 720,
                cols: 0,
                rows: 0,
            }]
        );
    }

    #[test]
    fn close_is_idempotent_and_blocks_followup_redraw_requests() {
        let control = PlatformWindowControl::detached();

        control.close().expect("first close");
        assert_eq!(
            control.poll_events().expect("poll close"),
            vec![WindowEvent::CloseRequested]
        );

        control.close().expect("second close");
        assert!(
            control
                .poll_events()
                .expect("poll after second close")
                .is_empty()
        );

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
    fn clipboard_round_trip_is_deterministic() {
        let control = PlatformWindowControl::detached();
        control
            .set_clipboard_text("wave3")
            .expect("set clipboard text");
        assert_eq!(
            control.clipboard_text().expect("get clipboard text"),
            "wave3"
        );
    }
}
