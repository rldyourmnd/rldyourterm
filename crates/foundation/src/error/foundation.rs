// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::types::*;
use thiserror::Error;

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
}
