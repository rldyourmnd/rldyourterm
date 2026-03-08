use anyhow::{Result, anyhow};
use rldyourterm_services::session::{
    FatalBoundaryReason, SessionBoundary, SessionController, SessionState,
};

use crate::shared::{
    PtyBoundaryPolicyDecision, classify_pty_boundary_failure, fatal_boundary_reason_token,
    session_boundary_token,
};

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
    use super::fatal_pty_boundary_failure;
    use rldyourterm_services::session::{SessionBoundary, SessionController};

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
}
