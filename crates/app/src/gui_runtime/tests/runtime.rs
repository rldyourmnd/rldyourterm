// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::super::{
    DEFAULT_FG, DEFAULT_FG_U32, GpuFailureHandling, MonitorAffectingWindowEvent,
    cadence_resync_command_for_monitor_event, dispatch_gpu_failure_command,
    dispatch_runtime_palette_command, emit_gpu_auto_fallback_observability,
    is_runtime_palette_shortcut_key, resolve_cell_colors, sample_monitor_refresh_rate_millihz,
};
use super::{StubWindowControl, StubWindowControlScenario, test_ui_runtime};
use crate::runtime_shared::pty_boundary::{
    PtyBoundaryPolicyDecision, classify_pty_boundary_failure,
};
use rldyourterm_diagnostics::{DiagnosticsSink, EventKind};
use rldyourterm_services::render_mode::{ActiveRenderPath, GpuFailureKind, RenderMode};
use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
use rldyourterm_services::terminal::{ANSI_PALETTE, Attrs, Color, color_to_u32};
use rldyourterm_settings::SettingsService;
use rldyourterm_ui::UiRuntimeCommand;
use winit::keyboard::{Key, ModifiersState};

#[test]
fn detects_palette_shortcut_with_ctrl_or_cmd_shift_p() {
    let key = Key::Character("p".into());
    assert!(is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::CONTROL | ModifiersState::SHIFT
    ));
    assert!(is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::SUPER | ModifiersState::SHIFT
    ));
    assert!(!is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::CONTROL
    ));
    assert!(!is_runtime_palette_shortcut_key(
        key.as_ref(),
        ModifiersState::SHIFT
    ));
}

#[test]
fn palette_dispatch_updates_render_mode_via_runtime_path() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    let mut settings = SettingsService::default();

    let message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "mode cpu")
        .expect("dispatch mode cpu");
    assert!(message.contains("mode=cpu"));
    assert_eq!(settings.state().mode, RenderMode::Cpu);
    assert_eq!(ui_runtime.render_mode(), RenderMode::Cpu);
}

#[test]
fn palette_dispatch_toggles_diagnostics_state() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    let mut settings = SettingsService::default();

    let on_message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug on")
        .expect("dispatch debug on");
    assert!(on_message.contains("diagnostics=on"));
    assert!(settings.state().debug_mode);

    let off_message = dispatch_runtime_palette_command(&mut ui_runtime, &mut settings, "debug off")
        .expect("dispatch debug off");
    assert!(off_message.contains("diagnostics=off"));
    assert!(!settings.state().debug_mode);
}

#[test]
fn injected_gpu_failure_falls_back_without_forcing_exit_in_auto_mode() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Auto);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let (_, first) =
        dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 10)
            .expect("first gpu failure");
    assert_eq!(
        first,
        GpuFailureHandling::RetryScheduled {
            failure_streak: 1,
            retry_budget_remaining: 1
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let (_, second) =
        dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SubmitError, 20)
            .expect("second gpu failure");
    assert_eq!(
        second,
        GpuFailureHandling::RetryScheduled {
            failure_streak: 2,
            retry_budget_remaining: 0
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let (_, third) =
        dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SwapchainOutOfDate, 30)
            .expect("third gpu failure");
    assert_eq!(
        third,
        GpuFailureHandling::FallbackToCpu {
            transition_sequence: 1
        }
    );
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Cpu);
}

#[test]
fn forced_gpu_mode_reports_explicit_gpu_failure() {
    let mut ui_runtime = test_ui_runtime(RenderMode::Gpu);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);

    let (_, decision) =
        dispatch_gpu_failure_command(&mut ui_runtime, GpuFailureKind::SurfaceError, 7)
            .expect("gpu failure decision");
    assert_eq!(decision, GpuFailureHandling::FatalForcedGpu);
    assert_eq!(ui_runtime.active_render_path(), ActiveRenderPath::Gpu);
}

#[test]
fn monitor_timing_sampling_uses_window_control_contract() {
    let control = StubWindowControl {
        scenario: StubWindowControlScenario::Timing(Some(144_000)),
    };
    assert_eq!(
        sample_monitor_refresh_rate_millihz(Some(&control)),
        Some(144_000)
    );
}

#[test]
fn monitor_timing_sampling_returns_none_when_contract_fails() {
    let control = StubWindowControl {
        scenario: StubWindowControlScenario::Error,
    };
    assert_eq!(sample_monitor_refresh_rate_millihz(Some(&control)), None);
}

#[test]
fn monitor_affecting_events_emit_expected_cadence_resync_commands() {
    let sampled_refresh = Some(144_000);

    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::Moved,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 144_000,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::Resized,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 144_000,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::ScaleFactorChanged,
            sampled_refresh,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 144_000,
        }
    );
}

#[test]
fn cadence_resync_commands_use_zero_when_monitor_timing_is_unavailable() {
    assert_eq!(
        cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Moved, None),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 0,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(MonitorAffectingWindowEvent::Resized, None),
        UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 0,
        }
    );
    assert_eq!(
        cadence_resync_command_for_monitor_event(
            MonitorAffectingWindowEvent::ScaleFactorChanged,
            None,
        ),
        UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 0,
        }
    );
}

#[test]
fn gpu_auto_fallback_emits_correlated_diagnostics_and_runtime_notice() {
    let diagnostics = DiagnosticsSink::default();
    let (event, notice) = emit_gpu_auto_fallback_observability(
        &diagnostics,
        7,
        3,
        41,
        GpuFailureKind::SwapchainOutOfDate,
        2_500,
    );

    assert_eq!(event.kind, EventKind::RenderModeTransition);
    let correlation = event
        .correlation_id
        .as_ref()
        .expect("fallback diagnostics must include correlation");
    assert!(event.message.contains("transition-seq=7"));
    assert!(event.message.contains("failure-seq=3"));
    assert!(event.message.contains("render-attempt-seq=41"));
    assert!(event.message.contains("observed-ms=2500"));
    assert!(notice.contains("transition-seq=7"));
    assert!(notice.contains("failure-seq=3"));
    assert!(notice.contains("render-attempt-seq=41"));
    assert!(notice.contains("observed-ms=2500"));
    assert!(notice.contains(correlation.as_str()));
}

#[test]
fn gui_write_boundary_policy_stays_recoverable_with_budget() {
    let mut session_policy = SessionController::with_recoverable_budget(2);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let decision = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("recoverable write boundary should classify");

    assert_eq!(
        decision,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 1,
        }
    );
}

#[test]
fn gui_write_boundary_policy_escalates_after_budget_exhaustion() {
    let mut session_policy = SessionController::with_recoverable_budget(1);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let first = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("first write boundary should stay recoverable");
    assert_eq!(
        first,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 0,
        }
    );

    let second = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("second write boundary should escalate after budget exhaustion");
    assert_eq!(
        second,
        PtyBoundaryPolicyDecision::Fatal {
            reason: FatalBoundaryReason::RecoverableBudgetExhausted,
        }
    );
}

#[test]
fn color_to_u32_default_uses_default_color() {
    let default_fg = color_to_u32(Color::Default, DEFAULT_FG);
    assert_eq!(default_fg, DEFAULT_FG_U32);
}

#[test]
fn color_to_u32_indexed_looks_up_palette() {
    let red = color_to_u32(Color::Indexed(1), DEFAULT_FG);
    assert_eq!(red, ANSI_PALETTE[1]);
}

#[test]
fn color_to_u32_rgb_constructs_correctly() {
    let c = color_to_u32(Color::Rgb(0xFF, 0x80, 0x00), DEFAULT_FG);
    assert_eq!(c, 0x00FF_8000);
}

#[test]
fn resolve_cell_colors_inverse_swaps_fg_bg() {
    let attrs = Attrs::default()
        .with_fg(Color::Indexed(1))
        .with_bg(Color::Indexed(2))
        .with_inverse();
    let (fg, bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, ANSI_PALETTE[2]);
    assert_eq!(bg, ANSI_PALETTE[1]);
}

#[test]
fn resolve_cell_colors_dim_halves_fg() {
    let attrs = Attrs::default()
        .with_fg(Color::Rgb(200, 100, 50))
        .with_dim();
    let (fg, _bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, rldyourterm_render_cpu::rgb_to_u32(100, 50, 25));
}
