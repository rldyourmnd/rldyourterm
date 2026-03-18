// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;
use crate::error::{BoundaryClassification, BoundarySeverity, BoundaryStage, ServiceError};
use rldyourterm_foundation::error::{
    FoundationError, PtyFailureCode, PtyOperation, Recoverability,
};

#[test]
fn start_success_moves_starting_to_running_with_fish() {
    let mut controller = SessionController::new();

    let transition = controller.start_succeeded().expect("start should succeed");

    assert_eq!(transition.from, SessionState::Starting);
    assert_eq!(transition.to, SessionState::Running);
    assert_eq!(
        transition.outcome,
        SessionTransitionOutcome::Started {
            shell: SessionShell::Fish
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    assert_eq!(controller.state(), SessionState::Running);
}

#[test]
fn recoverable_start_boundary_switches_to_zsh_and_degrades() {
    let mut controller = SessionController::with_recoverable_budget(2);

    let transition = controller
        .handle_recoverable_boundary(SessionBoundary::StartupSpawn)
        .expect("recoverable boundary should not fail");

    assert_eq!(transition.from, SessionState::Starting);
    assert_eq!(transition.to, SessionState::Degraded);
    assert_eq!(
        transition.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::StartupSpawn,
            action: RecoverableAction::SwitchShell(SessionShell::Zsh),
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.requested_shell(), SessionShell::Zsh);
    assert_eq!(controller.active_shell(), None);
    assert_eq!(controller.state(), SessionState::Degraded);
}

#[test]
fn recoverable_runtime_boundary_retries_and_preserves_active_shell() {
    let mut controller = SessionController::with_recoverable_budget(3);
    controller.start_succeeded().expect("start should succeed");

    let transition = controller
        .handle_recoverable_boundary(SessionBoundary::PtyRead)
        .expect("recoverable boundary should not fail");

    assert_eq!(transition.from, SessionState::Running);
    assert_eq!(transition.to, SessionState::Degraded);
    assert_eq!(
        transition.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyRead,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 2,
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    assert_eq!(controller.state(), SessionState::Degraded);
}

#[test]
fn pty_boundary_classification_is_state_aware() {
    assert_eq!(
        SessionBoundary::StartupSpawn.classify_for_state(SessionState::Starting),
        BoundaryClassification::new(BoundaryStage::Start, BoundarySeverity::Recoverable)
    );
    assert_eq!(
        SessionBoundary::StartupSpawn.classify_for_state(SessionState::Running),
        BoundaryClassification::new(BoundaryStage::Start, BoundarySeverity::Fatal)
    );
    assert_eq!(
        SessionBoundary::PtyRead.classify_for_state(SessionState::Degraded),
        BoundaryClassification::new(BoundaryStage::Degrade, BoundarySeverity::Recoverable)
    );
    assert_eq!(
        SessionBoundary::PtyWriterAcquire.classify_for_state(SessionState::Running),
        BoundaryClassification::new(BoundaryStage::Run, BoundarySeverity::Recoverable)
    );
}

#[test]
fn writer_boundary_recoverability_matches_foundation_single_writer_contract() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");

    let session_classification = controller.classify_boundary(SessionBoundary::PtyWriterAcquire);
    let foundation_contract = FoundationError::pty(
        PtyOperation::AcquireWriterLease,
        PtyFailureCode::SingleWriterInvariantViolation,
        Recoverability::Degrade,
        "pty writer is already acquired",
        None,
    );

    assert_eq!(
        session_classification.severity,
        BoundarySeverity::Recoverable
    );
    assert!(foundation_contract.recoverability().is_recoverable());
    assert_eq!(
        session_classification.is_recoverable(),
        foundation_contract.recoverability().is_recoverable()
    );
}

#[test]
fn handle_boundary_failure_uses_classification_for_deterministic_outcomes() {
    let mut controller = SessionController::with_recoverable_budget(2);
    controller.start_succeeded().expect("start should succeed");

    let recoverable = controller
        .handle_boundary_failure(SessionBoundary::PtyResize)
        .expect("pty resize should be recoverable while running");
    assert_eq!(
        recoverable.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyResize,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    controller.mark_running().expect("session should resume");

    let writer_boundary = controller
        .handle_boundary_failure(SessionBoundary::PtyWriterAcquire)
        .expect("writer invariant violations should remain recoverable while running");
    assert_eq!(
        writer_boundary.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyWriterAcquire,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    controller
        .mark_running()
        .expect("session should resume after recoverable writer boundary");

    let fatal = controller
        .handle_boundary_failure(SessionBoundary::PtyWait)
        .expect("wait boundary should remain fatal");
    assert_eq!(
        fatal.outcome,
        SessionTransitionOutcome::FatalBoundary {
            boundary: SessionBoundary::PtyWait,
            reason: FatalBoundaryReason::BoundaryFatal,
        }
    );
    assert_eq!(controller.active_shell(), None);
    assert_eq!(controller.state(), SessionState::Stopping);
}

#[test]
fn runtime_recoverable_cycles_preserve_active_shell_continuity() {
    let mut controller = SessionController::with_recoverable_budget(3);
    controller.start_succeeded().expect("start should succeed");

    let first = controller
        .handle_boundary_failure(SessionBoundary::PtyRead)
        .expect("read disturbance should remain recoverable");
    assert_eq!(first.to, SessionState::Degraded);
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    controller
        .mark_running()
        .expect("controller should return to running");
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));

    let second = controller
        .handle_boundary_failure(SessionBoundary::PtyWrite)
        .expect("write disturbance should remain recoverable");
    assert_eq!(second.to, SessionState::Degraded);
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    controller
        .mark_running()
        .expect("controller should return to running");
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    assert_eq!(controller.state(), SessionState::Running);
}

#[test]
fn recoverable_budget_exhaustion_escalates_to_controlled_stop() {
    let mut controller = SessionController::with_recoverable_budget(1);
    controller.start_succeeded().expect("start should succeed");

    let first = controller
        .handle_recoverable_boundary(SessionBoundary::PtyWrite)
        .expect("first recoverable boundary should stay recoverable");
    assert_eq!(first.to, SessionState::Degraded);

    let second = controller
        .handle_recoverable_boundary(SessionBoundary::PtyWrite)
        .expect("exhaustion should resolve into deterministic fatal outcome");

    assert_eq!(second.from, SessionState::Degraded);
    assert_eq!(second.to, SessionState::Stopping);
    assert_eq!(
        second.outcome,
        SessionTransitionOutcome::FatalBoundary {
            boundary: SessionBoundary::PtyWrite,
            reason: FatalBoundaryReason::RecoverableBudgetExhausted,
        }
    );
    assert_eq!(controller.active_shell(), None);
    assert_eq!(controller.state(), SessionState::Stopping);
}

#[test]
fn recoverable_budget_resets_after_recovery_cycles() {
    let mut controller = SessionController::with_recoverable_budget(2);
    controller.start_succeeded().expect("start should succeed");

    let first = controller
        .handle_boundary_failure(SessionBoundary::PtyRead)
        .expect("first recoverable boundary should stay recoverable");
    assert_eq!(
        first.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyRead,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    controller
        .mark_running()
        .expect("session should resume after first recoverable boundary");
    assert_eq!(controller.recoverable_boundaries(), 0);

    let second = controller
        .handle_boundary_failure(SessionBoundary::PtyWrite)
        .expect("second recoverable boundary should stay recoverable");
    assert_eq!(
        second.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyWrite,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    controller
        .mark_running()
        .expect("session should resume after second recoverable boundary");
    assert_eq!(controller.recoverable_boundaries(), 0);

    let third = controller
        .handle_boundary_failure(SessionBoundary::PtyResize)
        .expect("third recoverable boundary should still be within refreshed budget");
    assert_eq!(third.from, SessionState::Running);
    assert_eq!(third.to, SessionState::Degraded);
    assert_eq!(
        third.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyResize,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Fish));
    assert_eq!(controller.state(), SessionState::Degraded);
}

#[test]
fn startup_completion_resets_recoverable_budget_before_runtime_retries() {
    let mut controller = SessionController::with_recoverable_budget(2);

    let startup_boundary = controller
        .handle_recoverable_boundary(SessionBoundary::StartupSpawn)
        .expect("startup failure should be recoverable from fish baseline");
    assert_eq!(
        startup_boundary.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::StartupSpawn,
            action: RecoverableAction::SwitchShell(SessionShell::Zsh),
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.recoverable_boundaries(), 1);

    let started = controller
        .mark_running()
        .expect("degraded startup should resume with zsh fallback");
    assert_eq!(
        started.outcome,
        SessionTransitionOutcome::Started {
            shell: SessionShell::Zsh
        }
    );
    assert_eq!(controller.recoverable_boundaries(), 0);
    assert_eq!(controller.active_shell(), Some(SessionShell::Zsh));

    let runtime_boundary = controller
        .handle_recoverable_boundary(SessionBoundary::PtyRead)
        .expect("runtime recoverable boundary should use fresh retry budget");
    assert_eq!(
        runtime_boundary.outcome,
        SessionTransitionOutcome::RecoverableBoundary {
            boundary: SessionBoundary::PtyRead,
            action: RecoverableAction::RetryCurrentPath,
            attempt: 1,
            remaining_budget: 1,
        }
    );
    assert_eq!(controller.active_shell(), Some(SessionShell::Zsh));
}

#[test]
fn recoverable_entrypoint_escalates_fatal_pty_wait_to_controlled_stop() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");

    let transition = controller
        .handle_recoverable_boundary(SessionBoundary::PtyWait)
        .expect("fatal classification should be handled without panic");

    assert_eq!(transition.from, SessionState::Running);
    assert_eq!(transition.to, SessionState::Stopping);
    assert_eq!(
        transition.outcome,
        SessionTransitionOutcome::FatalBoundary {
            boundary: SessionBoundary::PtyWait,
            reason: FatalBoundaryReason::BoundaryFatal,
        }
    );
    assert_eq!(controller.active_shell(), None);
}

#[test]
fn fatal_boundary_always_moves_to_stopping() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");

    let transition = controller
        .handle_fatal_boundary(SessionBoundary::PtyWait)
        .expect("fatal boundary should be accepted in running state");

    assert_eq!(transition.from, SessionState::Running);
    assert_eq!(transition.to, SessionState::Stopping);
    assert_eq!(
        transition.outcome,
        SessionTransitionOutcome::FatalBoundary {
            boundary: SessionBoundary::PtyWait,
            reason: FatalBoundaryReason::BoundaryFatal,
        }
    );
    assert_eq!(controller.state(), SessionState::Stopping);
}

#[test]
fn stop_flow_is_deterministic() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");

    let stop_request = controller.request_stop().expect("stop should be requested");
    assert_eq!(stop_request.from, SessionState::Running);
    assert_eq!(stop_request.to, SessionState::Stopping);
    assert_eq!(
        stop_request.outcome,
        SessionTransitionOutcome::StopRequested
    );

    let stopped = controller
        .mark_stopped()
        .expect("stop completion should succeed");
    assert_eq!(stopped.from, SessionState::Stopping);
    assert_eq!(stopped.to, SessionState::Stopped);
    assert_eq!(stopped.outcome, SessionTransitionOutcome::Stopped);
    assert_eq!(controller.state(), SessionState::Stopped);
}

#[test]
fn boundary_error_uses_stage_and_severity_from_classifier() {
    let mut controller = SessionController::new();
    let startup_error =
        controller.boundary_error(SessionBoundary::StartupSpawn, "spawn failed during startup");
    assert_eq!(
        startup_error,
        ServiceError::Boundary {
            stage: BoundaryStage::Start,
            severity: BoundarySeverity::Recoverable,
            details: "spawn failed during startup".to_string(),
        }
    );

    controller.start_succeeded().expect("start should succeed");
    let writer_error =
        controller.boundary_error(SessionBoundary::PtyWriterAcquire, "writer already acquired");
    assert_eq!(
        writer_error,
        ServiceError::Boundary {
            stage: BoundaryStage::Run,
            severity: BoundarySeverity::Recoverable,
            details: "writer already acquired".to_string(),
        }
    );
}

#[test]
fn recoverable_boundary_after_stopped_returns_transition_error() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");
    controller
        .request_stop()
        .expect("stop request should succeed");
    controller
        .mark_stopped()
        .expect("stop completion should succeed");

    let error = controller
        .handle_recoverable_boundary(SessionBoundary::PtyRead)
        .expect_err("recoverable boundary in stopped state must error");

    assert_eq!(
        error,
        ServiceError::InvalidSessionTransition {
            event: "handle_recoverable_boundary",
            state: "stopped",
        }
    );
}

#[test]
fn boundary_failure_after_stopped_returns_transition_error() {
    let mut controller = SessionController::new();
    controller.start_succeeded().expect("start should succeed");
    controller
        .request_stop()
        .expect("stop request should succeed");
    controller
        .mark_stopped()
        .expect("stop completion should succeed");

    let error = controller
        .handle_boundary_failure(SessionBoundary::PtyRead)
        .expect_err("boundary failure in stopped state must error");

    assert_eq!(
        error,
        ServiceError::InvalidSessionTransition {
            event: "handle_boundary_failure",
            state: "stopped",
        }
    );
}
