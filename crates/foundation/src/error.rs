use thiserror::Error;

pub type FoundationResult<T> = std::result::Result<T, FoundationError>;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub layer: RuntimeLayer,
    pub code: String,
    pub message: String,
    pub severity: ErrorSeverity,
    pub recoverability: Recoverability,
    pub correlation_id: Option<CorrelationId>,
    pub raw: Option<String>,
}

impl RuntimeError {
    pub const fn is_recoverable(&self) -> bool {
        self.recoverability.is_recoverable()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FoundationError {
    #[error(
        "pty boundary failure: op={operation:?} code={code:?} recoverability={recoverability:?} correlation_id={correlation_id:?} message={message}"
    )]
    Pty {
        operation: PtyOperation,
        code: PtyFailureCode,
        recoverability: Recoverability,
        message: String,
        correlation_id: Option<CorrelationId>,
    },
    #[error(
        "window boundary failure: op={operation:?} code={code:?} recoverability={recoverability:?} correlation_id={correlation_id:?} message={message}"
    )]
    Window {
        operation: WindowOperation,
        code: WindowFailureCode,
        recoverability: Recoverability,
        message: String,
        correlation_id: Option<CorrelationId>,
    },
    #[error(
        "clipboard boundary failure: op={operation:?} code={code:?} recoverability={recoverability:?} correlation_id={correlation_id:?} message={message}"
    )]
    Clipboard {
        operation: ClipboardOperation,
        code: ClipboardFailureCode,
        recoverability: Recoverability,
        message: String,
        correlation_id: Option<CorrelationId>,
    },
    #[error(
        "diagnostics boundary failure: op={operation:?} code={code:?} recoverability={recoverability:?} correlation_id={correlation_id:?} message={message}"
    )]
    Diagnostics {
        operation: DiagnosticsOperation,
        code: DiagnosticsFailureCode,
        recoverability: Recoverability,
        message: String,
        correlation_id: Option<CorrelationId>,
    },
    #[error(
        "foundation contract violation: boundary={boundary:?} contract={contract} correlation_id={correlation_id:?} message={message}"
    )]
    ContractViolation {
        boundary: FoundationBoundary,
        contract: &'static str,
        message: String,
        correlation_id: Option<CorrelationId>,
    },
}

impl FoundationError {
    pub fn pty(
        operation: PtyOperation,
        code: PtyFailureCode,
        recoverability: Recoverability,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self::Pty {
            operation,
            code,
            recoverability,
            message: message.into(),
            correlation_id,
        }
    }

    pub fn window(
        operation: WindowOperation,
        code: WindowFailureCode,
        recoverability: Recoverability,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self::Window {
            operation,
            code,
            recoverability,
            message: message.into(),
            correlation_id,
        }
    }

    pub fn clipboard(
        operation: ClipboardOperation,
        code: ClipboardFailureCode,
        recoverability: Recoverability,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self::Clipboard {
            operation,
            code,
            recoverability,
            message: message.into(),
            correlation_id,
        }
    }

    pub fn diagnostics(
        operation: DiagnosticsOperation,
        code: DiagnosticsFailureCode,
        recoverability: Recoverability,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self::Diagnostics {
            operation,
            code,
            recoverability,
            message: message.into(),
            correlation_id,
        }
    }

    pub fn contract_violation(
        boundary: FoundationBoundary,
        contract: &'static str,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
    ) -> Self {
        Self::ContractViolation {
            boundary,
            contract,
            message: message.into(),
            correlation_id,
        }
    }

    pub const fn boundary(&self) -> FoundationBoundary {
        match self {
            Self::Pty { .. } => FoundationBoundary::Pty,
            Self::Window { .. } => FoundationBoundary::Window,
            Self::Clipboard { .. } => FoundationBoundary::Clipboard,
            Self::Diagnostics { .. } => FoundationBoundary::Diagnostics,
            Self::ContractViolation { boundary, .. } => *boundary,
        }
    }

    pub const fn runtime_layer(&self) -> RuntimeLayer {
        self.boundary().runtime_layer()
    }

    pub const fn recoverability(&self) -> Recoverability {
        match self {
            Self::Pty { recoverability, .. }
            | Self::Window { recoverability, .. }
            | Self::Clipboard { recoverability, .. }
            | Self::Diagnostics { recoverability, .. } => *recoverability,
            Self::ContractViolation { .. } => Recoverability::Fatal,
        }
    }

    pub const fn severity(&self) -> ErrorSeverity {
        let base = self.recoverability().default_severity();
        match self {
            Self::Pty {
                code: PtyFailureCode::BoundaryFault,
                ..
            }
            | Self::Window {
                code: WindowFailureCode::BoundaryFault,
                ..
            }
            | Self::Clipboard {
                code: ClipboardFailureCode::BoundaryFault,
                ..
            }
            | Self::Diagnostics {
                code: DiagnosticsFailureCode::BoundaryFault,
                ..
            } => ErrorSeverity::High,
            _ => base,
        }
    }

    pub fn code(&self) -> String {
        match self {
            Self::Pty {
                operation, code, ..
            } => format!("foundation.pty.{}.{}", operation.as_str(), code.as_str()),
            Self::Window {
                operation, code, ..
            } => format!("foundation.window.{}.{}", operation.as_str(), code.as_str()),
            Self::Clipboard {
                operation, code, ..
            } => format!(
                "foundation.clipboard.{}.{}",
                operation.as_str(),
                code.as_str()
            ),
            Self::Diagnostics {
                operation, code, ..
            } => format!(
                "foundation.diagnostics.{}.{}",
                operation.as_str(),
                code.as_str()
            ),
            Self::ContractViolation {
                boundary, contract, ..
            } => format!(
                "foundation.{}.contract-violation.{}",
                boundary.as_str(),
                contract.replace(' ', "-")
            ),
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Pty { message, .. }
            | Self::Window { message, .. }
            | Self::Clipboard { message, .. }
            | Self::Diagnostics { message, .. }
            | Self::ContractViolation { message, .. } => message.as_str(),
        }
    }

    pub fn correlation_id(&self) -> Option<&CorrelationId> {
        match self {
            Self::Pty { correlation_id, .. }
            | Self::Window { correlation_id, .. }
            | Self::Clipboard { correlation_id, .. }
            | Self::Diagnostics { correlation_id, .. }
            | Self::ContractViolation { correlation_id, .. } => correlation_id.as_ref(),
        }
    }

    pub fn to_runtime_error(&self) -> RuntimeError {
        RuntimeError {
            layer: self.runtime_layer(),
            code: self.code(),
            message: self.message().to_owned(),
            severity: self.severity(),
            recoverability: self.recoverability(),
            correlation_id: self.correlation_id().cloned(),
            raw: Some(self.to_string()),
        }
    }
}

impl From<FoundationError> for RuntimeError {
    fn from(value: FoundationError) -> Self {
        value.to_runtime_error()
    }
}
