use crate::api::common::{ContractResult, CorrelationId};
use crate::error::RuntimeLayer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEventId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Trace,
    Info,
    Warn,
    Error,
    Fatal,
}

pub type DiagnosticLayer = RuntimeLayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    SessionStarted,
    SessionEnded,
    SessionError,
    PtyError,
    RenderModeTransition,
    DisplayRefreshChanged,
    RenderCadenceUpdated,
    SettingsApply,
    SettingsRejected,
    ShellResolved,
    ShellResolutionFailed,
    ShellFallbackApplied,
    ShellLaunchPlanned,
    Resize,
    ResourceWarning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEvent {
    pub event_id: DiagnosticEventId,
    pub kind: DiagnosticKind,
    pub severity: DiagnosticSeverity,
    pub layer: DiagnosticLayer,
    pub correlation_id: Option<CorrelationId>,
    pub message: String,
    pub payload_json: Option<String>,
    pub timestamp_ms: u64,
}

impl DiagnosticEvent {
    pub fn new(
        event_id: impl Into<String>,
        kind: DiagnosticKind,
        severity: DiagnosticSeverity,
        layer: DiagnosticLayer,
        message: impl Into<String>,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            event_id: DiagnosticEventId(event_id.into()),
            kind,
            severity,
            layer,
            correlation_id: None,
            message: message.into(),
            payload_json: None,
            timestamp_ms,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(CorrelationId(correlation_id.into()));
        self
    }

    pub fn with_payload_json(mut self, payload_json: impl Into<String>) -> Self {
        self.payload_json = Some(payload_json.into());
        self
    }
}

pub trait DiagnosticSink: Send + Sync {
    fn emit(&self, event: DiagnosticEvent) -> ContractResult<()>;
    fn flush(&self) -> ContractResult<()>;
}

pub trait DiagnosticConfig: Send + Sync {
    fn is_enabled(&self) -> bool;
    fn is_debug_mode(&self) -> bool;
}
