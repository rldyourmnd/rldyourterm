use rldyourterm_foundation::api::diagnostics::{
    DiagnosticConfig as FoundationDiagnosticConfig, DiagnosticEvent as FoundationDiagnosticEvent,
    DiagnosticKind as FoundationDiagnosticKind, DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity, DiagnosticSink as FoundationDiagnosticSink,
};

use crate::{
    CorrelationId, DiagnosticsPayloadError, DiagnosticsRuntimeConfig, DiagnosticsSink, Event,
    EventKind, FishBaselineFailureCauseKind, SettingsApplyOutcomeKind, SettingsApplyTypedPayload,
    ShellLaunchPayload, ShellLaunchProfileKind, ShellLaunchTypedPayload, ShellResolutionErrorKind,
    ShellResolutionReasonKind, ShellResolutionTypedPayload, ShellTargetKind,
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
                command: "mode gpu".to_string(),
                outcome: SettingsApplyOutcomeKind::Rejected,
                previous_state: "{\"mode\":\"auto\"}".to_string(),
                current_state: Some("{\"mode\":\"gpu\"}".to_string()),
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
