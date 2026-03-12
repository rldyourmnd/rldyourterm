// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_foundation::api::diagnostics::{
    DiagnosticConfig as FoundationDiagnosticConfig, DiagnosticEvent as FoundationDiagnosticEvent,
    DiagnosticKind as FoundationDiagnosticKind, DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity, DiagnosticSink as FoundationDiagnosticSink,
};

use crate::{
    CorrelationId, DiagnosticsPayloadError, DiagnosticsRuntimeConfig, DiagnosticsSink, Event,
    EventKind, FishBaselineFailureCauseKind, RenderCadencePolicyKind, RenderModeKind,
    RuntimeCommandReceiptTypedPayload, RuntimeCommandSourceKind, RuntimeProfilePresetKind,
    SettingsApplyOutcomeKind, SettingsApplySourceKind, SettingsApplyTypedPayload,
    SettingsStateTypedPayload, ShellLaunchPayload, ShellLaunchProfileKind, ShellLaunchTypedPayload,
    ShellResolutionErrorKind, ShellResolutionReasonKind, ShellResolutionTypedPayload,
    ShellTargetKind, ThemePresetKind,
};
use rldyourterm_services::render_mode::{GpuFailureKind, RenderMode};
use rldyourterm_services::runtime_protocol::{
    UiCommandOutcome, UiCommandReceipt, UiRuntimeCommand,
};
use rldyourterm_services::session::SessionState;
use rldyourterm_settings::{SettingsCommand, SettingsService};
use rldyourterm_shell_integration::{
    ShellDiagnosticsEvent, ShellLaunchPlan, ShellResolution, ShellResolutionReason, ShellTarget,
};

#[test]
fn emit_assigns_event_id_when_missing() {
    let sink = DiagnosticsSink::default();
    let emitted = sink.emit(Event::new(EventKind::SessionStarted, "boot"));

    assert_eq!(emitted.event_id, "diag-1");
    assert!(emitted.timestamp_ms > 0);
}

#[test]
fn emit_preserves_existing_event_id() {
    let sink = DiagnosticsSink::default();
    let emitted = sink.emit(Event {
        event_id: "custom-1".to_string(),
        kind: EventKind::SettingsApply,
        message: "mode auto".to_string(),
        correlation_id: None,
        payload_json: None,
        timestamp_ms: 0,
    });

    assert_eq!(emitted.event_id, "custom-1");
    assert!(emitted.timestamp_ms > 0);
}

#[test]
fn correlation_sink_attaches_id_to_emitted_events() {
    let sink = DiagnosticsSink::default();
    let correlation_id = CorrelationId::new("corr-123");

    let emitted = sink
        .with_correlation(correlation_id.clone())
        .emit_kind(EventKind::ShellResolved, "resolved fish");

    assert_eq!(emitted.event_id, "diag-1");
    assert_eq!(emitted.correlation_id, Some(correlation_id));
}

#[test]
fn explicit_event_correlation_is_not_overridden() {
    let sink = DiagnosticsSink::default();
    let default_corr = CorrelationId::new("corr-default");
    let explicit_corr = CorrelationId::new("corr-explicit");

    let emitted = sink.with_correlation(default_corr).emit(
        Event::new(EventKind::SessionError, "pty read error")
            .with_correlation(explicit_corr.clone()),
    );

    assert_eq!(emitted.correlation_id, Some(explicit_corr));
}

#[test]
fn event_try_with_payload_serializes_json() {
    let payload = ShellLaunchPayload {
        executable: "fish".to_string(),
        args: vec!["-i".to_string(), "-l".to_string()],
        profile: "fish-starship".to_string(),
    };

    let event = Event::new(EventKind::ShellLaunchPlanned, "launch")
        .try_with_payload(&payload)
        .unwrap();
    assert!(event.payload_json.is_some());
}

#[test]
fn session_error_maps_to_error_severity() {
    let event = Event::new(EventKind::SessionError, "pty read error").to_foundation_event();
    assert_eq!(event.severity, FoundationDiagnosticSeverity::Error);
}

#[test]
fn shell_resolution_failed_maps_to_warn_severity() {
    let event =
        Event::new(EventKind::ShellResolutionFailed, "fish unavailable").to_foundation_event();
    assert_eq!(event.severity, FoundationDiagnosticSeverity::Warn);
}

#[test]
fn render_mode_transition_maps_to_warn_severity_and_kind() {
    let event =
        Event::new(EventKind::RenderModeTransition, "gpu->cpu fallback").to_foundation_event();
    assert_eq!(event.severity, FoundationDiagnosticSeverity::Warn);
    assert_eq!(event.kind, FoundationDiagnosticKind::RenderModeTransition);
}

#[test]
fn emit_settings_apply_typed_validates_payload_shape() {
    let sink = DiagnosticsSink::default();
    let err = sink
        .emit_settings_apply_typed(
            None,
            &SettingsApplyTypedPayload {
                source: SettingsApplySourceKind::RuntimeBootstrap,
                step: None,
                command_input: "mode gpu".to_string(),
                outcome: SettingsApplyOutcomeKind::Rejected,
                previous_state: sample_settings_state_payload(),
                current_state: Some(sample_settings_state_payload()),
                reject_reason: None,
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        DiagnosticsPayloadError::InvalidPayload {
            payload: "settings.apply.typed",
            reason: "rejected outcome requires reject_reason",
        }
    );
}

#[test]
fn emit_settings_apply_outcome_serializes_structured_state() {
    let sink = DiagnosticsSink::default();
    let mut settings = SettingsService::default();
    let outcome = settings.apply(SettingsCommand::SetDebugMode(true));

    let emitted = sink
        .emit_settings_apply_outcome(None, SettingsApplySourceKind::RuntimeBootstrap, &outcome)
        .unwrap();

    assert_eq!(emitted.kind, EventKind::SettingsApply);
    let payload_json = emitted.payload_json.expect("payload must exist");
    assert!(payload_json.contains("\"source\":\"runtime-bootstrap\""));
    assert!(payload_json.contains("\"command_input\":\"debug on\""));
    assert!(payload_json.contains("\"debug_mode\":true"));
}

#[test]
fn emit_shell_resolution_typed_maps_fallback_kind_and_serializes_enums() {
    let sink = DiagnosticsSink::default();
    let emitted = sink
        .emit_shell_resolution_typed(
            Some(CorrelationId::new("corr-shell")),
            &ShellResolutionTypedPayload {
                requested: ShellTargetKind::Auto,
                resolved: Some(ShellTargetKind::Zsh),
                fallback_applied: true,
                fallback_cause: Some(FishBaselineFailureCauseKind::FishUnavailable),
                reason: Some(ShellResolutionReasonKind::AutoFallbackToZsh),
                error: None,
            },
        )
        .unwrap();

    assert_eq!(emitted.kind, EventKind::ShellFallbackApplied);
    assert_eq!(
        emitted.correlation_id,
        Some(CorrelationId::new("corr-shell"))
    );

    let payload_json = emitted.payload_json.expect("payload must exist");
    assert!(payload_json.contains("\"requested\":\"auto\""));
    assert!(payload_json.contains("\"fallback_cause\":\"fish-unavailable\""));
    assert!(payload_json.contains("\"reason\":\"auto-fallback-to-zsh\""));
}

#[test]
fn emit_shell_resolution_typed_rejects_error_payload_marked_as_fallback() {
    let sink = DiagnosticsSink::default();
    let err = sink
        .emit_shell_resolution_typed(
            None,
            &ShellResolutionTypedPayload {
                requested: ShellTargetKind::Fish,
                resolved: None,
                fallback_applied: true,
                fallback_cause: Some(FishBaselineFailureCauseKind::FishAndStarshipUnavailable),
                reason: None,
                error: Some(ShellResolutionErrorKind::FishBaselineUnavailableAndZshUnavailable),
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        DiagnosticsPayloadError::InvalidPayload {
            payload: "shell.resolve.typed",
            reason: "error payload must not be marked as fallback_applied",
        }
    );
}

#[test]
fn emit_shell_launch_typed_serializes_profile() {
    let sink = DiagnosticsSink::default();
    let emitted = sink
        .emit_shell_launch_typed(
            None,
            &ShellLaunchTypedPayload {
                executable: "zsh".to_string(),
                args: vec!["-i".to_string(), "-l".to_string()],
                profile: ShellLaunchProfileKind::ZshFallback,
            },
        )
        .unwrap();

    assert_eq!(emitted.kind, EventKind::ShellLaunchPlanned);
    let payload_json = emitted.payload_json.expect("payload must exist");
    assert!(payload_json.contains("\"profile\":\"zsh-fallback\""));
}

#[test]
fn emit_shell_event_typed_maps_shell_hook_events() {
    let sink = DiagnosticsSink::default();
    let event = ShellDiagnosticsEvent::ShellLaunchPlanned(ShellLaunchPlan {
        resolution: ShellResolution {
            requested: ShellTarget::Auto,
            resolved: ShellTarget::Zsh,
            fallback_applied: true,
            reason: ShellResolutionReason::AutoFallbackToZsh,
            fallback_cause: Some(
                rldyourterm_shell_integration::FishBaselineFailureCause::FishUnavailable,
            ),
        },
        executable: "zsh".to_string(),
        args: vec!["-i".to_string(), "-l".to_string()],
        profile: rldyourterm_shell_integration::ShellLaunchProfile::ZshFallback,
    });

    let emitted = sink.emit_shell_event_typed(None, &event).unwrap();

    assert_eq!(emitted.kind, EventKind::ShellLaunchPlanned);
    assert!(
        emitted
            .payload_json
            .expect("payload must exist")
            .contains("\"profile\":\"zsh-fallback\"")
    );
}

#[test]
fn correlated_sink_emits_payload_with_existing_correlation() {
    let sink = DiagnosticsSink::default();
    let correlated = sink.with_correlation(CorrelationId::new("corr-launch"));
    let emitted = correlated
        .emit_kind_with_payload(
            EventKind::ShellLaunchPlanned,
            "launch plan",
            &ShellLaunchPayload {
                executable: "zsh".to_string(),
                args: vec!["-i".to_string(), "-l".to_string()],
                profile: "zsh-fallback".to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        emitted.correlation_id,
        Some(CorrelationId::new("corr-launch"))
    );
    assert!(emitted.payload_json.is_some());
}

#[test]
fn foundation_diagnostic_sink_trait_emits_without_error() {
    let sink = DiagnosticsSink::default();
    let foundation_sink: &dyn FoundationDiagnosticSink = &sink;

    foundation_sink
        .emit(
            FoundationDiagnosticEvent::new(
                "diag-foundation",
                FoundationDiagnosticKind::SessionStarted,
                FoundationDiagnosticSeverity::Info,
                FoundationDiagnosticLayer::App,
                "boot",
                1,
            )
            .with_correlation_id("corr-1"),
        )
        .expect("foundation event should emit");
    foundation_sink.flush().expect("flush should be a no-op");
}

#[test]
fn diagnostics_runtime_config_implements_foundation_contract() {
    let config = DiagnosticsRuntimeConfig::new(true, true);
    let foundation_config: &dyn FoundationDiagnosticConfig = &config;

    assert!(foundation_config.is_enabled());
    assert!(foundation_config.is_debug_mode());
}

#[test]
fn emit_runtime_command_receipt_serializes_structured_protocol_payload() {
    let sink = DiagnosticsSink::default();
    let emitted = sink
        .emit_runtime_command_receipt(
            None,
            RuntimeCommandSourceKind::GpuFailureHandler,
            None,
            &sample_gpu_retry_receipt(),
        )
        .unwrap();

    assert_eq!(emitted.kind, EventKind::ResourceWarning);
    let payload_json = emitted.payload_json.expect("payload must exist");
    assert!(payload_json.contains("\"source\":\"gpu-failure-handler\""));
    assert!(payload_json.contains("\"kind\":\"gpu-failure\""));
    assert!(payload_json.contains("\"failure_kind\":\"surface-error\""));
    assert!(payload_json.contains("\"outcome\":{\"kind\":\"gpu-retry-scheduled\""));
}

#[test]
fn emit_runtime_command_receipt_maps_cadence_resync_to_cadence_event() {
    let sink = DiagnosticsSink::default();
    let emitted = sink
        .emit_runtime_command_receipt(
            None,
            RuntimeCommandSourceKind::MonitorEvent,
            None,
            &sample_cadence_receipt(),
        )
        .unwrap();

    assert_eq!(emitted.kind, EventKind::RenderCadenceUpdated);
    assert!(
        emitted
            .payload_json
            .expect("payload must exist")
            .contains("\"kind\":\"cadence-resynced\"")
    );
}

#[test]
fn emit_runtime_command_receipt_rejects_bootstrap_without_step() {
    let sink = DiagnosticsSink::default();
    let err = sink
        .emit_runtime_command_receipt_typed(
            None,
            &RuntimeCommandReceiptTypedPayload {
                source: RuntimeCommandSourceKind::BootstrapHook,
                step: None,
                receipt: sample_gpu_retry_receipt(),
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        DiagnosticsPayloadError::InvalidPayload {
            payload: "runtime.command.receipt",
            reason: "bootstrap-hook payload requires step",
        }
    );
}

fn sample_settings_state_payload() -> SettingsStateTypedPayload {
    SettingsStateTypedPayload {
        mode: RenderModeKind::Auto,
        shell_target: ShellTargetKind::Auto,
        shell_auto_init: true,
        render_cadence_policy: RenderCadencePolicyKind::MonitorAuto,
        theme: ThemePresetKind::Cuberpunk,
        runtime_profile: RuntimeProfilePresetKind::Balanced,
        debug_mode: false,
    }
}

fn sample_gpu_retry_receipt() -> UiCommandReceipt {
    UiCommandReceipt {
        command: UiRuntimeCommand::GpuFailure {
            kind: GpuFailureKind::SurfaceError,
            observed_at_millis: 24,
        },
        outcome: UiCommandOutcome::GpuRetryScheduled {
            failure_kind: GpuFailureKind::SurfaceError,
            failure_streak: 2,
            retry_budget_remaining: 0,
        },
        state: SessionState::Degraded,
        render_mode: RenderMode::Auto,
        cadence_millihz: 60_000,
        window_count: 1,
    }
}

fn sample_cadence_receipt() -> UiCommandReceipt {
    UiCommandReceipt {
        command: UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 144_000,
        },
        outcome: UiCommandOutcome::CadenceResynced {
            previous_refresh_rate_millihz: Some(60_000),
            current_refresh_rate_millihz: Some(144_000),
            generation: 2,
            schedule_invalidated: true,
            monitor_transfer: false,
        },
        state: SessionState::Running,
        render_mode: RenderMode::Auto,
        cadence_millihz: 144_000,
        window_count: 1,
    }
}
