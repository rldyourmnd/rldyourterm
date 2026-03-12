// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod sink;
mod types;

#[cfg(test)]
mod tests;

use std::time::{SystemTime, UNIX_EPOCH};

pub use sink::{CorrelatedDiagnosticsSink, DiagnosticsSink};
pub use types::{
    CorrelationId, Event, EventKind,
};

fn now_timestamp_ms() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis() as u64
}
