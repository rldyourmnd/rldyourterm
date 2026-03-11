// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_services::render_mode::{ActiveRenderPath, RenderMode};
use rldyourterm_settings::{
    SettingsCommand, SettingsPaletteApplyOutcome, SettingsService, parse_palette_command,
};
use tracing::warn;

use crate::runtime_shared::display::{on_off_token, render_mode_token};
use crate::runtime_shared::input::RuntimeKey;

pub(crate) const RUNTIME_PALETTE_HELP_LINE: &str =
    "[palette] 1:mode cpu 2:mode gpu 3:mode auto d:diagnostics toggle i:info Esc:close";
pub(crate) const RUNTIME_PALETTE_CLOSED_LINE: &str = "[palette] closed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimePaletteAction {
    ApplyCommand(&'static str),
    ShowInfo,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePaletteDispatchResult {
    pub command: Option<SettingsCommand>,
    pub message: String,
    pub updated_mode: Option<RenderMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePaletteDecision {
    pub consumed: bool,
    pub next_open: bool,
    pub notice: Option<String>,
    pub dispatch: Option<RuntimePaletteDispatchResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimePaletteView {
    pub mode: RenderMode,
    pub diagnostics_enabled: bool,
    pub active_render_path: Option<ActiveRenderPath>,
}

pub(crate) fn toggle_runtime_palette(open: bool) -> RuntimePaletteDecision {
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

pub(crate) fn handle_runtime_palette_key_input(
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

pub(crate) fn runtime_palette_action_for_key(
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

pub(crate) fn dispatch_runtime_palette_command(
    settings: &mut SettingsService,
    input: &str,
    active_render_path: Option<ActiveRenderPath>,
) -> RuntimePaletteDispatchResult {
    let input = canonical_runtime_palette_input(input);
    let parsed = match parse_palette_command(&input) {
        Ok(parsed) => parsed,
        Err(reason) => {
            warn!(?reason, input = input, "runtime palette command rejected");
            return RuntimePaletteDispatchResult {
                command: None,
                message: format!("[palette] rejected input={input} reason={reason:?}"),
                updated_mode: None,
            };
        }
    };
    if !runtime_palette_command_supported(parsed) {
        warn!(
            ?parsed,
            input = input,
            "runtime palette command rejected as unsupported in the live runtime"
        );
        return RuntimePaletteDispatchResult {
            command: None,
            message: format!("[palette] rejected input={input} reason=UnsupportedInRuntimePalette"),
            updated_mode: None,
        };
    }

    match settings.apply_palette_command(&input) {
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

pub(crate) fn runtime_palette_info_line(view: RuntimePaletteView) -> String {
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

pub(crate) fn runtime_palette_dispatch_result(
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

pub(crate) fn runtime_palette_status_line(
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
        SettingsCommand::SetShellTarget(_)
        | SettingsCommand::SetShellAutoInit(_)
        | SettingsCommand::SetRenderCadencePolicy(_)
        | SettingsCommand::SetTheme(_)
        | SettingsCommand::SetRuntimeProfile(_) => {
            format!("[palette] rejected input={command:?} reason=UnsupportedInRuntimePalette")
        }
    }
}

fn runtime_palette_command_supported(command: SettingsCommand) -> bool {
    matches!(
        command,
        SettingsCommand::SetMode(_) | SettingsCommand::SetDebugMode(_)
    )
}

fn canonical_runtime_palette_input(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn active_render_path_token(active_render_path: ActiveRenderPath) -> &'static str {
    match active_render_path {
        ActiveRenderPath::Cpu => "cpu",
        ActiveRenderPath::Gpu => "gpu",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RUNTIME_PALETTE_CLOSED_LINE, RUNTIME_PALETTE_HELP_LINE, RuntimePaletteView,
        dispatch_runtime_palette_command, handle_runtime_palette_key_input, toggle_runtime_palette,
    };
    use crate::runtime_shared::input::RuntimeKey;
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

    #[test]
    fn dispatch_runtime_palette_command_rejects_unsupported_command_without_mutation() {
        let mut settings = SettingsService::default();
        let initial = settings.state().clone();

        let result =
            dispatch_runtime_palette_command(&mut settings, "theme set aurora", None);

        assert_eq!(settings.state(), &initial);
        assert!(result.command.is_none());
        assert!(result.updated_mode.is_none());
        assert!(
            result
                .message
                .contains("reason=UnsupportedInRuntimePalette")
        );
        assert!(result.message.contains("input=theme set aurora"));
    }
}
