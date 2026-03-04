use rldyourterm_foundation::api::diagnostics::{
    DiagnosticEvent as FoundationDiagnosticEvent, DiagnosticKind as FoundationDiagnosticKind,
    DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
    ResourceWarning,
}

impl EventKind {
    fn foundation_kind(self) -> FoundationDiagnosticKind {
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
            Self::ResourceWarning => FoundationDiagnosticKind::ResourceWarning,
        }
    }

    fn foundation_severity(self) -> FoundationDiagnosticSeverity {
        match self {
            Self::SessionError | Self::ShellResolutionFailed | Self::SettingsRejected => {
                FoundationDiagnosticSeverity::Warn
            }
            Self::ResourceWarning => FoundationDiagnosticSeverity::Warn,
            Self::SessionStarted
            | Self::SessionEnded
            | Self::SettingsApply
            | Self::ShellResolved
            | Self::ShellFallbackApplied
            | Self::ShellLaunchPlanned => FoundationDiagnosticSeverity::Info,
        }
    }

    fn foundation_layer(self) -> FoundationDiagnosticLayer {
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
    pub payload_json: Option<String>,
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

    pub fn with_payload_json(mut self, payload_json: impl Into<String>) -> Self {
        self.payload_json = Some(payload_json.into());
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

    fn to_foundation_event(&self) -> FoundationDiagnosticEvent {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsApplyPayload {
    pub command: String,
    pub outcome: String,
    pub previous_state: String,
    pub current_state: Option<String>,
    pub reject_reason: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellResolutionPayload {
    pub requested: String,
    pub resolved: Option<String>,
    pub fallback_applied: bool,
    pub fallback_cause: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
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
    fn event_kind(&self) -> EventKind {
        match self.outcome {
            SettingsApplyOutcomeKind::Rejected => EventKind::SettingsRejected,
            SettingsApplyOutcomeKind::Applied | SettingsApplyOutcomeKind::Noop => {
                EventKind::SettingsApply
            }
        }
    }

    fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
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
    fn event_kind(&self) -> EventKind {
        if self.error.is_some() {
            EventKind::ShellResolutionFailed
        } else if self.fallback_applied {
            EventKind::ShellFallbackApplied
        } else {
            EventKind::ShellResolved
        }
    }

    fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
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
    pub fn emit(&self, mut event: Event) -> Event {
        if event.event_id.is_empty() {
            event.event_id = self.next_event_id();
        }
        if event.timestamp_ms == 0 {
            event.timestamp_ms = now_timestamp_ms();
        }

        let canonical_event = event.to_foundation_event();
        tracing::info!(
            event_id = %canonical_event.event_id.0,
            kind = ?canonical_event.kind,
            severity = ?canonical_event.severity,
            layer = ?canonical_event.layer,
            correlation_id = ?canonical_event.correlation_id,
            "diagnostics event emitted"
        );

        event
    }

    pub fn emit_kind(&self, kind: EventKind, message: impl Into<String>) -> Event {
        self.emit(Event::new(kind, message))
    }

    pub fn emit_serialized_payload<T: Serialize>(
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

    pub fn emit_settings_apply(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &SettingsApplyPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let kind = settings_apply_event_kind(payload);
        self.emit_serialized_payload(kind, "settings.apply", correlation_id, payload)
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

    pub fn emit_shell_resolution(
        &self,
        correlation_id: Option<CorrelationId>,
        payload: &ShellResolutionPayload,
    ) -> Result<Event, DiagnosticsPayloadError> {
        let kind = shell_resolution_event_kind(payload);
        self.emit_serialized_payload(kind, "shell.resolve", correlation_id, payload)
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

fn settings_apply_event_kind(payload: &SettingsApplyPayload) -> EventKind {
    if payload.reject_reason.is_some() || payload.outcome.eq_ignore_ascii_case("rejected") {
        EventKind::SettingsRejected
    } else {
        EventKind::SettingsApply
    }
}

fn shell_resolution_event_kind(payload: &ShellResolutionPayload) -> EventKind {
    if payload.error.is_some() {
        EventKind::ShellResolutionFailed
    } else if payload.fallback_applied {
        EventKind::ShellFallbackApplied
    } else {
        EventKind::ShellResolved
    }
}

fn now_timestamp_ms() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration.as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn emit_settings_apply_maps_rejected_payload_to_settings_rejected_kind() {
        let sink = DiagnosticsSink::default();
        let emitted = sink
            .emit_settings_apply(
                Some(CorrelationId::new("corr-settings")),
                &SettingsApplyPayload {
                    command: "mode gpu".to_string(),
                    outcome: "rejected".to_string(),
                    previous_state: "{\"mode\":\"auto\"}".to_string(),
                    current_state: None,
                    reject_reason: Some("invalid".to_string()),
                },
            )
            .unwrap();

        assert_eq!(emitted.kind, EventKind::SettingsRejected);
        assert_eq!(
            emitted.correlation_id,
            Some(CorrelationId::new("corr-settings"))
        );
        assert!(emitted.payload_json.is_some());
    }

    #[test]
    fn emit_shell_resolution_maps_fallback_to_shell_fallback_kind() {
        let sink = DiagnosticsSink::default();
        let emitted = sink
            .emit_shell_resolution(
                None,
                &ShellResolutionPayload {
                    requested: "auto".to_string(),
                    resolved: Some("zsh".to_string()),
                    fallback_applied: true,
                    fallback_cause: Some("fish-unavailable".to_string()),
                    reason: Some("fish baseline unavailable".to_string()),
                    error: None,
                },
            )
            .unwrap();

        assert_eq!(emitted.kind, EventKind::ShellFallbackApplied);
        assert!(emitted.payload_json.is_some());
    }

    #[test]
    fn emit_settings_apply_uses_outcome_when_reason_missing() {
        let sink = DiagnosticsSink::default();
        let emitted = sink
            .emit_settings_apply(
                None,
                &SettingsApplyPayload {
                    command: "shell auto-init on".to_string(),
                    outcome: "rejected".to_string(),
                    previous_state: "{\"shell_target\":\"zsh\"}".to_string(),
                    current_state: None,
                    reject_reason: None,
                },
            )
            .unwrap();

        assert_eq!(emitted.kind, EventKind::SettingsRejected);
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
}
