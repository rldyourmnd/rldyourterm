// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod foundation;
mod types;

pub use foundation::FoundationError;
pub use types::{
    ClipboardFailureCode, ClipboardOperation, CorrelationId, DiagnosticsFailureCode,
    DiagnosticsOperation, ErrorSeverity, FoundationBoundary, FoundationResult, PtyFailureCode,
    PtyOperation, Recoverability, RuntimeLayer, WindowFailureCode, WindowOperation,
};
