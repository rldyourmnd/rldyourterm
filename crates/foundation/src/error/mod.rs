mod foundation;
mod types;

pub use foundation::FoundationError;
pub use types::{
    ClipboardFailureCode, ClipboardOperation, CorrelationId, DiagnosticsFailureCode,
    DiagnosticsOperation, ErrorSeverity, FoundationBoundary, FoundationResult, PtyFailureCode,
    PtyOperation, Recoverability, RuntimeLayer, WindowFailureCode, WindowOperation,
};
