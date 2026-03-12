// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_foundation::api::diagnostics::{
    DiagnosticEvent as FoundationDiagnosticEvent, DiagnosticKind as FoundationDiagnosticKind,
    DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity,
};
use serde::{Deserialize, Serialize};

use crate::now_timestamp_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    SessionStarted,
    SessionEnded,
    SessionError,
    SettingsApply,
    SettingsRejected,
    ShellResolved,
    ShellResolutionFailed,
    ShellFallbackApplied,
    ShellLaunchPlanned,
    RenderModeTransition,
    ResourceWarning,
}

impl EventKind {
    pub(crate) fn foundation_kind(self) -> FoundationDiagnosticKind {
        match self {
            Self::SessionStarted => FoundationDiagnosticKind::SessionStarted,
            Self::SessionEnded => FoundationDiagnosticKind::SessionEnded,
            Self::SessionError => FoundationDiagnosticKind::SessionError,
            Self::SettingsApply => FoundationDiagnosticKind::SettingsApply,
            Self::SettingsRejected => FoundationDiagnosticKind::SettingsRejected,
            Self::ShellResolved => FoundationDiagnosticKind::ShellResolved,
            Self::ShellResolutionFailed => FoundationDiagnosticKind::ShellResolutionFailed,
            Self::ShellFallbackApplied => FoundationDiagnosticKind::ShellFallbackApplied,
            Self::ShellLaunchPlanned => FoundationDiagnosticKind::ShellLaunchPlanned,
            Self::RenderModeTransition => FoundationDiagnosticKind::RenderModeTransition,
            Self::ResourceWarning => FoundationDiagnosticKind::ResourceWarning,
        }
    }

    pub(crate) fn foundation_severity(self) -> FoundationDiagnosticSeverity {
        match self {
            Self::SessionError => FoundationDiagnosticSeverity::Error,
            Self::ShellResolutionFailed | Self::SettingsRejected => {
                FoundationDiagnosticSeverity::Warn
            }
            Self::RenderModeTransition => FoundationDiagnosticSeverity::Warn,
            Self::ResourceWarning => FoundationDiagnosticSeverity::Warn,
            Self::SessionStarted
            | Self::SessionEnded
            | Self::SettingsApply
            | Self::ShellResolved
            | Self::ShellFallbackApplied
            | Self::ShellLaunchPlanned => FoundationDiagnosticSeverity::Info,
        }
    }

    pub(crate) fn foundation_layer(self) -> FoundationDiagnosticLayer {
        FoundationDiagnosticLayer::App
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub kind: EventKind,
    pub message: String,
    pub correlation_id: Option<CorrelationId>,
    pub(crate) payload_json: Option<String>,
    pub timestamp_ms: u64,
}

impl Event {
    pub fn new(kind: EventKind, message: impl Into<String>) -> Self {
        Self {
            event_id: String::new(),
            kind,
            message: message.into(),
            correlation_id: None,
            payload_json: None,
            timestamp_ms: now_timestamp_ms(),
        }
    }

    pub fn with_correlation(mut self, correlation_id: CorrelationId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub(crate) fn to_foundation_event(&self) -> FoundationDiagnosticEvent {
        let mut event = FoundationDiagnosticEvent::new(
            self.event_id.clone(),
            self.kind.foundation_kind(),
            self.kind.foundation_severity(),
            self.kind.foundation_layer(),
            self.message.clone(),
            self.timestamp_ms,
        );

        if let Some(correlation_id) = self.correlation_id.as_ref() {
            event = event.with_correlation_id(correlation_id.as_str().to_owned());
        }
        if let Some(payload_json) = self.payload_json.as_ref() {
            event = event.with_payload_json(payload_json.clone());
        }

        event
    }
}
