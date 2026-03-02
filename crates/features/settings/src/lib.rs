use rldyourterm_services::render_mode::RenderMode;
use serde::{Deserialize, Serialize};

pub const RUNTIME_PROFILE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellTarget {
    Fish,
    Zsh,
    Auto,
}

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

pub fn parse_palette_command(input: &str) -> Result<SettingsCommand, SettingsCommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SettingsCommandParseError::EmptyInput);
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let command = tokens[0].to_ascii_lowercase();

    match command.as_str() {
        "mode" => parse_mode_command(&tokens),
        "shell" => parse_shell_command(&tokens),
        "render" => parse_render_command(&tokens),
        "theme" => parse_theme_command(&tokens),
        "profile" => parse_profile_command(&tokens),
        "debug" => parse_debug_command(&tokens),
        _ => Err(SettingsCommandParseError::UnknownCommand { command }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsService {
    state: SettingsState,
}

impl Default for SettingsService {
    fn default() -> Self {
        Self::new(SettingsState::default())
    }
}

impl SettingsService {
    pub fn new(initial_state: SettingsState) -> Self {
        Self {
            state: sanitize_settings_state(initial_state),
        }
    }

    pub fn state(&self) -> &SettingsState {
        &self.state
    }

    pub fn export_runtime_profile_state(&self) -> RuntimeProfileState {
        RuntimeProfileState::from_settings_state(&self.state)
    }

    pub fn apply_runtime_profile_state(
        &mut self,
        profile: RuntimeProfileState,
    ) -> RuntimeProfileApplyOutcome {
        let previous = self.state.clone();
        let candidate = match profile.to_settings_state() {
            Ok(state) => state,
            Err(reason) => {
                tracing::warn!(
                    ?reason,
                    ?profile,
                    "runtime profile rejected; previous valid state preserved"
                );
                return RuntimeProfileApplyOutcome::Rejected {
                    profile,
                    state: previous,
                    reason,
                };
            }
        };

        if candidate == previous {
            return RuntimeProfileApplyOutcome::Noop {
                profile,
                state: previous,
                reason: SettingsApplyNoopReason::Unchanged,
            };
        }

        self.state = candidate.clone();
        RuntimeProfileApplyOutcome::Applied {
            profile,
            previous,
            current: candidate,
        }
    }

    pub fn apply_palette_command(&mut self, input: &str) -> SettingsPaletteApplyOutcome {
        let normalized_input = canonicalize_palette_input(input);
        let parsed = match parse_palette_command(input) {
            Ok(command) => command,
            Err(reason) => {
                return SettingsPaletteApplyOutcome::Rejected {
                    input: normalized_input,
                    state: self.state.clone(),
                    reason: SettingsPaletteRejectReason::Parse(reason),
                };
            }
        };

        match self.apply(parsed) {
            SettingsApplyOutcome::Applied {
                command,
                previous,
                current,
            } => SettingsPaletteApplyOutcome::Applied {
                input: normalized_input,
                command,
                previous,
                current,
            },
            SettingsApplyOutcome::Noop {
                command,
                state,
                reason,
            } => SettingsPaletteApplyOutcome::Noop {
                input: normalized_input,
                command,
                state,
                reason,
            },
            SettingsApplyOutcome::Rejected {
                state,
                reason,
                command: _,
            } => SettingsPaletteApplyOutcome::Rejected {
                input: normalized_input,
                state,
                reason: SettingsPaletteRejectReason::Validation(reason),
            },
        }
    }

    pub fn apply(&mut self, command: SettingsCommand) -> SettingsApplyOutcome {
        let previous = self.state.clone();
        let mut candidate = previous.clone();

        let reject_reason = match command {
            SettingsCommand::SetMode(mode) => {
                candidate.mode = mode;
                None
            }
            SettingsCommand::SetShellTarget(target) => {
                candidate.shell_target = target;
                if matches!(target, ShellTarget::Zsh) {
                    candidate.shell_auto_init = false;
                }
                None
            }
            SettingsCommand::SetShellAutoInit(enabled) => {
                if enabled && matches!(previous.shell_target, ShellTarget::Zsh) {
                    Some(SettingsApplyRejectReason::ShellAutoInitRequiresFishOrAuto)
                } else {
                    candidate.shell_auto_init = enabled;
                    None
                }
            }
            SettingsCommand::SetRenderCadencePolicy(policy) => {
                candidate.render_cadence_policy = policy;
                None
            }
            SettingsCommand::SetTheme(theme) => {
                candidate.theme = theme;
                None
            }
            SettingsCommand::SetRuntimeProfile(profile) => {
                candidate.runtime_profile = profile;
                None
            }
            SettingsCommand::SetDebugMode(enabled) => {
                candidate.debug_mode = enabled;
                None
            }
        };

        if let Some(reason) = reject_reason {
            tracing::warn!(
                ?command,
                ?reason,
                "settings command rejected; previous valid state preserved"
            );
            return SettingsApplyOutcome::Rejected {
                command,
                state: previous,
                reason,
            };
        }

        if candidate == previous {
            return SettingsApplyOutcome::Noop {
                command,
                state: previous,
                reason: SettingsApplyNoopReason::Unchanged,
            };
        }

        self.state = candidate.clone();
        SettingsApplyOutcome::Applied {
            command,
            previous,
            current: candidate,
        }
    }
}

fn parse_mode_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "mode".to_string(),
            expected: "cpu|gpu|auto",
        });
    }

    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "mode".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let mode = match normalize_token(tokens[1]).as_str() {
        "cpu" => RenderMode::Cpu,
        "gpu" => RenderMode::Gpu,
        "auto" => RenderMode::Auto,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "mode",
                value: value.to_string(),
                expected: "cpu|gpu|auto",
            });
        }
    };

    Ok(SettingsCommand::SetMode(mode))
}

fn parse_shell_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "shell".to_string(),
            expected: "fish|zsh|auto|auto-init on|off",
        });
    }

    let shell_option = normalize_token(tokens[1]);
    if shell_option == "auto-init" {
        if tokens.len() < 3 {
            return Err(SettingsCommandParseError::MissingArgument {
                command: "shell auto-init".to_string(),
                expected: "on|off",
            });
        }

        if tokens.len() > 3 {
            return Err(SettingsCommandParseError::UnexpectedTrailingInput {
                command: "shell auto-init".to_string(),
                trailing: normalize_trailing_tokens(tokens, 3),
            });
        }

        let enabled = match normalize_token(tokens[2]).as_str() {
            "on" => true,
            "off" => false,
            value => {
                return Err(SettingsCommandParseError::InvalidValue {
                    field: "shell auto-init",
                    value: value.to_string(),
                    expected: "on|off",
                });
            }
        };
        return Ok(SettingsCommand::SetShellAutoInit(enabled));
    }

    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "shell".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let target = match shell_option.as_str() {
        "fish" => ShellTarget::Fish,
        "zsh" => ShellTarget::Zsh,
        "auto" => ShellTarget::Auto,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "shell",
                value: value.to_string(),
                expected: "fish|zsh|auto|auto-init on|off",
            });
        }
    };
    Ok(SettingsCommand::SetShellTarget(target))
}

fn parse_render_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "render".to_string(),
            expected: "cadence monitor-auto",
        });
    }

    if normalize_token(tokens[1]) != "cadence" {
        return Err(SettingsCommandParseError::InvalidValue {
            field: "render",
            value: normalize_token(tokens[1]),
            expected: "cadence monitor-auto",
        });
    }

    if tokens.len() < 3 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "render cadence".to_string(),
            expected: "monitor-auto",
        });
    }

    if tokens.len() > 3 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "render cadence".to_string(),
            trailing: normalize_trailing_tokens(tokens, 3),
        });
    }

    match normalize_token(tokens[2]).as_str() {
        "monitor-auto" => Ok(SettingsCommand::SetRenderCadencePolicy(
            RenderCadencePolicy::MonitorAuto,
        )),
        value => Err(SettingsCommandParseError::InvalidValue {
            field: "render cadence",
            value: value.to_string(),
            expected: "monitor-auto",
        }),
    }
}

fn parse_theme_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 3 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "theme".to_string(),
            expected: "set <cuberpunk|aurora|monochrome>",
        });
    }
    if normalize_token(tokens[1]) != "set" {
        return Err(SettingsCommandParseError::InvalidValue {
            field: "theme",
            value: normalize_token(tokens[1]),
            expected: "set <cuberpunk|aurora|monochrome>",
        });
    }
    if tokens.len() > 3 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "theme set".to_string(),
            trailing: normalize_trailing_tokens(tokens, 3),
        });
    }

    let theme = match normalize_token(tokens[2]).as_str() {
        "cuberpunk" => ThemePreset::Cuberpunk,
        "aurora" => ThemePreset::Aurora,
        "monochrome" => ThemePreset::Monochrome,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "theme",
                value: value.to_string(),
                expected: "cuberpunk|aurora|monochrome",
            });
        }
    };

    Ok(SettingsCommand::SetTheme(theme))
}

fn parse_profile_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "profile".to_string(),
            expected: "balanced|throughput|stability",
        });
    }
    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "profile".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let profile = match normalize_token(tokens[1]).as_str() {
        "balanced" => RuntimeProfilePreset::Balanced,
        "throughput" => RuntimeProfilePreset::Throughput,
        "stability" => RuntimeProfilePreset::Stability,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "profile",
                value: value.to_string(),
                expected: "balanced|throughput|stability",
            });
        }
    };

    Ok(SettingsCommand::SetRuntimeProfile(profile))
}

fn parse_debug_command(tokens: &[&str]) -> Result<SettingsCommand, SettingsCommandParseError> {
    if tokens.len() < 2 {
        return Err(SettingsCommandParseError::MissingArgument {
            command: "debug".to_string(),
            expected: "on|off",
        });
    }
    if tokens.len() > 2 {
        return Err(SettingsCommandParseError::UnexpectedTrailingInput {
            command: "debug".to_string(),
            trailing: normalize_trailing_tokens(tokens, 2),
        });
    }

    let enabled = match normalize_token(tokens[1]).as_str() {
        "on" => true,
        "off" => false,
        value => {
            return Err(SettingsCommandParseError::InvalidValue {
                field: "debug",
                value: value.to_string(),
                expected: "on|off",
            });
        }
    };

    Ok(SettingsCommand::SetDebugMode(enabled))
}

fn normalize_token(token: &str) -> String {
    token.trim().to_ascii_lowercase()
}

fn normalize_trailing_tokens(tokens: &[&str], start: usize) -> String {
    tokens[start..]
        .iter()
        .map(|token| normalize_token(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonicalize_palette_input(input: &str) -> String {
    input
        .split_whitespace()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_settings_state(mut state: SettingsState) -> SettingsState {
    if state.shell_auto_init && matches!(state.shell_target, ShellTarget::Zsh) {
        tracing::warn!(
            shell_target = ?state.shell_target,
            "invalid initial settings state normalized: disabling shell auto-init for zsh target"
        );
        state.shell_auto_init = false;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_change_returns_explicit_applied_outcome() {
        let mut service = SettingsService::default();

        let outcome = service.apply(SettingsCommand::SetMode(RenderMode::Cpu));

        match outcome {
            SettingsApplyOutcome::Applied {
                command,
                previous,
                current,
            } => {
                assert_eq!(command, SettingsCommand::SetMode(RenderMode::Cpu));
                assert_eq!(previous.mode, RenderMode::Auto);
                assert_eq!(current.mode, RenderMode::Cpu);
                assert_eq!(service.state().mode, RenderMode::Cpu);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn repeated_command_returns_noop_outcome() {
        let mut service = SettingsService::default();

        let outcome = service.apply(SettingsCommand::SetMode(RenderMode::Auto));

        assert_eq!(
            outcome,
            SettingsApplyOutcome::Noop {
                command: SettingsCommand::SetMode(RenderMode::Auto),
                state: SettingsState::default(),
                reason: SettingsApplyNoopReason::Unchanged,
            }
        );
    }

    #[test]
    fn setting_zsh_target_disables_shell_auto_init() {
        let mut service = SettingsService::default();
        assert!(service.state().shell_auto_init);

        let outcome = service.apply(SettingsCommand::SetShellTarget(ShellTarget::Zsh));

        match outcome {
            SettingsApplyOutcome::Applied { current, .. } => {
                assert_eq!(current.shell_target, ShellTarget::Zsh);
                assert!(!current.shell_auto_init);
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn shell_auto_init_enable_on_zsh_is_rejected_without_state_change() {
        let mut service = SettingsService::new(SettingsState {
            shell_target: ShellTarget::Zsh,
            shell_auto_init: false,
            ..SettingsState::default()
        });

        let outcome = service.apply(SettingsCommand::SetShellAutoInit(true));

        assert_eq!(
            outcome,
            SettingsApplyOutcome::Rejected {
                command: SettingsCommand::SetShellAutoInit(true),
                state: SettingsState {
                    shell_target: ShellTarget::Zsh,
                    shell_auto_init: false,
                    ..SettingsState::default()
                },
                reason: SettingsApplyRejectReason::ShellAutoInitRequiresFishOrAuto,
            }
        );
        assert_eq!(
            service.state(),
            &SettingsState {
                shell_target: ShellTarget::Zsh,
                shell_auto_init: false,
                ..SettingsState::default()
            }
        );
    }

    #[test]
    fn parser_accepts_mode_and_shell_commands() {
        assert_eq!(
            parse_palette_command("mode gpu").unwrap(),
            SettingsCommand::SetMode(RenderMode::Gpu)
        );
        assert_eq!(
            parse_palette_command("shell auto-init on").unwrap(),
            SettingsCommand::SetShellAutoInit(true)
        );
        assert_eq!(
            parse_palette_command("render cadence monitor-auto").unwrap(),
            SettingsCommand::SetRenderCadencePolicy(RenderCadencePolicy::MonitorAuto)
        );
        assert_eq!(
            parse_palette_command("  MODE\tGPU\n").unwrap(),
            SettingsCommand::SetMode(RenderMode::Gpu)
        );
        assert_eq!(
            parse_palette_command("theme set cuberpunk").unwrap(),
            SettingsCommand::SetTheme(ThemePreset::Cuberpunk)
        );
        assert_eq!(
            parse_palette_command("profile throughput").unwrap(),
            SettingsCommand::SetRuntimeProfile(RuntimeProfilePreset::Throughput)
        );
        assert_eq!(
            parse_palette_command("debug on").unwrap(),
            SettingsCommand::SetDebugMode(true)
        );
    }

    #[test]
    fn parser_rejects_invalid_theme_value() {
        let err = parse_palette_command("theme set neon").unwrap_err();
        assert_eq!(
            err,
            SettingsCommandParseError::InvalidValue {
                field: "theme",
                value: "neon".to_string(),
                expected: "cuberpunk|aurora|monochrome",
            }
        );
    }

    #[test]
    fn invalid_palette_command_does_not_mutate_runtime_state() {
        let mut service = SettingsService::default();
        let previous = service.state().clone();

        let outcome = service.apply_palette_command("mode boosted");

        assert_eq!(
            outcome,
            SettingsPaletteApplyOutcome::Rejected {
                input: "mode boosted".to_string(),
                state: previous.clone(),
                reason: SettingsPaletteRejectReason::Parse(
                    SettingsCommandParseError::InvalidValue {
                        field: "mode",
                        value: "boosted".to_string(),
                        expected: "cpu|gpu|auto",
                    }
                ),
            }
        );
        assert_eq!(service.state(), &previous);
    }

    #[test]
    fn palette_apply_outcome_uses_canonical_input_representation() {
        let mut service = SettingsService::default();

        let outcome = service.apply_palette_command("  MODE\tGPU  ");

        match outcome {
            SettingsPaletteApplyOutcome::Applied { input, .. } => {
                assert_eq!(input, "mode gpu");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn constructor_normalizes_invalid_initial_zsh_auto_init_state() {
        let service = SettingsService::new(SettingsState {
            shell_target: ShellTarget::Zsh,
            shell_auto_init: true,
            ..SettingsState::default()
        });

        assert_eq!(
            service.state(),
            &SettingsState {
                shell_target: ShellTarget::Zsh,
                shell_auto_init: false,
                ..SettingsState::default()
            }
        );
    }

    #[test]
    fn parser_reports_canonicalized_trailing_input() {
        let err = parse_palette_command("mode cpu EXTRA\tToken").unwrap_err();
        assert_eq!(
            err,
            SettingsCommandParseError::UnexpectedTrailingInput {
                command: "mode".to_string(),
                trailing: "extra token".to_string(),
            }
        );
    }

    #[test]
    fn runtime_profile_roundtrip_is_typed_and_stable() {
        let state = SettingsState {
            mode: RenderMode::Gpu,
            shell_target: ShellTarget::Fish,
            shell_auto_init: true,
            render_cadence_policy: RenderCadencePolicy::MonitorAuto,
            theme: ThemePreset::Aurora,
            runtime_profile: RuntimeProfilePreset::Throughput,
            debug_mode: true,
        };

        let profile = RuntimeProfileState::from_settings_state(&state);
        assert_eq!(profile.schema_version, RUNTIME_PROFILE_SCHEMA_VERSION);
        assert_eq!(profile.to_settings_state().unwrap(), state);
    }

    #[test]
    fn invalid_runtime_profile_is_rejected_without_state_mutation() {
        let mut service = SettingsService::default();
        let previous = service.state().clone();
        let profile = RuntimeProfileState {
            schema_version: RUNTIME_PROFILE_SCHEMA_VERSION,
            mode: PersistedRenderMode::Auto,
            shell_target: ShellTarget::Zsh,
            shell_auto_init: true,
            render_cadence_policy: RenderCadencePolicy::MonitorAuto,
            theme: ThemePreset::Cuberpunk,
            runtime_profile: RuntimeProfilePreset::Balanced,
            debug_mode: false,
        };

        let outcome = service.apply_runtime_profile_state(profile.clone());
        assert_eq!(
            outcome,
            RuntimeProfileApplyOutcome::Rejected {
                profile,
                state: previous.clone(),
                reason: RuntimeProfileValidationError::ShellAutoInitRequiresFishOrAuto,
            }
        );
        assert_eq!(service.state(), &previous);
    }
}
