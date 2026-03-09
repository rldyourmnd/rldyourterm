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

pub trait WindowControl: Send + Sync {
    fn request_redraw(&self) -> ContractResult<()>;
    fn set_title(&self, title: &str) -> ContractResult<()>;
    fn current_monitor_timing(&self) -> ContractResult<MonitorTiming>;
    fn close(&self) -> ContractResult<()>;
}

pub trait WindowFactory: Send + Sync {
    fn init(&self, config: WindowConfig) -> ContractResult<Box<dyn WindowControl>>;
}
