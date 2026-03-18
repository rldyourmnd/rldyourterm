// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_services::render_mode::RenderMode;
use serde::{Deserialize, Serialize};

pub const RUNTIME_PROFILE_SCHEMA_VERSION: u16 = 1;

pub use rldyourterm_services::shell_target::ShellTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderCadencePolicy {
    MonitorAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    Cuberpunk,
    Aurora,
    Monochrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeProfilePreset {
    Balanced,
    Throughput,
    Stability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistedRenderMode {
    Cpu,
    Gpu,
    Auto,
}

impl From<RenderMode> for PersistedRenderMode {
    fn from(value: RenderMode) -> Self {
        match value {
            RenderMode::Cpu => Self::Cpu,
            RenderMode::Gpu => Self::Gpu,
            RenderMode::Auto => Self::Auto,
        }
    }
}

impl From<PersistedRenderMode> for RenderMode {
    fn from(value: PersistedRenderMode) -> Self {
        match value {
            PersistedRenderMode::Cpu => Self::Cpu,
            PersistedRenderMode::Gpu => Self::Gpu,
            PersistedRenderMode::Auto => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCommand {
    SetMode(RenderMode),
    SetShellTarget(ShellTarget),
    SetShellAutoInit(bool),
    SetRenderCadencePolicy(RenderCadencePolicy),
    SetTheme(ThemePreset),
    SetRuntimeProfile(RuntimeProfilePreset),
    SetDebugMode(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsState {
    pub mode: RenderMode,
    pub shell_target: ShellTarget,
    pub shell_auto_init: bool,
    pub render_cadence_policy: RenderCadencePolicy,
    pub theme: ThemePreset,
    pub runtime_profile: RuntimeProfilePreset,
    pub debug_mode: bool,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            mode: RenderMode::Auto,
            shell_target: ShellTarget::Auto,
            shell_auto_init: true,
            render_cadence_policy: RenderCadencePolicy::MonitorAuto,
            theme: ThemePreset::Cuberpunk,
            runtime_profile: RuntimeProfilePreset::Balanced,
            debug_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProfileState {
    pub schema_version: u16,
    pub mode: PersistedRenderMode,
    pub shell_target: ShellTarget,
    pub shell_auto_init: bool,
    pub render_cadence_policy: RenderCadencePolicy,
    pub theme: ThemePreset,
    pub runtime_profile: RuntimeProfilePreset,
    pub debug_mode: bool,
}

impl RuntimeProfileState {
    pub fn from_settings_state(state: &SettingsState) -> Self {
        Self {
            schema_version: RUNTIME_PROFILE_SCHEMA_VERSION,
            mode: state.mode.into(),
            shell_target: state.shell_target,
            shell_auto_init: state.shell_auto_init,
            render_cadence_policy: state.render_cadence_policy,
            theme: state.theme,
            runtime_profile: state.runtime_profile,
            debug_mode: state.debug_mode,
        }
    }

    pub fn validate(&self) -> Result<(), RuntimeProfileValidationError> {
        if self.schema_version != RUNTIME_PROFILE_SCHEMA_VERSION {
            return Err(RuntimeProfileValidationError::UnsupportedSchemaVersion {
                expected: RUNTIME_PROFILE_SCHEMA_VERSION,
                actual: self.schema_version,
            });
        }

        if self.shell_auto_init && matches!(self.shell_target, ShellTarget::Zsh) {
            return Err(RuntimeProfileValidationError::ShellAutoInitRequiresFishOrAuto);
        }

        Ok(())
    }

    pub fn to_settings_state(&self) -> Result<SettingsState, RuntimeProfileValidationError> {
        self.validate()?;
        Ok(SettingsState {
            mode: self.mode.into(),
            shell_target: self.shell_target,
            shell_auto_init: self.shell_auto_init,
            render_cadence_policy: self.render_cadence_policy,
            theme: self.theme,
            runtime_profile: self.runtime_profile,
            debug_mode: self.debug_mode,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfileValidationError {
    UnsupportedSchemaVersion { expected: u16, actual: u16 },
    ShellAutoInitRequiresFishOrAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsApplyNoopReason {
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsApplyRejectReason {
    ShellAutoInitRequiresFishOrAuto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsApplyOutcome {
    Applied {
        command: SettingsCommand,
        previous: SettingsState,
        current: SettingsState,
    },
    Noop {
        command: SettingsCommand,
        state: SettingsState,
        reason: SettingsApplyNoopReason,
    },
    Rejected {
        command: SettingsCommand,
        state: SettingsState,
        reason: SettingsApplyRejectReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProfileApplyOutcome {
    Applied {
        profile: RuntimeProfileState,
        previous: SettingsState,
        current: SettingsState,
    },
    Noop {
        profile: RuntimeProfileState,
        state: SettingsState,
        reason: SettingsApplyNoopReason,
    },
    Rejected {
        profile: RuntimeProfileState,
        state: SettingsState,
        reason: RuntimeProfileValidationError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsCommandParseError {
    EmptyInput,
    UnknownCommand {
        command: String,
    },
    UnsupportedCommandNamespace {
        command: String,
    },
    MissingArgument {
        command: String,
        expected: &'static str,
    },
    InvalidValue {
        field: &'static str,
        value: String,
        expected: &'static str,
    },
    UnexpectedTrailingInput {
        command: String,
        trailing: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsPaletteRejectReason {
    Parse(SettingsCommandParseError),
    Validation(SettingsApplyRejectReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsPaletteApplyOutcome {
    Applied {
        input: String,
        command: SettingsCommand,
        previous: SettingsState,
        current: SettingsState,
    },
    Noop {
        input: String,
        command: SettingsCommand,
        state: SettingsState,
        reason: SettingsApplyNoopReason,
    },
    Rejected {
        input: String,
        state: SettingsState,
        reason: SettingsPaletteRejectReason,
    },
}
