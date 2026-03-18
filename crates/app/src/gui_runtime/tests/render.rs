// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::super::{
    BackendSyncAction, DeferredGpuInitState, RenderBackendCoordinator, RenderMode,
    RenderWaitPolicy, RenderWaitPolicy::EventDriven, deferred_gpu_init_backoff, render_wait_policy,
};
use rldyourterm_services::render_mode::ActiveRenderPath;
use std::time::Duration;

#[test]
fn deferred_gpu_init_backoff_is_bounded_and_monotonic() {
    let first = deferred_gpu_init_backoff(1);
    let second = deferred_gpu_init_backoff(2);
    let third = deferred_gpu_init_backoff(3);
    let fourth = deferred_gpu_init_backoff(4);
    let saturated = deferred_gpu_init_backoff(8);

    assert!(first <= second);
    assert!(second <= third);
    assert!(third <= fourth);
    assert_eq!(fourth, saturated);
}

#[test]
fn deferred_gpu_init_state_transitions_are_consistent() {
    let mut state = DeferredGpuInitState::new(true);
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 1);
    assert_eq!(state.begin_attempt(), 1);
    assert_eq!(state.retry_deadline(), None);

    state.schedule_retry(1, Duration::from_millis(10));
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 2);
    assert!(state.retry_deadline().is_some());

    state.record_failure_attempt(2);
    assert_eq!(state.next_attempt(), 3);

    state.mark_exhausted(2);
    assert!(!state.is_pending());
    assert_eq!(state.retry_deadline(), None);
    assert_eq!(state.next_attempt(), 3);

    state.sync_with_target_path(ActiveRenderPath::Gpu, false);
    assert!(state.is_pending());
    assert_eq!(state.next_attempt(), 3);

    state.mark_ready();
    assert!(!state.is_pending());
    assert_eq!(state.next_attempt(), 1);

    state.sync_with_target_path(ActiveRenderPath::Cpu, false);
    assert!(!state.is_pending());
    assert_eq!(state.next_attempt(), 1);
    assert_eq!(state.retry_deadline(), None);
}

#[test]
fn render_backend_coordinator_tracks_sequences_and_sync_policy() {
    let mut coordinator = RenderBackendCoordinator::new(RenderMode::Auto);

    assert!(coordinator.deferred_gpu_init_pending());
    assert_eq!(coordinator.begin_render_attempt(), 1);
    assert_eq!(coordinator.begin_render_attempt(), 2);
    assert_eq!(coordinator.current_render_attempt_sequence(), 2);
    assert_eq!(coordinator.next_gpu_failure_sequence(), 1);
    assert_eq!(coordinator.next_gpu_failure_sequence(), 2);

    assert_eq!(
        coordinator.sync_with_target_path(ActiveRenderPath::Cpu, true),
        BackendSyncAction::ReleaseGpuBackend
    );
    assert!(!coordinator.deferred_gpu_init_pending());

    coordinator.mark_deferred_ready();
    assert_eq!(
        coordinator.wait_policy(
            ActiveRenderPath::Gpu,
            true,
            true,
            Some(Duration::from_millis(16))
        ),
        EventDriven
    );
}

#[test]
fn render_wait_policy_uses_event_driven_gpu_lane() {
    assert_eq!(
        render_wait_policy(true, true, Some(Duration::from_millis(8))),
        EventDriven
    );
}

#[test]
fn render_wait_policy_uses_cpu_cadence_when_dirty() {
    assert_eq!(
        render_wait_policy(false, true, Some(Duration::from_millis(16))),
        RenderWaitPolicy::CadenceTimed(Duration::from_millis(16))
    );
}

#[test]
fn render_wait_policy_falls_back_to_event_driven_without_cadence() {
    assert_eq!(render_wait_policy(false, true, None), EventDriven);
    assert_eq!(
        render_wait_policy(false, false, Some(Duration::from_millis(16))),
        EventDriven
    );
}
