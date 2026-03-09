// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_services::render_mode::RenderMode;

use crate::{
    PersistedRenderMode, RUNTIME_PROFILE_SCHEMA_VERSION, RenderCadencePolicy,
    RuntimeProfileApplyOutcome, RuntimeProfilePreset, RuntimeProfileState,
    RuntimeProfileValidationError, SettingsApplyNoopReason, SettingsApplyOutcome,
    SettingsApplyRejectReason, SettingsCommand, SettingsCommandParseError,
    SettingsPaletteApplyOutcome, SettingsPaletteRejectReason, SettingsService, SettingsState,
    ShellTarget, ThemePreset, parse_palette_command,
};

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
            reason: SettingsPaletteRejectReason::Parse(SettingsCommandParseError::InvalidValue {
                field: "mode",
                value: "boosted".to_string(),
                expected: "cpu|gpu|auto",
            }),
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
