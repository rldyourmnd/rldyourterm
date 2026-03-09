// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::{
    RuntimeProfileApplyOutcome, RuntimeProfileState, SettingsApplyNoopReason, SettingsApplyOutcome,
    SettingsApplyRejectReason, SettingsCommand, SettingsPaletteApplyOutcome,
    SettingsPaletteRejectReason, SettingsState, ShellTarget, parse::canonicalize_palette_input,
    parse_palette_command,
};

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
