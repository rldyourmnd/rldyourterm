// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_foundation::api::common::ContractResult;
use rldyourterm_foundation::api::diagnostics::{
    DiagnosticConfig as FoundationDiagnosticConfig, DiagnosticEvent as FoundationDiagnosticEvent,
    DiagnosticSink as FoundationDiagnosticSink,
};
use rldyourterm_services::runtime_protocol::UiCommandReceipt;
use rldyourterm_settings::{SettingsApplyOutcome, SettingsPaletteApplyOutcome};
use rldyourterm_shell_integration::{ShellDiagnosticsEvent, ShellDiagnosticsHook};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    CorrelationId, DiagnosticsPayloadError, Event, EventKind, RuntimeCommandReceiptTypedPayload,
    RuntimeCommandSourceKind, SettingsApplySourceKind, SettingsApplyTypedPayload,
    ShellLaunchPayload, ShellLaunchTypedPayload, ShellResolutionTypedPayload, now_timestamp_ms,
};

#[derive(Debug)]
pub struct DiagnosticsSink {
    next_id: AtomicU64,
}

impl Default for DiagnosticsSink {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }
}

impl DiagnosticsSink {
    fn emit_foundation_event(&self, canonical_event: &FoundationDiagnosticEvent) {
        tracing::info!(
            event_id = %canonical_event.event_id.0,
            kind = ?canonical_event.kind,
            severity = ?canonical_event.severity,
            layer = ?canonical_event.layer,
            correlation_id = ?canonical_event.correlation_id,
            "diagnostics event emitted"
        );
    }

    pub fn emit(&self, mut event: Event) -> Event {
        if event.event_id.is_empty() {
            event.event_id = self.next_event_id();
        }
        if event.timestamp_ms == 0 {
            event.timestamp_ms = now_timestamp_ms();
        }

        let canonical_event = event.to_foundation_event();
        self.emit_foundation_event(&canonical_event);

        event
    }

    pub fn emit_kind(&self, kind: EventKind, message: impl Into<String>) -> Event {
        self.emit(Event::new(kind, message))
    }

    fn emit_serialized_payload<T: Serialize>(
        &self,
        kind: EventKind,
        message: impl Into<String>,
        correlation_id: Option<CorrelationId>,
        payload: &T,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let mut event = Event::new(kind, message).try_with_payload(payload)?;
        if let Some(correlation_id) = correlation_id {
            event = event.with_correlation(correlation_id);
        }
        Ok(self.emit(event))
    }

    pub fn emit_settings_apply_typed(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &SettingsApplyTypedPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        payload.validate()?;
        self.emit_serialized_payload(
            payload.event_kind(),
            "settings.apply",
            correlation_id,
            payload,
        )
    }

    pub fn emit_settings_apply_outcome(
        &self,
        correlation_id: Option<CorrelationId>,
        source: SettingsApplySourceKind,
        outcome: &SettingsApplyOutcome,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let payload = SettingsApplyTypedPayload::from_settings_outcome(source, outcome);
        self.emit_settings_apply_typed(correlation_id, &payload)
    }

    pub fn emit_settings_palette_outcome(
        &self,
        correlation_id: Option<CorrelationId>,
        step: u32,
        outcome: &SettingsPaletteApplyOutcome,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let payload = SettingsApplyTypedPayload::from_palette_outcome(step, outcome);
        self.emit_settings_apply_typed(correlation_id, &payload)
    }

    pub fn emit_shell_resolution_typed(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &ShellResolutionTypedPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        payload.validate()?;
        self.emit_serialized_payload(
            payload.event_kind(),
            "shell.resolve",
            correlation_id,
            payload,
        )
    }

    pub fn emit_shell_event_typed(
        &self,
        correlation_id: Option<CorrelationId>,
        event: &ShellDiagnosticsEvent,
    ) -> Result<Event, DiagnosticsPayloadError> {
        match event {
            ShellDiagnosticsEvent::ShellFallbackApplied(resolution) => {
                let payload = ShellResolutionTypedPayload::from_resolution(*resolution);
                payload.validate()?;
                self.emit_serialized_payload(
                    EventKind::ShellFallbackApplied,
                    "shell.resolve",
                    correlation_id,
                    &payload,
                )
            }
            ShellDiagnosticsEvent::ShellResolved(resolution) => {
                let payload = ShellResolutionTypedPayload::from_resolution(*resolution);
                payload.validate()?;
                self.emit_serialized_payload(
                    EventKind::ShellResolved,
                    "shell.resolve",
                    correlation_id,
                    &payload,
                )
            }
            ShellDiagnosticsEvent::ShellResolutionFailed {
                requested,
                error,
                fallback_cause,
            } => {
                let payload = ShellResolutionTypedPayload::from_resolution_failure(
                    *requested,
                    *error,
                    *fallback_cause,
                );
                payload.validate()?;
                self.emit_serialized_payload(
                    EventKind::ShellResolutionFailed,
                    "shell.resolve",
                    correlation_id,
                    &payload,
                )
            }
            ShellDiagnosticsEvent::ShellLaunchPlanned(plan) => {
                let payload = ShellLaunchTypedPayload::from_plan(plan);
                self.emit_shell_launch_typed(correlation_id, &payload)
            }
        }
    }

    pub fn emit_shell_launch(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &ShellLaunchPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        self.emit_serialized_payload(
            EventKind::ShellLaunchPlanned,
            "shell.launch.plan",
            correlation_id,
            payload,
        )
    }

    pub fn emit_shell_launch_typed(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &ShellLaunchTypedPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        self.emit_serialized_payload(
            EventKind::ShellLaunchPlanned,
            "shell.launch.plan",
            correlation_id,
            payload,
        )
    }

    pub fn emit_runtime_command_receipt_typed(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &RuntimeCommandReceiptTypedPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        payload.validate()?;
        self.emit_serialized_payload(
            payload.event_kind(),
            "runtime.command.receipt",
            correlation_id,
            payload,
        )
    }

    pub fn emit_runtime_command_receipt(
        &self,
        correlation_id: Option<CorrelationId>,
        source: RuntimeCommandSourceKind,
        step: Option<u32>,
        receipt: &UiCommandReceipt,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let payload = RuntimeCommandReceiptTypedPayload::from_receipt(source, step, receipt);
        self.emit_runtime_command_receipt_typed(correlation_id, &payload)
    }

    pub fn with_correlation<'a>(
        &'a self,
        correlation_id: CorrelationId,
    ) -> CorrelatedDiagnosticsSink<'a> {
        CorrelatedDiagnosticsSink {
            sink: self,
            correlation_id,
        }
    }

    fn next_event_id(&self) -> String {
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("diag-{sequence}")
    }
}

impl FoundationDiagnosticSink for DiagnosticsSink {
    fn emit(&self, event: FoundationDiagnosticEvent) -> ContractResult<()> {
        self.emit_foundation_event(&event);
        Ok(())
    }

    fn flush(&self) -> ContractResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ShellDiagnosticsEmitter<'a> {
    sink: &'a DiagnosticsSink,
    correlation_id: Option<CorrelationId>,
}

impl<'a> ShellDiagnosticsEmitter<'a> {
    pub fn new(sink: &'a DiagnosticsSink, correlation_id: Option<CorrelationId>) -> Self {
        Self {
            sink,
            correlation_id,
        }
    }
}

impl ShellDiagnosticsHook for ShellDiagnosticsEmitter<'_> {
    fn on_shell_event(&mut self, event: ShellDiagnosticsEvent) {
        if let Err(error) = self
            .sink
            .emit_shell_event_typed(self.correlation_id.clone(), &event)
        {
            tracing::warn!(
                error = ?error,
                ?event,
                "failed to emit typed shell diagnostics"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticsRuntimeConfig {
    enabled: bool,
    debug_mode: bool,
}

impl Default for DiagnosticsRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debug_mode: false,
        }
    }
}

impl DiagnosticsRuntimeConfig {
    pub const fn new(enabled: bool, debug_mode: bool) -> Self {
        Self {
            enabled,
            debug_mode,
        }
    }
}

impl FoundationDiagnosticConfig for DiagnosticsRuntimeConfig {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn is_debug_mode(&self) -> bool {
        self.debug_mode
    }
}

#[derive(Debug)]
pub struct CorrelatedDiagnosticsSink<'a> {
    sink: &'a DiagnosticsSink,
    correlation_id: CorrelationId,
}

impl<'a> CorrelatedDiagnosticsSink<'a> {
    pub fn emit(&self, event: Event) -> Event {
        if event.correlation_id.is_some() {
            return self.sink.emit(event);
        }

        self.sink
            .emit(event.with_correlation(self.correlation_id.clone()))
    }

    pub fn emit_kind(&self, kind: EventKind, message: impl Into<String>) -> Event {
        self.emit(Event::new(kind, message))
    }

    pub fn emit_kind_with_payload<T: Serialize>(
        &self,
        kind: EventKind,
        message: impl Into<String>,
        payload: &T,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let event = Event::new(kind, message).try_with_payload(payload)?;
        Ok(self.emit(event))
    }
}
