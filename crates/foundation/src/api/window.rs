use crate::api::common::ContractResult;
pub use crate::api::common::MonitorTiming;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub high_dpi: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowInput {
    Text {
        text: String,
    },
    Key {
        key_code: u32,
        name: String,
        pressed: bool,
        repeat: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    CloseRequested,
    Moved {
        x: i32,
        y: i32,
    },
    Resized {
        width: u32,
        height: u32,
        cols: u16,
        rows: u16,
    },
    ScaleFactorChanged {
        scale: f64,
    },
    DisplayRefreshChanged {
        refresh_rate_millihz: Option<u32>,
        monitor_name: Option<String>,
    },
    RedrawRequested,
    Focused(bool),
    ModifierChanged {
        shift: bool,
        ctrl: bool,
        alt: bool,
        logo: bool,
    },
    Keyboard {
        input: WindowInput,
    },
    MouseWheel {
        delta_x: f32,
        delta_y: f32,
    },
    MouseMove {
        x: f64,
        y: f64,
    },
    MouseButton {
        button: u8,
        pressed: bool,
    },
}

pub trait WindowEventSink: Send + Sync {
    fn on_event(&self, event: WindowEvent);
}

pub trait WindowControl: Send + Sync {
    fn request_redraw(&self) -> ContractResult<()>;
    fn set_title(&self, title: &str) -> ContractResult<()>;
    fn current_monitor_timing(&self) -> ContractResult<MonitorTiming>;
    fn clipboard_text(&self) -> ContractResult<String>;
    fn set_clipboard_text(&self, text: &str) -> ContractResult<()>;
    fn close(&self) -> ContractResult<()>;
    fn poll_events(&self) -> ContractResult<Vec<WindowEvent>>;
}

pub trait WindowFactory: Send + Sync {
    fn init(
        &self,
        config: WindowConfig,
        sink: Box<dyn WindowEventSink>,
    ) -> ContractResult<Box<dyn WindowControl>>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowSignal {
    RedrawRequested,
    Moved { x: i32, y: i32 },
    Resized { size: WindowSize },
    ScaleFactorChanged { scale_factor: f64 },
    DisplayRefreshChanged { timing: Option<MonitorTiming> },
    FocusChanged { focused: bool },
    CloseRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

#[deprecated(note = "Use WindowEvent and WindowControl contracts.")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedrawRequestOutcome {
    Queued,
    AlreadyQueued,
}

#[deprecated(note = "Use WindowEvent and WindowControl contracts.")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTitleOutcome {
    Updated,
    Unchanged,
}

#[deprecated(note = "Use WindowControl::current_monitor_timing.")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorTimingQueryOutcome {
    Available(MonitorTiming),
    Unavailable,
}

#[deprecated(note = "Use WindowControl::poll_events.")]
#[derive(Debug, Clone, PartialEq)]
pub enum WindowPollOutcome {
    Signals(Vec<WindowSignal>),
    Idle,
}

#[deprecated(note = "Use WindowControl::close.")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCloseOutcome {
    Closed,
    AlreadyClosed,
}

#[deprecated(note = "Use WindowFactory/WindowControl contracts.")]
#[allow(deprecated)]
pub trait WindowAdapter: Send {
    fn request_redraw(&self) -> ContractResult<RedrawRequestOutcome>;
    fn set_title(&self, title: &str) -> ContractResult<WindowTitleOutcome>;
    fn current_monitor_timing(&self) -> ContractResult<MonitorTimingQueryOutcome>;
    fn poll_signals(&mut self) -> ContractResult<WindowPollOutcome>;
    fn close(&mut self) -> ContractResult<WindowCloseOutcome>;
}
