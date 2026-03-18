// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;
use rldyourterm_services::render_mode::{GpuFailureKind, RenderModeTransition};
use rldyourterm_services::session::SessionBoundary;

fn test_config() -> UiBootstrapConfig {
    UiBootstrapConfig::single_window(RenderMode::Auto, 60_000)
}

#[test]
fn bootstrap_hooks_apply_startup_commands() {
    let hooks = UiBootstrapHooks::from_commands([
        UiRuntimeCommand::AssertSingleWindow {
            requested: SINGLE_WINDOW_BASELINE,
        },
        UiRuntimeCommand::Tick,
    ]);

    let (runtime, receipts) =
        UiRuntime::bootstrap_with_hooks(test_config(), &hooks).expect("bootstrap with hooks");

    assert_eq!(runtime.state(), SessionState::Running);
    assert_eq!(receipts.len(), 2);
    assert!(matches!(
        receipts[0].outcome,
        UiCommandOutcome::SingleWindowConfirmed { window_count: 1 }
    ));
    assert!(matches!(
        receipts[1].outcome,
        UiCommandOutcome::SessionTransition(_)
    ));
}

#[test]
fn assert_single_window_command_rejects_multi_window() {
    let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

    let err = runtime
        .handle_command(UiRuntimeCommand::AssertSingleWindow { requested: 2 })
        .expect_err("expected single-window check failure");

    assert_eq!(
        err,
        UiRuntimeError::Bootstrap(UiBootstrapError::UnsupportedWindowCount { requested: 2 })
    );
}

#[test]
fn cadence_resync_command_updates_refresh_rate() {
    let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

    let receipt = runtime
        .handle_command(UiRuntimeCommand::ResyncCadence {
            refresh_rate_millihz: 144_000,
        })
        .expect("resync command");

    assert_eq!(runtime.cadence().refresh_rate_millihz, 144_000);
    assert!(matches!(
        receipt.outcome,
        UiCommandOutcome::CadenceResynced {
            previous_refresh_rate_millihz: Some(60_000),
            current_refresh_rate_millihz: Some(144_000),
            schedule_invalidated: true,
            monitor_transfer: false,
            ..
        }
    ));
}

#[test]
fn cadence_transfer_resync_invalidates_even_on_same_refresh() {
    let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");
    let receipt = runtime
        .handle_command(UiRuntimeCommand::ResyncCadenceAfterTransfer {
            refresh_rate_millihz: 60_000,
        })
        .expect("transfer resync");

    assert!(matches!(
        receipt.outcome,
        UiCommandOutcome::CadenceResynced {
            previous_refresh_rate_millihz: Some(60_000),
            current_refresh_rate_millihz: Some(60_000),
            schedule_invalidated: true,
            monitor_transfer: true,
            ..
        }
    ));
}

#[test]
fn gpu_failure_commands_drive_auto_fallback_path() {
    let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);

    let first = runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: GpuFailureKind::SurfaceError,
            observed_at_millis: 1_000,
        })
        .expect("first gpu failure");
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
    assert!(matches!(
        first.outcome,
        UiCommandOutcome::GpuRetryScheduled {
            failure_kind: GpuFailureKind::SurfaceError,
            failure_streak: 1,
            retry_budget_remaining: 1
        }
    ));

    let second = runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: GpuFailureKind::SubmitError,
            observed_at_millis: 1_500,
        })
        .expect("second gpu failure");
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
    assert!(matches!(
        second.outcome,
        UiCommandOutcome::GpuRetryScheduled {
            failure_kind: GpuFailureKind::SubmitError,
            failure_streak: 2,
            retry_budget_remaining: 0
        }
    ));

    let third = runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: GpuFailureKind::SwapchainOutOfDate,
            observed_at_millis: 2_000,
        })
        .expect("third gpu failure");
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Cpu);
    assert!(matches!(
        third.outcome,
        UiCommandOutcome::RenderModeTransition(RenderModeTransition {
            from: rldyourterm_services::render_mode::ActiveRenderPath::Gpu,
            to: rldyourterm_services::render_mode::ActiveRenderPath::Cpu,
            reason: rldyourterm_services::render_mode::RenderTransitionReason::AutoGpuFallback { .. },
            ..
        })
    ));
}

#[test]
fn forced_gpu_mode_does_not_auto_fallback_on_gpu_failure_command() {
    let config = UiBootstrapConfig::single_window(RenderMode::Gpu, 60_000);
    let mut runtime = UiRuntime::bootstrap(config).expect("bootstrap");
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);

    let receipt = runtime
        .handle_command(UiRuntimeCommand::GpuFailure {
            kind: GpuFailureKind::SurfaceError,
            observed_at_millis: 250,
        })
        .expect("forced gpu failure command");

    assert!(matches!(receipt.outcome, UiCommandOutcome::Noop));
    assert_eq!(runtime.active_render_path(), ActiveRenderPath::Gpu);
}

#[test]
fn recoverable_boundary_then_tick_returns_running() {
    let mut runtime = UiRuntime::bootstrap(test_config()).expect("bootstrap");

    let degraded = runtime
        .handle_command(UiRuntimeCommand::RecoverableBoundary(
            SessionBoundary::PtyRead,
        ))
        .expect("recoverable boundary");
    assert_eq!(runtime.state(), SessionState::Degraded);
    assert!(matches!(
        degraded.outcome,
        UiCommandOutcome::SessionTransition(_)
    ));

    let resumed = runtime
        .handle_command(UiRuntimeCommand::Tick)
        .expect("tick after degrade");
    assert_eq!(runtime.state(), SessionState::Running);
    assert!(matches!(
        resumed.outcome,
        UiCommandOutcome::SessionTransition(_)
    ));
}
