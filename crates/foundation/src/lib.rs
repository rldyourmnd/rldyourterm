// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

pub mod api;
pub mod clipboard;
pub mod diagnostics;
pub mod error;
pub mod window;

pub use api::common::{ContractResult, FontMetrics, MonitorTiming, ViewportSize};
pub use error::{
    CorrelationId, ErrorSeverity, FoundationError, FoundationResult, Recoverability, RuntimeLayer,
};
