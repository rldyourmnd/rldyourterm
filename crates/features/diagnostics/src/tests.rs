// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_foundation::api::diagnostics::{
    DiagnosticEvent as FoundationDiagnosticEvent, DiagnosticKind as FoundationDiagnosticKind,
    DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity, DiagnosticSink as FoundationDiagnosticSink,
};

use crate::{CorrelationId, DiagnosticsSink, Event, EventKind};

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
