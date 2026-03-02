use crate::error::{ClipboardFailureCode, ClipboardOperation, FoundationError, Recoverability};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardHealth {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardFailureKind {
    ContentNotAvailable,
    NotSupported,
    Occupied,
    ConversionFailure,
    Unknown,
}

impl ClipboardFailureKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContentNotAvailable => "content-not-available",
            Self::NotSupported => "not-supported",
            Self::Occupied => "occupied",
            Self::ConversionFailure => "conversion-failure",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardFailure {
    pub kind: ClipboardFailureKind,
    pub operation: ClipboardOperation,
    pub detail: String,
}

impl ClipboardFailure {
    pub fn new(
        kind: ClipboardFailureKind,
        operation: ClipboardOperation,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation,
            detail: detail.into(),
        }
    }

    pub fn into_foundation_error(self) -> FoundationError {
        let (code, recoverability) = match self.kind {
            ClipboardFailureKind::ContentNotAvailable => {
                (ClipboardFailureCode::Unavailable, Recoverability::Degrade)
            }
            ClipboardFailureKind::NotSupported => {
                (ClipboardFailureCode::Unsupported, Recoverability::Degrade)
            }
            ClipboardFailureKind::Occupied => (
                ClipboardFailureCode::AccessDenied,
                Recoverability::Retryable,
            ),
            ClipboardFailureKind::ConversionFailure => {
                (ClipboardFailureCode::BoundaryFault, Recoverability::Degrade)
            }
            ClipboardFailureKind::Unknown => {
                (ClipboardFailureCode::BoundaryFault, Recoverability::Degrade)
            }
        };

        FoundationError::clipboard(
            self.operation,
            code,
            recoverability,
            format!(
                "clipboard failure ({}): {}",
                self.kind.as_str(),
                self.detail
            ),
            None,
        )
    }
}
