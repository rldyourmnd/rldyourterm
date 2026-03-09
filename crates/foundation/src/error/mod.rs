// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod foundation;
mod types;

pub use foundation::FoundationError;
pub use types::{
    ClipboardFailureCode, ClipboardOperation, CorrelationId, DiagnosticsFailureCode,
    DiagnosticsOperation, ErrorSeverity, FoundationBoundary, FoundationResult, PtyFailureCode,
    PtyOperation, Recoverability, RuntimeLayer, WindowFailureCode, WindowOperation,
};
