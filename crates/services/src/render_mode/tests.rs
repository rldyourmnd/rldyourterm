// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::*;
use std::time::Duration;

fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

fn default_metadata(
    failure_kind: GpuFailureKind,
    observed_at: Duration,
    failure_streak: u8,
) -> AutoFallbackMetadata {
    AutoFallbackMetadata {
        failure_kind,
        observed_at,
        failure_streak,
        retry_budget: 2,
        failure_window: Duration::from_secs(3),
    }
}

#[test]
fn auto_mode_starts_on_gpu_path() {
    let controller = RenderModeController::new(RenderMode::Auto);
    assert_eq!(controller.mode(), RenderMode::Auto);
    assert_eq!(controller.active_path(), ActiveRenderPath::Gpu);
}

#[test]
fn auto_mode_retries_then_falls_back_to_cpu() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(0)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SurfaceError, at(0), 1),
            retry_budget_remaining: 1,
        }
    );
    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SubmitError, at(1_000)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SubmitError, at(1_000), 2),
            retry_budget_remaining: 0,
        }
    );

    match controller.on_gpu_failure(GpuFailureKind::SwapchainOutOfDate, at(2_000)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(transition.from, ActiveRenderPath::Gpu);
            assert_eq!(transition.to, ActiveRenderPath::Cpu);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: default_metadata(GpuFailureKind::SwapchainOutOfDate, at(2_000), 3,),
                }
            );
        }
        other => panic!("unexpected fallback decision: {other:?}"),
    }
    assert_eq!(controller.active_path(), ActiveRenderPath::Cpu);
}

#[test]
fn auto_mode_failure_window_resets_streak() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(0)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SurfaceError, at(0), 1),
            retry_budget_remaining: 1,
        }
    );
    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(3_100)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SurfaceError, at(3_100), 1),
            retry_budget_remaining: 1,
        }
    );
}

#[test]
fn device_lost_forces_immediate_fallback_in_auto_mode() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    match controller.on_gpu_failure(GpuFailureKind::DeviceLost, at(0)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(transition.from, ActiveRenderPath::Gpu);
            assert_eq!(transition.to, ActiveRenderPath::Cpu);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: default_metadata(GpuFailureKind::DeviceLost, at(0), 1),
                }
            );
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn out_of_memory_forces_immediate_fallback_in_auto_mode() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    match controller.on_gpu_failure(GpuFailureKind::OutOfMemory, at(0)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(transition.from, ActiveRenderPath::Gpu);
            assert_eq!(transition.to, ActiveRenderPath::Cpu);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: default_metadata(GpuFailureKind::OutOfMemory, at(0), 1),
                }
            );
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn successful_gpu_frame_resets_auto_failure_streak() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(0)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SurfaceError, at(0), 1),
            retry_budget_remaining: 1,
        }
    );
    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SubmitError, at(1_000)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SubmitError, at(1_000), 2),
            retry_budget_remaining: 0,
        }
    );

    controller.on_gpu_frame_presented();

    assert_eq!(
        controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(1_500)),
        FallbackDecision::RetryGpu {
            metadata: default_metadata(GpuFailureKind::SurfaceError, at(1_500), 1),
            retry_budget_remaining: 1,
        }
    );
}

#[test]
fn retry_and_fallback_metadata_reflect_custom_policy_and_observed_time() {
    let mut controller = RenderModeController::new(RenderMode::Auto);
    controller.set_fallback_policy(AutoFallbackPolicy {
        retry_budget: 1,
        failure_window: Duration::from_millis(750),
    });

    let first = controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(250));
    assert_eq!(
        first,
        FallbackDecision::RetryGpu {
            metadata: AutoFallbackMetadata {
                failure_kind: GpuFailureKind::SurfaceError,
                observed_at: at(250),
                failure_streak: 1,
                retry_budget: 1,
                failure_window: Duration::from_millis(750),
            },
            retry_budget_remaining: 0,
        }
    );

    match controller.on_gpu_failure(GpuFailureKind::SubmitError, at(500)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: AutoFallbackMetadata {
                        failure_kind: GpuFailureKind::SubmitError,
                        observed_at: at(500),
                        failure_streak: 2,
                        retry_budget: 1,
                        failure_window: Duration::from_millis(750),
                    },
                }
            );
        }
        other => panic!("unexpected fallback decision: {other:?}"),
    }
}

#[test]
fn forced_modes_do_not_auto_fallback() {
    let mut cpu_controller = RenderModeController::new(RenderMode::Cpu);
    let mut gpu_controller = RenderModeController::new(RenderMode::Gpu);

    assert_eq!(
        cpu_controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(0)),
        FallbackDecision::Noop
    );
    assert_eq!(
        gpu_controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(0)),
        FallbackDecision::Noop
    );
    assert_eq!(cpu_controller.active_path(), ActiveRenderPath::Cpu);
    assert_eq!(gpu_controller.active_path(), ActiveRenderPath::Gpu);
}

#[test]
fn explicit_mode_change_with_same_active_path_emits_transition_metadata() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    let transition = controller
        .set_mode_with_transition(RenderMode::Gpu)
        .expect("mode change should be observable even when path is unchanged");

    assert_eq!(transition.sequence, 1);
    assert_eq!(transition.from_mode, RenderMode::Auto);
    assert_eq!(transition.to_mode, RenderMode::Gpu);
    assert_eq!(transition.from, ActiveRenderPath::Gpu);
    assert_eq!(transition.to, ActiveRenderPath::Gpu);
    assert_eq!(transition.reason, RenderTransitionReason::ExplicitModeSet);
}

#[test]
fn setting_same_mode_and_path_is_noop() {
    let mut controller = RenderModeController::new(RenderMode::Auto);
    assert_eq!(controller.set_mode_with_transition(RenderMode::Auto), None);
    assert_eq!(controller.transition_sequence(), 0);
}

#[test]
fn zero_retry_budget_falls_back_on_first_non_immediate_failure() {
    let mut controller = RenderModeController::new(RenderMode::Auto);
    controller.set_fallback_policy(AutoFallbackPolicy {
        retry_budget: 0,
        failure_window: Duration::from_millis(250),
    });

    match controller.on_gpu_failure(GpuFailureKind::SurfaceError, at(10)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.sequence, 1);
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(transition.from, ActiveRenderPath::Gpu);
            assert_eq!(transition.to, ActiveRenderPath::Cpu);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: AutoFallbackMetadata {
                        failure_kind: GpuFailureKind::SurfaceError,
                        observed_at: at(10),
                        failure_streak: 1,
                        retry_budget: 0,
                        failure_window: Duration::from_millis(250),
                    },
                }
            );
        }
        other => panic!("unexpected decision: {other:?}"),
    }
}

#[test]
fn max_retry_budget_still_has_bounded_fallback_path() {
    let mut controller = RenderModeController::new(RenderMode::Auto);
    controller.set_fallback_policy(AutoFallbackPolicy {
        retry_budget: u8::MAX,
        failure_window: Duration::from_millis(500),
    });

    for observed_ms in 1..u16::from(u8::MAX) {
        assert!(matches!(
            controller.on_gpu_failure(
                GpuFailureKind::SurfaceError,
                Duration::from_millis(u64::from(observed_ms))
            ),
            FallbackDecision::RetryGpu { .. }
        ));
    }

    match controller.on_gpu_failure(GpuFailureKind::SurfaceError, Duration::from_millis(255)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.from, ActiveRenderPath::Gpu);
            assert_eq!(transition.to, ActiveRenderPath::Cpu);
            assert!(matches!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: AutoFallbackMetadata {
                        failure_streak: u8::MAX,
                        retry_budget: u8::MAX,
                        ..
                    }
                }
            ));
        }
        other => panic!("unexpected decision at saturated retry budget: {other:?}"),
    }
}

#[test]
fn explicit_mode_set_can_rearm_auto_after_fallback() {
    let mut controller = RenderModeController::new(RenderMode::Auto);
    let _ = controller.on_gpu_failure(GpuFailureKind::DeviceLost, at(0));
    assert_eq!(controller.active_path(), ActiveRenderPath::Cpu);

    let transition = controller
        .set_mode_with_transition(RenderMode::Auto)
        .expect("expected explicit transition");

    assert_eq!(transition.from_mode, RenderMode::Auto);
    assert_eq!(transition.to_mode, RenderMode::Auto);
    assert_eq!(transition.from, ActiveRenderPath::Cpu);
    assert_eq!(transition.to, ActiveRenderPath::Gpu);
    assert_eq!(transition.reason, RenderTransitionReason::ExplicitModeSet);
    assert_eq!(controller.active_path(), ActiveRenderPath::Gpu);
}

#[test]
fn transition_sequence_stays_deterministic_across_explicit_set_and_fallback() {
    let mut controller = RenderModeController::new(RenderMode::Auto);

    let first = controller
        .set_mode_with_transition(RenderMode::Cpu)
        .expect("cpu path transition should be emitted");
    assert_eq!(first.sequence, 1);
    assert_eq!(first.from_mode, RenderMode::Auto);
    assert_eq!(first.to_mode, RenderMode::Cpu);
    assert_eq!(first.reason, RenderTransitionReason::ExplicitModeSet);

    let second = controller
        .set_mode_with_transition(RenderMode::Auto)
        .expect("auto path transition should be emitted");
    assert_eq!(second.sequence, 2);
    assert_eq!(second.from_mode, RenderMode::Cpu);
    assert_eq!(second.to_mode, RenderMode::Auto);
    assert_eq!(second.reason, RenderTransitionReason::ExplicitModeSet);

    match controller.on_gpu_failure(GpuFailureKind::DeviceLost, at(42)) {
        FallbackDecision::SwitchToCpu(transition) => {
            assert_eq!(transition.sequence, 3);
            assert_eq!(transition.from_mode, RenderMode::Auto);
            assert_eq!(transition.to_mode, RenderMode::Auto);
            assert_eq!(
                transition.reason,
                RenderTransitionReason::AutoGpuFallback {
                    metadata: default_metadata(GpuFailureKind::DeviceLost, at(42), 1),
                }
            );
        }
        other => panic!("expected deterministic fallback transition, got: {other:?}"),
    }
    assert_eq!(controller.transition_sequence(), 3);
}
