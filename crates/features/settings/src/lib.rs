// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod parse;
mod service;
mod types;

#[cfg(test)]
mod tests;

pub use parse::parse_palette_command;
pub use service::SettingsService;
pub use types::{
    PersistedRenderMode, RUNTIME_PROFILE_SCHEMA_VERSION, RenderCadencePolicy,
    RuntimeProfileApplyOutcome, RuntimeProfilePreset, RuntimeProfileState,
    RuntimeProfileValidationError, SettingsApplyNoopReason, SettingsApplyOutcome,
    SettingsApplyRejectReason, SettingsCommand, SettingsCommandParseError,
    SettingsPaletteApplyOutcome, SettingsPaletteRejectReason, SettingsState, ShellTarget,
    ThemePreset,
};
