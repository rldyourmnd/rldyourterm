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

    pub fn try_with_payload<T: Serialize>(
        mut self,
        payload: &T,
    ) -> Result<Self, DiagnosticsPayloadError> {
        let payload_json = serde_json::to_string(payload)
            .map_err(|err| DiagnosticsPayloadError::PayloadSerializationFailed(err.to_string()))?;
        self.payload_json = Some(payload_json);
        Ok(self)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticsPayloadError {
    PayloadSerializationFailed(String),
    InvalidPayload {
        payload: &'static str,
        reason: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsApplyOutcomeKind {
    Applied,
    Noop,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsApplyTypedPayload {
    pub command: String,
    pub outcome: SettingsApplyOutcomeKind,
    pub previous_state: String,
    pub current_state: Option<String>,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellTargetKind {
    Fish,
    Zsh,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FishBaselineFailureCauseKind {
    FishUnavailable,
    StarshipUnavailable,
    FishAndStarshipUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellResolutionReasonKind {
    FishBaselineReady,
    FishRequestedFallbackToZsh,
    AutoSelectedFishBaseline,
    AutoFallbackToZsh,
    ZshRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellResolutionErrorKind {
    FishBaselineUnavailableAndZshUnavailable,
    ZshRequestedButUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellResolutionTypedPayload {
    pub requested: ShellTargetKind,
    pub resolved: Option<ShellTargetKind>,
    pub fallback_applied: bool,
    pub fallback_cause: Option<FishBaselineFailureCauseKind>,
    pub reason: Option<ShellResolutionReasonKind>,
    pub error: Option<ShellResolutionErrorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellLaunchPayload {
    pub executable: String,
    pub args: Vec<String>,
    pub profile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellLaunchProfileKind {
    FishStarshipBaseline,
    ZshRequested,
    ZshFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellLaunchTypedPayload {
    pub executable: String,
    pub args: Vec<String>,
    pub profile: ShellLaunchProfileKind,
}

impl SettingsApplyTypedPayload {
    pub(crate) fn event_kind(&self) -> EventKind {
        match self.outcome {
            SettingsApplyOutcomeKind::Rejected => EventKind::SettingsRejected,
            SettingsApplyOutcomeKind::Applied | SettingsApplyOutcomeKind::Noop => {
                EventKind::SettingsApply
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
        match self.outcome {
            SettingsApplyOutcomeKind::Rejected => {
                if self.reject_reason.is_none() {
                    return Err(DiagnosticsPayloadError::InvalidPayload {
                        payload: "settings.apply.typed",
                        reason: "rejected outcome requires reject_reason",
                    });
                }
                if self.current_state.is_some() {
                    return Err(DiagnosticsPayloadError::InvalidPayload {
                        payload: "settings.apply.typed",
                        reason: "rejected outcome must not include current_state",
                    });
                }
            }
            SettingsApplyOutcomeKind::Applied | SettingsApplyOutcomeKind::Noop => {
                if self.reject_reason.is_some() {
                    return Err(DiagnosticsPayloadError::InvalidPayload {
                        payload: "settings.apply.typed",
                        reason: "non-rejected outcome must not include reject_reason",
                    });
                }
                if self.current_state.is_none() {
                    return Err(DiagnosticsPayloadError::InvalidPayload {
                        payload: "settings.apply.typed",
                        reason: "non-rejected outcome requires current_state",
                    });
                }
            }
        }
        Ok(())
    }
}

impl ShellResolutionTypedPayload {
    pub(crate) fn event_kind(&self) -> EventKind {
        if self.error.is_some() {
            EventKind::ShellResolutionFailed
        } else if self.fallback_applied {
            EventKind::ShellFallbackApplied
        } else {
            EventKind::ShellResolved
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
        if self.error.is_some() {
            if self.fallback_applied {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "shell.resolve.typed",
                    reason: "error payload must not be marked as fallback_applied",
                });
            }
            if self.fallback_cause.is_some() {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "shell.resolve.typed",
                    reason: "error payload must not include fallback_cause",
                });
            }
            return Ok(());
        }

        if self.fallback_applied {
            if self.fallback_cause.is_none() {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "shell.resolve.typed",
                    reason: "fallback payload requires fallback_cause",
                });
            }
            if self.resolved.is_none() {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "shell.resolve.typed",
                    reason: "fallback payload requires resolved target",
                });
            }
        } else if self.fallback_cause.is_some() {
            return Err(DiagnosticsPayloadError::InvalidPayload {
                payload: "shell.resolve.typed",
                reason: "resolved payload must not include fallback_cause without fallback",
            });
        }

        Ok(())
    }
}
