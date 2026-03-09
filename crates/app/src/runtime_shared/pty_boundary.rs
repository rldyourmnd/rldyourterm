use anyhow::{Result, anyhow};
use rldyourterm_foundation::api::pty::PtyIo;
use rldyourterm_services::session::{
    FatalBoundaryReason, SessionBoundary, SessionController, SessionState, SessionTransitionOutcome,
};

use crate::runtime_shared::display::{fatal_boundary_reason_token, session_boundary_token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PtyBoundaryPolicyDecision {
    Continue { attempt: u8, remaining_budget: u8 },
    Fatal { reason: FatalBoundaryReason },
}

pub(crate) fn classify_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
) -> Result<PtyBoundaryPolicyDecision> {
    let transition = session_policy
        .handle_boundary_failure(boundary)
        .map_err(|error| {
            anyhow!(
                "failed to apply PTY boundary policy boundary={}: {error}",
                session_boundary_token(boundary)
            )
        })?;

    match transition.outcome {
        SessionTransitionOutcome::RecoverableBoundary {
            attempt,
            remaining_budget,
            ..
        } => Ok(PtyBoundaryPolicyDecision::Continue {
            attempt,
            remaining_budget,
        }),
        SessionTransitionOutcome::FatalBoundary { reason, .. } => {
            Ok(PtyBoundaryPolicyDecision::Fatal { reason })
        }
        outcome @ (SessionTransitionOutcome::Started { .. }
        | SessionTransitionOutcome::StopRequested
        | SessionTransitionOutcome::Stopped) => Err(anyhow!(
            "unexpected session transition for boundary={} outcome={outcome:?}",
            session_boundary_token(boundary)
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundaryFailureOutcome {
    Continue { attempt: u8, remaining_budget: u8 },
    Fatal { reason: FatalBoundaryReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundaryRecovery {
    pub from: SessionState,
    pub to: SessionState,
    pub notice: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PtyReadFailureResolution {
    ChildExited(i32),
}

pub(crate) fn apply_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
    _detail: &str,
) -> Result<BoundaryFailureOutcome> {
    match classify_pty_boundary_failure(session_policy, boundary)? {
        PtyBoundaryPolicyDecision::Continue {
            attempt,
            remaining_budget,
        } => Ok(BoundaryFailureOutcome::Continue {
            attempt,
            remaining_budget,
        }),
        PtyBoundaryPolicyDecision::Fatal { reason } => Ok(BoundaryFailureOutcome::Fatal { reason }),
    }
}

pub(crate) fn runtime_boundary_notice(
    boundary: SessionBoundary,
    attempt: u8,
    remaining_budget: u8,
    detail: &str,
) -> String {
    format!(
        "[runtime] recoverable pty-boundary={} attempt={} remaining-budget={} detail={detail}",
        session_boundary_token(boundary),
        attempt,
        remaining_budget,
    )
}

pub(crate) fn fatal_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
    detail: &str,
) -> anyhow::Error {
    match apply_pty_boundary_failure(session_policy, boundary, detail) {
        Ok(BoundaryFailureOutcome::Fatal { reason }) => anyhow!(
            "fatal PTY boundary failure boundary={} reason={} detail={detail}",
            session_boundary_token(boundary),
            fatal_boundary_reason_token(reason),
        ),
        Ok(BoundaryFailureOutcome::Continue {
            attempt,
            remaining_budget,
        }) => anyhow!(
            "unexpected recoverable PTY boundary failure boundary={} attempt={} remaining_budget={} detail={detail}",
            session_boundary_token(boundary),
            attempt,
            remaining_budget,
        ),
        Err(error) => anyhow!(
            "failed to classify fatal PTY boundary failure boundary={}: {error}",
            session_boundary_token(boundary),
        ),
    }
}

pub(crate) fn force_fatal_pty_boundary_failure(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
    detail: &str,
) -> anyhow::Error {
    match session_policy.handle_fatal_boundary(boundary) {
        Ok(_) => anyhow!(
            "fatal PTY boundary failure boundary={} reason={} detail={detail}",
            session_boundary_token(boundary),
            fatal_boundary_reason_token(FatalBoundaryReason::BoundaryFatal),
        ),
        Err(error) => anyhow!(
            "failed to force fatal PTY boundary failure boundary={}: {error}",
            session_boundary_token(boundary),
        ),
    }
}

pub(crate) fn resolve_live_pty_read_failure(
    pty: &dyn PtyIo,
    session_policy: &mut SessionController,
    detail: &str,
    poll_context: &'static str,
) -> Result<PtyReadFailureResolution, anyhow::Error> {
    match pty.try_wait() {
        Ok(Some(code)) => Ok(PtyReadFailureResolution::ChildExited(code)),
        Ok(None) => Err(force_fatal_pty_boundary_failure(
            session_policy,
            SessionBoundary::PtyRead,
            detail,
        )),
        Err(error) => {
            let detail = format!("{poll_context}: {error}");
            Err(fatal_pty_boundary_failure(
                session_policy,
                SessionBoundary::PtyWait,
                &detail,
            ))
        }
    }
}

pub(crate) fn mark_pty_boundary_recovered(
    session_policy: &mut SessionController,
    boundary: SessionBoundary,
) -> Result<Option<BoundaryRecovery>> {
    if session_policy.state() != SessionState::Degraded {
        return Ok(None);
    }

    let transition = session_policy.mark_running().map_err(|error| {
        anyhow!(
            "failed to mark PTY boundary recovery boundary={}: {error}",
            session_boundary_token(boundary),
        )
    })?;

    Ok(Some(BoundaryRecovery {
        from: transition.from,
        to: transition.to,
        notice: format!(
            "[runtime] recovered pty-boundary={}",
            session_boundary_token(boundary)
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        PtyReadFailureResolution, fatal_pty_boundary_failure, force_fatal_pty_boundary_failure,
        resolve_live_pty_read_failure,
    };
    use rldyourterm_foundation::api::{
        common::ContractResult,
        pty::{PtyIo, PtySize},
    };
    use rldyourterm_foundation::error::{
        FoundationError, PtyFailureCode, PtyOperation, Recoverability,
    };
    use rldyourterm_services::session::{SessionBoundary, SessionController};
    use std::io::{Read, Write};
    use std::sync::Mutex;

    #[derive(Debug)]
    enum StubTryWaitResult {
        Running,
        Exited(i32),
        Error(&'static str),
    }

    struct StubPtyIo {
        try_wait_result: Mutex<StubTryWaitResult>,
    }

    impl PtyIo for StubPtyIo {
        fn take_reader(&self) -> ContractResult<Box<dyn Read + Send>> {
            unreachable!("reader access is not part of this test")
        }

        fn take_writer(&self) -> ContractResult<Box<dyn Write + Send>> {
            unreachable!("writer access is not part of this test")
        }

        fn resize(&self, _size: PtySize) -> ContractResult<()> {
            unreachable!("resize is not part of this test")
        }

        fn kill(&self) -> ContractResult<()> {
            unreachable!("kill is not part of this test")
        }

        fn wait(&self) -> ContractResult<i32> {
            unreachable!("wait is not part of this test")
        }

        fn try_wait(&self) -> ContractResult<Option<i32>> {
            match &*self.try_wait_result.lock().expect("stub lock") {
                StubTryWaitResult::Running => Ok(None),
                StubTryWaitResult::Exited(code) => Ok(Some(*code)),
                StubTryWaitResult::Error(message) => Err(FoundationError::pty(
                    PtyOperation::TryWait,
                    PtyFailureCode::BoundaryFault,
                    Recoverability::Fatal,
                    *message,
                    None,
                )),
            }
        }

        fn close(&self) -> ContractResult<()> {
            unreachable!("close is not part of this test")
        }
    }

    #[test]
    fn fatal_pty_boundary_failure_uses_explicit_error_path() {
        let mut session_policy = SessionController::with_recoverable_budget(3);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let error = fatal_pty_boundary_failure(
            &mut session_policy,
            SessionBoundary::PtyWait,
            "wait failed",
        );

        assert!(error.to_string().contains("fatal PTY boundary failure"));
        assert!(error.to_string().contains("boundary=pty-wait"));
    }

    #[test]
    fn force_fatal_pty_boundary_failure_overrides_recoverable_classification() {
        let mut session_policy = SessionController::with_recoverable_budget(3);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let error = force_fatal_pty_boundary_failure(
            &mut session_policy,
            SessionBoundary::PtyRead,
            "reader lost",
        );

        assert!(error.to_string().contains("fatal PTY boundary failure"));
        assert!(error.to_string().contains("boundary=pty-read"));
        assert_eq!(
            session_policy.state(),
            rldyourterm_services::session::SessionState::Stopping
        );
    }

    #[test]
    fn resolve_live_pty_read_failure_is_fatal_while_child_is_running() {
        let pty = StubPtyIo {
            try_wait_result: Mutex::new(StubTryWaitResult::Running),
        };
        let mut session_policy = SessionController::with_recoverable_budget(3);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let error = resolve_live_pty_read_failure(
            &pty,
            &mut session_policy,
            "reader pump failed",
            "failed to poll PTY after reader failure",
        )
        .expect_err("live reader loss must be fatal under single-reader PTY contract");

        assert!(error.to_string().contains("boundary=pty-read"));
        assert_eq!(
            session_policy.state(),
            rldyourterm_services::session::SessionState::Stopping
        );
    }

    #[test]
    fn resolve_live_pty_read_failure_preserves_post_exit_handling() {
        let pty = StubPtyIo {
            try_wait_result: Mutex::new(StubTryWaitResult::Exited(17)),
        };
        let mut session_policy = SessionController::with_recoverable_budget(3);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let resolution = resolve_live_pty_read_failure(
            &pty,
            &mut session_policy,
            "reader pump failed",
            "failed to poll PTY after reader failure",
        )
        .expect("post-exit reader failure should not escalate");

        assert_eq!(resolution, PtyReadFailureResolution::ChildExited(17));
        assert_eq!(
            session_policy.state(),
            rldyourterm_services::session::SessionState::Running
        );
    }

    #[test]
    fn resolve_live_pty_read_failure_promotes_try_wait_errors_to_fatal_wait_boundary() {
        let pty = StubPtyIo {
            try_wait_result: Mutex::new(StubTryWaitResult::Error("wait poll broke")),
        };
        let mut session_policy = SessionController::with_recoverable_budget(3);
        session_policy
            .mark_running()
            .expect("session should enter running state");

        let error = resolve_live_pty_read_failure(
            &pty,
            &mut session_policy,
            "reader pump failed",
            "failed to poll PTY after reader failure",
        )
        .expect_err("try_wait failure should escalate as fatal wait boundary");

        assert!(error.to_string().contains("boundary=pty-wait"));
    }
}
