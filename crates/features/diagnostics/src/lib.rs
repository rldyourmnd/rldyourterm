mod sink;
mod types;

#[cfg(test)]
mod tests;

use std::time::{SystemTime, UNIX_EPOCH};

pub use sink::{CorrelatedDiagnosticsSink, DiagnosticsRuntimeConfig, DiagnosticsSink};
pub use types::{
    CorrelationId, DiagnosticsPayloadError, Event, EventKind, FishBaselineFailureCauseKind,
    SettingsApplyOutcomeKind, SettingsApplyTypedPayload, ShellLaunchPayload,
    ShellLaunchProfileKind, ShellLaunchTypedPayload, ShellResolutionErrorKind,
    ShellResolutionReasonKind, ShellResolutionTypedPayload, ShellTargetKind,
};

fn now_timestamp_ms() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis() as u64
}
