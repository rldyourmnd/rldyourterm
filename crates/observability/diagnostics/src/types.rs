// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_foundation::api::diagnostics::{
    DiagnosticEvent as FoundationDiagnosticEvent, DiagnosticKind as FoundationDiagnosticKind,
    DiagnosticLayer as FoundationDiagnosticLayer,
    DiagnosticSeverity as FoundationDiagnosticSeverity,
};
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::runtime_protocol::{UiCommandOutcome, UiCommandReceipt};
use rldyourterm_services::session::SessionTransitionOutcome;
use rldyourterm_services::shell_target::ShellTarget;
use rldyourterm_settings::{
    FontFallbackPolicy, RenderCadencePolicy, RuntimeProfilePreset, SettingsApplyOutcome,
    SettingsCommand, SettingsPaletteApplyOutcome, SettingsPaletteRejectReason, SettingsState,
    ThemePreset,
};
use rldyourterm_shell_integration::{
    FishBaselineFailureCause, ShellLaunchPlan, ShellLaunchProfile, ShellResolution,
    ShellResolutionError, ShellResolutionReason,
};
use serde::{Deserialize, Serialize};

use crate::now_timestamp_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    SessionStarted,
    SessionEnded,
    SessionError,
    RuntimeCommandProcessed,
    RenderCadenceUpdated,
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
            Self::RuntimeCommandProcessed => FoundationDiagnosticKind::RuntimeCommandProcessed,
            Self::RenderCadenceUpdated => FoundationDiagnosticKind::RenderCadenceUpdated,
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
            Self::RuntimeCommandProcessed | Self::RenderCadenceUpdated => {
                FoundationDiagnosticSeverity::Info
            }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsApplySourceKind {
    RuntimeBootstrap,
    PaletteCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCommandSourceKind {
    BootstrapHook,
    MonitorEvent,
    PaletteCommand,
    PtyBoundary,
    GpuFailureHandler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderModeKind {
    Cpu,
    Gpu,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderCadencePolicyKind {
    MonitorAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePresetKind {
    System,
    Cuberpunk,
    Aurora,
    Monochrome,
    Dark,
    Light,
    Solarized,
    Dracula,
    Catppuccin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeProfilePresetKind {
    Balanced,
    Throughput,
    Stability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsStateTypedPayload {
    pub mode: RenderModeKind,
    pub shell_target: ShellTargetKind,
    pub shell_auto_init: bool,
    pub render_cadence_policy: RenderCadencePolicyKind,
    pub theme: ThemePresetKind,
    pub runtime_profile: RuntimeProfilePresetKind,
    pub debug_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsApplyTypedPayload {
    pub source: SettingsApplySourceKind,
    pub step: Option<u32>,
    pub command_input: String,
    pub outcome: SettingsApplyOutcomeKind,
    pub previous_state: SettingsStateTypedPayload,
    pub current_state: Option<SettingsStateTypedPayload>,
    pub reject_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCommandReceiptTypedPayload {
    pub source: RuntimeCommandSourceKind,
    pub step: Option<u32>,
    pub receipt: UiCommandReceipt,
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
    pub fn from_settings_outcome(
        source: SettingsApplySourceKind,
        outcome: &SettingsApplyOutcome,
    ) -> Self {
        match outcome {
            SettingsApplyOutcome::Applied {
                command,
                previous,
                current,
            } => Self {
                source,
                step: None,
                command_input: settings_command_input(command),
                outcome: SettingsApplyOutcomeKind::Applied,
                previous_state: previous.into(),
                current_state: Some(current.into()),
                reject_reason: None,
            },
            SettingsApplyOutcome::Noop { command, state, .. } => Self {
                source,
                step: None,
                command_input: settings_command_input(command),
                outcome: SettingsApplyOutcomeKind::Noop,
                previous_state: state.into(),
                current_state: Some(state.into()),
                reject_reason: None,
            },
            SettingsApplyOutcome::Rejected {
                command,
                state,
                reason,
            } => Self {
                source,
                step: None,
                command_input: settings_command_input(command),
                outcome: SettingsApplyOutcomeKind::Rejected,
                previous_state: state.into(),
                current_state: None,
                reject_reason: Some(format!("{reason:?}")),
            },
        }
    }

    pub fn from_palette_outcome(step: u32, outcome: &SettingsPaletteApplyOutcome) -> Self {
        match outcome {
            SettingsPaletteApplyOutcome::Applied {
                input,
                previous,
                current,
                ..
            } => Self {
                source: SettingsApplySourceKind::PaletteCommand,
                step: Some(step),
                command_input: input.clone(),
                outcome: SettingsApplyOutcomeKind::Applied,
                previous_state: previous.into(),
                current_state: Some(current.into()),
                reject_reason: None,
            },
            SettingsPaletteApplyOutcome::Noop { input, state, .. } => Self {
                source: SettingsApplySourceKind::PaletteCommand,
                step: Some(step),
                command_input: input.clone(),
                outcome: SettingsApplyOutcomeKind::Noop,
                previous_state: state.into(),
                current_state: Some(state.into()),
                reject_reason: None,
            },
            SettingsPaletteApplyOutcome::Rejected {
                input,
                reason,
                state,
            } => Self {
                source: SettingsApplySourceKind::PaletteCommand,
                step: Some(step),
                command_input: input.clone(),
                outcome: SettingsApplyOutcomeKind::Rejected,
                previous_state: state.into(),
                current_state: None,
                reject_reason: Some(match reason {
                    SettingsPaletteRejectReason::Parse(reason) => format!("parse:{reason:?}"),
                    SettingsPaletteRejectReason::Validation(reason) => {
                        format!("validation:{reason:?}")
                    }
                }),
            },
        }
    }

    pub(crate) fn event_kind(&self) -> EventKind {
        match self.outcome {
            SettingsApplyOutcomeKind::Rejected => EventKind::SettingsRejected,
            SettingsApplyOutcomeKind::Applied | SettingsApplyOutcomeKind::Noop => {
                EventKind::SettingsApply
            }
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
        if self.command_input.is_empty() {
            return Err(DiagnosticsPayloadError::InvalidPayload {
                payload: "settings.apply.typed",
                reason: "command_input must be non-empty",
            });
        }
        match self.source {
            SettingsApplySourceKind::RuntimeBootstrap if self.step.is_some() => {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "settings.apply.typed",
                    reason: "runtime-bootstrap payload must not include step",
                });
            }
            SettingsApplySourceKind::PaletteCommand if self.step.is_none() => {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "settings.apply.typed",
                    reason: "palette-command payload requires step",
                });
            }
            _ => {}
        }

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

impl RuntimeCommandReceiptTypedPayload {
    pub fn from_receipt(
        source: RuntimeCommandSourceKind,
        step: Option<u32>,
        receipt: &UiCommandReceipt,
    ) -> Self {
        Self {
            source,
            step,
            receipt: *receipt,
        }
    }

    pub(crate) fn event_kind(&self) -> EventKind {
        match self.receipt.outcome {
            UiCommandOutcome::Noop | UiCommandOutcome::SingleWindowConfirmed { .. } => {
                EventKind::RuntimeCommandProcessed
            }
            UiCommandOutcome::SessionTransition(transition) => match transition.outcome {
                SessionTransitionOutcome::Started { .. } => EventKind::SessionStarted,
                SessionTransitionOutcome::RecoverableBoundary { .. } => EventKind::ResourceWarning,
                SessionTransitionOutcome::FatalBoundary { .. } => EventKind::SessionError,
                SessionTransitionOutcome::StopRequested | SessionTransitionOutcome::Stopped => {
                    EventKind::SessionEnded
                }
            },
            UiCommandOutcome::RenderModeTransition(_) => EventKind::RenderModeTransition,
            UiCommandOutcome::CadenceResynced { .. } => EventKind::RenderCadenceUpdated,
            UiCommandOutcome::GpuRetryScheduled { .. } => EventKind::ResourceWarning,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DiagnosticsPayloadError> {
        match self.source {
            RuntimeCommandSourceKind::BootstrapHook if self.step.is_none() => {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "runtime.command.receipt",
                    reason: "bootstrap-hook payload requires step",
                });
            }
            RuntimeCommandSourceKind::MonitorEvent
            | RuntimeCommandSourceKind::PaletteCommand
            | RuntimeCommandSourceKind::PtyBoundary
            | RuntimeCommandSourceKind::GpuFailureHandler
                if self.step.is_some() =>
            {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "runtime.command.receipt",
                    reason: "non-bootstrap payload must not include step",
                });
            }
            _ => {}
        }

        if self.receipt.window_count == 0 {
            return Err(DiagnosticsPayloadError::InvalidPayload {
                payload: "runtime.command.receipt",
                reason: "receipt.window_count must be greater than zero",
            });
        }

        Ok(())
    }
}

impl ShellResolutionTypedPayload {
    pub fn from_resolution(resolution: ShellResolution) -> Self {
        Self {
            requested: resolution.requested.into(),
            resolved: Some(resolution.resolved.into()),
            fallback_applied: resolution.fallback_applied,
            fallback_cause: resolution.fallback_cause.map(Into::into),
            reason: Some(resolution.reason.into()),
            error: None,
        }
    }

    pub fn from_resolution_failure(
        requested: ShellTarget,
        error: ShellResolutionError,
        fallback_cause: Option<FishBaselineFailureCause>,
    ) -> Self {
        Self {
            requested: requested.into(),
            resolved: None,
            fallback_applied: false,
            fallback_cause: fallback_cause.map(Into::into),
            reason: None,
            error: Some(error.into()),
        }
    }

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
            if self.resolved.is_some() {
                return Err(DiagnosticsPayloadError::InvalidPayload {
                    payload: "shell.resolve.typed",
                    reason: "error payload must not include resolved target",
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

impl ShellLaunchTypedPayload {
    pub fn from_plan(plan: &ShellLaunchPlan) -> Self {
        Self {
            executable: plan.executable.clone(),
            args: plan.args.clone(),
            profile: plan.profile.into(),
        }
    }
}

impl From<RenderMode> for RenderModeKind {
    fn from(value: RenderMode) -> Self {
        match value {
            RenderMode::Cpu => Self::Cpu,
            RenderMode::Gpu => Self::Gpu,
            RenderMode::Auto => Self::Auto,
        }
    }
}

impl From<RenderCadencePolicy> for RenderCadencePolicyKind {
    fn from(value: RenderCadencePolicy) -> Self {
        match value {
            RenderCadencePolicy::MonitorAuto => Self::MonitorAuto,
        }
    }
}

impl From<ThemePreset> for ThemePresetKind {
    fn from(value: ThemePreset) -> Self {
        match value {
            ThemePreset::System => Self::System,
            ThemePreset::Cuberpunk => Self::Cuberpunk,
            ThemePreset::Aurora => Self::Aurora,
            ThemePreset::Monochrome => Self::Monochrome,
            ThemePreset::Dark => Self::Dark,
            ThemePreset::Light => Self::Light,
            ThemePreset::Solarized => Self::Solarized,
            ThemePreset::Dracula => Self::Dracula,
            ThemePreset::Catppuccin => Self::Catppuccin,
        }
    }
}

impl From<RuntimeProfilePreset> for RuntimeProfilePresetKind {
    fn from(value: RuntimeProfilePreset) -> Self {
        match value {
            RuntimeProfilePreset::Balanced => Self::Balanced,
            RuntimeProfilePreset::Throughput => Self::Throughput,
            RuntimeProfilePreset::Stability => Self::Stability,
        }
    }
}

impl From<&SettingsState> for SettingsStateTypedPayload {
    fn from(value: &SettingsState) -> Self {
        Self {
            mode: value.mode.into(),
            shell_target: value.shell_target.into(),
            shell_auto_init: value.shell_auto_init,
            render_cadence_policy: value.render_cadence_policy.into(),
            theme: value.theme.into(),
            runtime_profile: value.runtime_profile.into(),
            debug_mode: value.debug_mode,
        }
    }
}

impl From<ShellTarget> for ShellTargetKind {
    fn from(value: ShellTarget) -> Self {
        match value {
            ShellTarget::Fish => Self::Fish,
            ShellTarget::Zsh => Self::Zsh,
            ShellTarget::Auto => Self::Auto,
        }
    }
}

impl From<FishBaselineFailureCause> for FishBaselineFailureCauseKind {
    fn from(value: FishBaselineFailureCause) -> Self {
        match value {
            FishBaselineFailureCause::FishUnavailable => Self::FishUnavailable,
            FishBaselineFailureCause::StarshipUnavailable => Self::StarshipUnavailable,
            FishBaselineFailureCause::FishAndStarshipUnavailable => {
                Self::FishAndStarshipUnavailable
            }
        }
    }
}

impl From<ShellResolutionReason> for ShellResolutionReasonKind {
    fn from(value: ShellResolutionReason) -> Self {
        match value {
            ShellResolutionReason::FishBaselineReady => Self::FishBaselineReady,
            ShellResolutionReason::FishRequestedFallbackToZsh => Self::FishRequestedFallbackToZsh,
            ShellResolutionReason::AutoSelectedFishBaseline => Self::AutoSelectedFishBaseline,
            ShellResolutionReason::AutoFallbackToZsh => Self::AutoFallbackToZsh,
            ShellResolutionReason::ZshRequested => Self::ZshRequested,
        }
    }
}

impl From<ShellResolutionError> for ShellResolutionErrorKind {
    fn from(value: ShellResolutionError) -> Self {
        match value {
            ShellResolutionError::FishBaselineUnavailableAndZshUnavailable => {
                Self::FishBaselineUnavailableAndZshUnavailable
            }
            ShellResolutionError::ZshRequestedButUnavailable => Self::ZshRequestedButUnavailable,
        }
    }
}

impl From<ShellLaunchProfile> for ShellLaunchProfileKind {
    fn from(value: ShellLaunchProfile) -> Self {
        match value {
            ShellLaunchProfile::FishStarshipBaseline => Self::FishStarshipBaseline,
            ShellLaunchProfile::ZshRequested => Self::ZshRequested,
            ShellLaunchProfile::ZshFallback => Self::ZshFallback,
        }
    }
}

fn settings_command_input(command: &SettingsCommand) -> String {
    match command {
        SettingsCommand::SetMode(mode) => format!("mode {}", render_mode_input(*mode)),
        SettingsCommand::SetShellTarget(target) => {
            format!("shell {}", settings_shell_input(*target))
        }
        SettingsCommand::SetShellAutoInit(enabled) => {
            format!("shell auto-init {}", if *enabled { "on" } else { "off" })
        }
        SettingsCommand::SetRenderCadencePolicy(RenderCadencePolicy::MonitorAuto) => {
            "render cadence monitor-auto".to_owned()
        }
        SettingsCommand::SetFontFallbackPolicy(policy) => {
            format!("font fallback {}", font_fallback_input(*policy))
        }
        SettingsCommand::SetTheme(theme) => format!("theme set {}", theme_input(*theme)),
        SettingsCommand::SetRuntimeProfile(profile) => {
            format!("profile {}", runtime_profile_input(*profile))
        }
        SettingsCommand::SetDebugMode(enabled) => {
            format!("debug {}", if *enabled { "on" } else { "off" })
        }
    }
}

fn font_fallback_input(policy: FontFallbackPolicy) -> &'static str {
    match policy {
        FontFallbackPolicy::BundledOnly => "bundled-only",
        FontFallbackPolicy::System => "system",
    }
}

fn render_mode_input(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Cpu => "cpu",
        RenderMode::Gpu => "gpu",
        RenderMode::Auto => "auto",
    }
}

fn settings_shell_input(target: ShellTarget) -> &'static str {
    match target {
        ShellTarget::Fish => "fish",
        ShellTarget::Zsh => "zsh",
        ShellTarget::Auto => "auto",
    }
}

fn theme_input(theme: ThemePreset) -> &'static str {
    match theme {
        ThemePreset::System => "system",
        ThemePreset::Cuberpunk => "cuberpunk",
        ThemePreset::Aurora => "aurora",
        ThemePreset::Monochrome => "monochrome",
        ThemePreset::Dark => "dark",
        ThemePreset::Light => "light",
        ThemePreset::Solarized => "solarized",
        ThemePreset::Dracula => "dracula",
        ThemePreset::Catppuccin => "catppuccin",
    }
}

fn runtime_profile_input(profile: RuntimeProfilePreset) -> &'static str {
    match profile {
        RuntimeProfilePreset::Balanced => "balanced",
        RuntimeProfilePreset::Throughput => "throughput",
        RuntimeProfilePreset::Stability => "stability",
    }
}
