pub type FoundationResult<T> = std::result::Result<T, super::FoundationError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for CorrelationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for CorrelationId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recoverability {
    Retryable,
    Degrade,
    Fatal,
}

impl Recoverability {
    pub const fn is_recoverable(self) -> bool {
        !matches!(self, Self::Fatal)
    }

    pub const fn default_severity(self) -> ErrorSeverity {
        match self {
            Self::Retryable => ErrorSeverity::Low,
            Self::Degrade => ErrorSeverity::Medium,
            Self::Fatal => ErrorSeverity::Fatal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLayer {
    Core,
    FoundationPty,
    FoundationWindow,
    FoundationClipboard,
    FoundationDiagnostics,
    Services,
    Features,
    Ui,
    App,
}

impl RuntimeLayer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::FoundationPty => "foundation-pty",
            Self::FoundationWindow => "foundation-window",
            Self::FoundationClipboard => "foundation-clipboard",
            Self::FoundationDiagnostics => "foundation-diagnostics",
            Self::Services => "services",
            Self::Features => "features",
            Self::Ui => "ui",
            Self::App => "app",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundationBoundary {
    Pty,
    Window,
    Clipboard,
    Diagnostics,
}

impl FoundationBoundary {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pty => "pty",
            Self::Window => "window",
            Self::Clipboard => "clipboard",
            Self::Diagnostics => "diagnostics",
        }
    }

    pub const fn runtime_layer(self) -> RuntimeLayer {
        match self {
            Self::Pty => RuntimeLayer::FoundationPty,
            Self::Window => RuntimeLayer::FoundationWindow,
            Self::Clipboard => RuntimeLayer::FoundationClipboard,
            Self::Diagnostics => RuntimeLayer::FoundationDiagnostics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyOperation {
    SpawnShell,
    AcquireWriterLease,
    ReleaseWriterLease,
    Resize,
    Write,
    Read,
    TryWait,
    Kill,
}

impl PtyOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpawnShell => "spawn-shell",
            Self::AcquireWriterLease => "acquire-writer-lease",
            Self::ReleaseWriterLease => "release-writer-lease",
            Self::Resize => "resize",
            Self::Write => "write",
            Self::Read => "read",
            Self::TryWait => "try-wait",
            Self::Kill => "kill",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtyFailureCode {
    InvalidSpawnRequest,
    SingleWriterInvariantViolation,
    SessionClosed,
    IoFailure,
    Unsupported,
    BoundaryFault,
}

impl PtyFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSpawnRequest => "invalid-spawn-request",
            Self::SingleWriterInvariantViolation => "single-writer-invariant-violation",
            Self::SessionClosed => "session-closed",
            Self::IoFailure => "io-failure",
            Self::Unsupported => "unsupported",
            Self::BoundaryFault => "boundary-fault",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOperation {
    RequestRedraw,
    SetTitle,
    QueryMonitorTiming,
    Close,
}

impl WindowOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestRedraw => "request-redraw",
            Self::SetTitle => "set-title",
            Self::QueryMonitorTiming => "query-monitor-timing",
            Self::Close => "close",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFailureCode {
    EventLoopUnavailable,
    MonitorUnavailable,
    InvalidWindowState,
    Unsupported,
    BoundaryFault,
}

impl WindowFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventLoopUnavailable => "event-loop-unavailable",
            Self::MonitorUnavailable => "monitor-unavailable",
            Self::InvalidWindowState => "invalid-window-state",
            Self::Unsupported => "unsupported",
            Self::BoundaryFault => "boundary-fault",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardOperation {
    SetText,
    GetText,
    Clear,
}

impl ClipboardOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SetText => "set-text",
            Self::GetText => "get-text",
            Self::Clear => "clear",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardFailureCode {
    Unavailable,
    AccessDenied,
    PayloadTooLarge,
    Unsupported,
    BoundaryFault,
}

impl ClipboardFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::AccessDenied => "access-denied",
            Self::PayloadTooLarge => "payload-too-large",
            Self::Unsupported => "unsupported",
            Self::BoundaryFault => "boundary-fault",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsOperation {
    Emit,
    Flush,
}

impl DiagnosticsOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Emit => "emit",
            Self::Flush => "flush",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsFailureCode {
    SinkUnavailable,
    Backpressure,
    SerializationFailure,
    Unsupported,
    BoundaryFault,
}

impl DiagnosticsFailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SinkUnavailable => "sink-unavailable",
            Self::Backpressure => "backpressure",
            Self::SerializationFailure => "serialization-failure",
            Self::Unsupported => "unsupported",
            Self::BoundaryFault => "boundary-fault",
        }
    }
}
