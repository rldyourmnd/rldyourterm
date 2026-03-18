// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

pub mod api;
pub mod clipboard;
pub mod diagnostics;
pub mod error;
pub mod window;

pub use api::common::{ContractResult, FontMetrics, MonitorTiming, ViewportSize};
pub use error::{
    CorrelationId, ErrorSeverity, FoundationError, FoundationResult, Recoverability, RuntimeLayer,
};
