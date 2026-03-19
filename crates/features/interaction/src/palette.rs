// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_services::render_mode::{ActiveRenderPath, RenderMode};
use rldyourterm_settings::{
    SettingsCommand, SettingsPaletteApplyOutcome, SettingsService, ThemePreset,
};
use tracing::warn;

use crate::RuntimeKey;

pub const RUNTIME_PALETTE_HELP_LINE: &str =
    "[palette] 1:mode cpu 2:mode gpu 3:mode auto d:diagnostics toggle i:info Esc:close";
pub const RUNTIME_PALETTE_CLOSED_LINE: &str = "[palette] closed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePaletteAction {
    ApplyCommand(&'static str),
    ShowInfo,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaletteDispatchResult {
    pub command: Option<SettingsCommand>,
    pub message: String,
    pub updated_mode: Option<RenderMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaletteDecision {
    pub consumed: bool,
    pub next_open: bool,
    pub notice: Option<String>,
    pub dispatch: Option<RuntimePaletteDispatchResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePaletteView {
    pub mode: RenderMode,
    pub diagnostics_enabled: bool,
    pub active_render_path: Option<ActiveRenderPath>,
}

pub fn toggle_runtime_palette(open: bool) -> RuntimePaletteDecision {
    let next_open = !open;
    RuntimePaletteDecision {
        consumed: true,
        next_open,
        notice: Some(if next_open {
            RUNTIME_PALETTE_HELP_LINE.to_owned()
        } else {
            RUNTIME_PALETTE_CLOSED_LINE.to_owned()
        }),
        dispatch: None,
    }
}

pub fn handle_runtime_palette_key_input(
    palette_open: bool,
    key: Option<RuntimeKey>,
    settings: &mut SettingsService,
    view: RuntimePaletteView,
) -> RuntimePaletteDecision {
    if !palette_open {
        return RuntimePaletteDecision {
            consumed: false,
            next_open: false,
            notice: None,
            dispatch: None,
        };
    }

    let Some(key) = key else {
        return RuntimePaletteDecision {
            consumed: true,
            next_open: true,
            notice: None,
            dispatch: None,
        };
    };

    let Some(action) = runtime_palette_action_for_key(key, view.diagnostics_enabled) else {
        return RuntimePaletteDecision {
            consumed: true,
            next_open: true,
            notice: None,
            dispatch: None,
        };
    };

    match action {
        RuntimePaletteAction::Close => RuntimePaletteDecision {
            consumed: true,
            next_open: false,
            notice: Some(RUNTIME_PALETTE_CLOSED_LINE.to_owned()),
            dispatch: None,
        },
        RuntimePaletteAction::ShowInfo => RuntimePaletteDecision {
            consumed: true,
            next_open: true,
            notice: Some(runtime_palette_info_line(view)),
            dispatch: None,
        },
        RuntimePaletteAction::ApplyCommand(input) => {
            let dispatch =
                dispatch_runtime_palette_command(settings, input, view.active_render_path);
            RuntimePaletteDecision {
                consumed: true,
                next_open: false,
                notice: Some(dispatch.message.clone()),
                dispatch: Some(dispatch),
            }
        }
    }
}

fn runtime_palette_action_for_key(
    key: RuntimeKey,
    diagnostics_enabled: bool,
) -> Option<RuntimePaletteAction> {
    match key {
        RuntimeKey::Escape => Some(RuntimePaletteAction::Close),
        RuntimeKey::Character('1') => Some(RuntimePaletteAction::ApplyCommand("mode cpu")),
        RuntimeKey::Character('2') => Some(RuntimePaletteAction::ApplyCommand("mode gpu")),
        RuntimeKey::Character('3') => Some(RuntimePaletteAction::ApplyCommand("mode auto")),
        RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'d') => {
            if diagnostics_enabled {
                Some(RuntimePaletteAction::ApplyCommand("debug off"))
            } else {
                Some(RuntimePaletteAction::ApplyCommand("debug on"))
            }
        }
        RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'i') => {
            Some(RuntimePaletteAction::ShowInfo)
        }
        _ => None,
    }
}

pub fn dispatch_runtime_palette_command(
    settings: &mut SettingsService,
    input: &str,
    active_render_path: Option<ActiveRenderPath>,
) -> RuntimePaletteDispatchResult {
    match settings.apply_palette_command(input) {
        SettingsPaletteApplyOutcome::Applied {
            command, current, ..
        } => runtime_palette_dispatch_result(
            Some(command),
            command,
            current.mode,
            current.debug_mode,
            active_render_path,
        ),
        SettingsPaletteApplyOutcome::Noop { command, state, .. } => {
            runtime_palette_dispatch_result(
                Some(command),
                command,
                state.mode,
                state.debug_mode,
                active_render_path,
            )
        }
        SettingsPaletteApplyOutcome::Rejected { reason, .. } => {
            warn!(?reason, input = input, "runtime palette command rejected");
            RuntimePaletteDispatchResult {
                command: None,
                message: format!("[palette] rejected input={input} reason={reason:?}"),
                updated_mode: None,
            }
        }
    }
}

pub fn runtime_palette_info_line(view: RuntimePaletteView) -> String {
    match view.active_render_path {
        Some(active_render_path) => format!(
            "[palette] info mode={} active-path={} diagnostics={}",
            render_mode_token(view.mode),
            active_render_path_token(active_render_path),
            on_off_token(view.diagnostics_enabled),
        ),
        None => format!(
            "[palette] info mode={} diagnostics={}",
            render_mode_token(view.mode),
            on_off_token(view.diagnostics_enabled),
        ),
    }
}

fn runtime_palette_dispatch_result(
    command_for_ui: Option<SettingsCommand>,
    command: SettingsCommand,
    mode: RenderMode,
    diagnostics_enabled: bool,
    active_render_path: Option<ActiveRenderPath>,
) -> RuntimePaletteDispatchResult {
    let message =
        runtime_palette_status_line(command, mode, diagnostics_enabled, active_render_path);
    let updated_mode = matches!(command, SettingsCommand::SetMode(_)).then_some(mode);
    RuntimePaletteDispatchResult {
        command: command_for_ui,
        message,
        updated_mode,
    }
}

pub fn runtime_palette_status_line(
    command: SettingsCommand,
    mode: RenderMode,
    diagnostics_enabled: bool,
    active_render_path: Option<ActiveRenderPath>,
) -> String {
    match command {
        SettingsCommand::SetMode(_) => match active_render_path {
            Some(active_render_path) => format!(
                "[palette] mode={} active-path={}",
                render_mode_token(mode),
                active_render_path_token(active_render_path),
            ),
            None => format!(
                "[palette] mode={} diagnostics={}",
                render_mode_token(mode),
                on_off_token(diagnostics_enabled),
            ),
        },
        SettingsCommand::SetDebugMode(_) => match active_render_path {
            Some(active_render_path) => format!(
                "[palette] diagnostics={} mode={} active-path={}",
                on_off_token(diagnostics_enabled),
                render_mode_token(mode),
                active_render_path_token(active_render_path),
            ),
            None => format!(
                "[palette] diagnostics={} mode={}",
                on_off_token(diagnostics_enabled),
                render_mode_token(mode),
            ),
        },
        SettingsCommand::SetTheme(theme) => match active_render_path {
            Some(active_render_path) => format!(
                "[palette] theme={} active-path={}",
                theme_preset_token(theme),
                active_render_path_token(active_render_path),
            ),
            None => format!("[palette] saved (restart required) input={command:?}"),
        },
        SettingsCommand::SetShellTarget(_)
        | SettingsCommand::SetShellAutoInit(_)
        | SettingsCommand::SetRenderCadencePolicy(_)
        | SettingsCommand::SetRuntimeProfile(_) => {
            format!("[palette] saved (restart required) input={command:?}")
        }
    }
}

fn active_render_path_token(active_render_path: ActiveRenderPath) -> &'static str {
    match active_render_path {
        ActiveRenderPath::Cpu => "cpu",
        ActiveRenderPath::Gpu => "gpu",
    }
}

fn render_mode_token(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Cpu => "cpu",
        RenderMode::Gpu => "gpu",
        RenderMode::Auto => "auto",
    }
}

fn theme_preset_token(theme: ThemePreset) -> &'static str {
    match theme {
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

fn on_off_token(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::{
        RUNTIME_PALETTE_CLOSED_LINE, RUNTIME_PALETTE_HELP_LINE, RuntimePaletteView,
        handle_runtime_palette_key_input, toggle_runtime_palette,
    };
    use crate::RuntimeKey;
    use rldyourterm_services::render_mode::{ActiveRenderPath, RenderMode};
    use rldyourterm_settings::SettingsService;

    #[test]
    fn toggle_runtime_palette_emits_open_and_close_notices() {
        let opened = toggle_runtime_palette(false);
        assert!(opened.consumed);
        assert!(opened.next_open);
        assert_eq!(opened.notice.as_deref(), Some(RUNTIME_PALETTE_HELP_LINE));

        let closed = toggle_runtime_palette(true);
        assert!(closed.consumed);
        assert!(!closed.next_open);
        assert_eq!(closed.notice.as_deref(), Some(RUNTIME_PALETTE_CLOSED_LINE));
    }

    #[test]
    fn handle_runtime_palette_key_input_consumes_unknown_keys_while_open() {
        let mut settings = SettingsService::default();
        let decision = handle_runtime_palette_key_input(
            true,
            Some(RuntimeKey::Character('x')),
            &mut settings,
            RuntimePaletteView {
                mode: RenderMode::Auto,
                diagnostics_enabled: false,
                active_render_path: Some(ActiveRenderPath::Gpu),
            },
        );

        assert!(decision.consumed);
        assert!(decision.next_open);
        assert!(decision.notice.is_none());
        assert!(decision.dispatch.is_none());
    }

    #[test]
    fn handle_runtime_palette_key_input_applies_mode_and_closes_palette() {
        let mut settings = SettingsService::default();
        let decision = handle_runtime_palette_key_input(
            true,
            Some(RuntimeKey::Character('1')),
            &mut settings,
            RuntimePaletteView {
                mode: RenderMode::Auto,
                diagnostics_enabled: false,
                active_render_path: Some(ActiveRenderPath::Gpu),
            },
        );

        assert!(decision.consumed);
        assert!(!decision.next_open);
        assert_eq!(settings.state().mode, RenderMode::Cpu);
        assert_eq!(
            decision
                .dispatch
                .as_ref()
                .and_then(|dispatch| dispatch.updated_mode),
            Some(RenderMode::Cpu)
        );
        assert_eq!(
            decision.notice.as_deref(),
            Some("[palette] mode=cpu active-path=gpu")
        );
    }
}
