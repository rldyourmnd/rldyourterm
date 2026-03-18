// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod sink;
mod types;

#[cfg(test)]
mod tests;

use std::time::{SystemTime, UNIX_EPOCH};

pub use sink::{
    CorrelatedDiagnosticsSink, DiagnosticsRuntimeConfig, DiagnosticsSink, ShellDiagnosticsEmitter,
};
pub use types::{
    CorrelationId, DiagnosticsPayloadError, Event, EventKind, FishBaselineFailureCauseKind,
    RenderCadencePolicyKind, RenderModeKind, RuntimeCommandReceiptTypedPayload,
    RuntimeCommandSourceKind, RuntimeProfilePresetKind, SettingsApplyOutcomeKind,
    SettingsApplySourceKind, SettingsApplyTypedPayload, SettingsStateTypedPayload,
    ShellLaunchPayload, ShellLaunchProfileKind, ShellLaunchTypedPayload, ShellResolutionErrorKind,
    ShellResolutionReasonKind, ShellResolutionTypedPayload, ShellTargetKind, ThemePresetKind,
};

fn now_timestamp_ms() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis() as u64
}
