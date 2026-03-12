// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::error::{BoundaryClassification, BoundarySeverity, BoundaryStage, ServiceError};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RECOVERABLE_BOUNDARY_BUDGET: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionState {
    Starting,
    Running,
    Degraded,
    Stopping,
    Stopped,
}

impl SessionState {
    pub const fn as_str(self) -> &'static str {
        match self {
            SessionState::Starting => "starting",
            SessionState::Running => "running",
            SessionState::Degraded => "degraded",
            SessionState::Stopping => "stopping",
            SessionState::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionShell {
    Fish,
    Zsh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionBoundary {
    StartupSpawn,
    PtyRead,
    PtyWrite,
    PtyResize,
    PtyWait,
    PtyWriterAcquire,
    Stop,
}

impl SessionBoundary {
    pub const fn classify_for_state(self, state: SessionState) -> BoundaryClassification {
        BoundaryClassification::new(self.stage_for_state(state), self.severity_for_state(state))
    }

    pub const fn severity_for_state(self, state: SessionState) -> BoundarySeverity {
        match self {
            SessionBoundary::StartupSpawn => {
                if matches!(state, SessionState::Starting) {
                    BoundarySeverity::Recoverable
                } else {
                    BoundarySeverity::Fatal
                }
            }
            SessionBoundary::PtyRead | SessionBoundary::PtyWrite | SessionBoundary::PtyResize => {
                match state {
                    SessionState::Starting | SessionState::Running | SessionState::Degraded => {
                        BoundarySeverity::Recoverable
                    }
                    SessionState::Stopping | SessionState::Stopped => BoundarySeverity::Fatal,
                }
            }
            SessionBoundary::PtyWriterAcquire => match state {
                SessionState::Starting | SessionState::Running | SessionState::Degraded => {
                    BoundarySeverity::Recoverable
                }
                SessionState::Stopping | SessionState::Stopped => BoundarySeverity::Fatal,
            },
            SessionBoundary::PtyWait | SessionBoundary::Stop => BoundarySeverity::Fatal,
        }
    }

    pub const fn stage_for_state(self, state: SessionState) -> BoundaryStage {
        match self {
            SessionBoundary::StartupSpawn => BoundaryStage::Start,
            SessionBoundary::PtyRead
            | SessionBoundary::PtyWrite
            | SessionBoundary::PtyResize
            | SessionBoundary::PtyWriterAcquire => match state {
                SessionState::Starting | SessionState::Running => BoundaryStage::Run,
                SessionState::Degraded => BoundaryStage::Degrade,
                SessionState::Stopping | SessionState::Stopped => BoundaryStage::Stop,
            },
            SessionBoundary::PtyWait | SessionBoundary::Stop => BoundaryStage::Stop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum RecoverableAction {
    RetryCurrentPath,
    SwitchShell(SessionShell),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FatalBoundaryReason {
    BoundaryFatal,
    RecoverableBudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum SessionTransitionOutcome {
    Started {
        shell: SessionShell,
    },
    RecoverableBoundary {
        boundary: SessionBoundary,
        action: RecoverableAction,
        attempt: u8,
        remaining_budget: u8,
    },
    FatalBoundary {
        boundary: SessionBoundary,
        reason: FatalBoundaryReason,
    },
    StopRequested,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTransition {
    pub from: SessionState,
    pub to: SessionState,
    pub outcome: SessionTransitionOutcome,
    pub sequence: u64,
}

#[derive(Debug)]
pub struct SessionController {
    state: SessionState,
    requested_shell: SessionShell,
    active_shell: Option<SessionShell>,
    recoverable_boundaries: u8,
    max_recoverable_boundaries: u8,
    transition_seq: u64,
}

impl SessionController {
    pub fn new() -> Self {
        Self::with_recoverable_budget(DEFAULT_RECOVERABLE_BOUNDARY_BUDGET)
    }

    pub fn with_recoverable_budget(max_recoverable_boundaries: u8) -> Self {
        Self {
            state: SessionState::Starting,
            requested_shell: SessionShell::Fish,
            active_shell: None,
            recoverable_boundaries: 0,
            max_recoverable_boundaries,
            transition_seq: 0,
        }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    #[cfg(test)]
    fn requested_shell(&self) -> SessionShell {
        self.requested_shell
    }

    #[cfg(test)]
    fn active_shell(&self) -> Option<SessionShell> {
        self.active_shell
    }

    #[cfg(test)]
    fn recoverable_boundaries(&self) -> u8 {
        self.recoverable_boundaries
    }

    fn classify_boundary(&self, boundary: SessionBoundary) -> BoundaryClassification {
        boundary.classify_for_state(self.state)
    }

    #[cfg(test)]
    fn boundary_error(
        &self,
        boundary: SessionBoundary,
        details: impl Into<String>,
    ) -> ServiceError {
        let classification = self.classify_boundary(boundary);
        ServiceError::boundary(classification.stage, classification.severity, details)
    }

    pub fn handle_boundary_failure(
        &mut self,
        boundary: SessionBoundary,
    ) -> Result<SessionTransition, ServiceError> {
        if matches!(self.state, SessionState::Stopped) {
            return Err(self.invalid_transition("handle_boundary_failure"));
        }

        let classification = self.classify_boundary(boundary);
        if classification.is_recoverable() {
            return self.apply_recoverable_boundary(boundary);
        }
        self.handle_fatal_boundary(boundary)
    }

    pub fn mark_running(&mut self) -> Result<SessionTransition, ServiceError> {
        self.start_succeeded()
    }

    fn start_succeeded(&mut self) -> Result<SessionTransition, ServiceError> {
        match self.state {
            SessionState::Starting | SessionState::Degraded => {
                let shell = self.active_shell.unwrap_or(self.requested_shell);
                self.active_shell = Some(shell);
                self.recoverable_boundaries = 0;
                Ok(self.transition(
                    SessionState::Running,
                    SessionTransitionOutcome::Started { shell },
                ))
            }
            _ => Err(self.invalid_transition("start_succeeded")),
        }
    }

    pub fn handle_recoverable_boundary(
        &mut self,
        boundary: SessionBoundary,
    ) -> Result<SessionTransition, ServiceError> {
        if matches!(self.state, SessionState::Stopped) {
            return Err(self.invalid_transition("handle_recoverable_boundary"));
        }

        let classification = self.classify_boundary(boundary);
        if !classification.is_recoverable() {
            return self.handle_fatal_boundary(boundary);
        }

        self.apply_recoverable_boundary(boundary)
    }

    fn apply_recoverable_boundary(
        &mut self,
        boundary: SessionBoundary,
    ) -> Result<SessionTransition, ServiceError> {
        match self.state {
            SessionState::Starting | SessionState::Running | SessionState::Degraded => {
                if self.recoverable_boundaries >= self.max_recoverable_boundaries {
                    self.active_shell = None;
                    return Ok(self.transition(
                        SessionState::Stopping,
                        SessionTransitionOutcome::FatalBoundary {
                            boundary,
                            reason: FatalBoundaryReason::RecoverableBudgetExhausted,
                        },
                    ));
                }

                self.recoverable_boundaries = self.recoverable_boundaries.saturating_add(1);

                let action = if matches!(self.state, SessionState::Starting)
                    && self.requested_shell == SessionShell::Fish
                {
                    self.requested_shell = SessionShell::Zsh;
                    RecoverableAction::SwitchShell(SessionShell::Zsh)
                } else {
                    RecoverableAction::RetryCurrentPath
                };

                let remaining_budget = self
                    .max_recoverable_boundaries
                    .saturating_sub(self.recoverable_boundaries);

                Ok(self.transition(
                    SessionState::Degraded,
                    SessionTransitionOutcome::RecoverableBoundary {
                        boundary,
                        action,
                        attempt: self.recoverable_boundaries,
                        remaining_budget,
                    },
                ))
            }
            _ => Err(self.invalid_transition("handle_recoverable_boundary")),
        }
    }

    pub fn handle_fatal_boundary(
        &mut self,
        boundary: SessionBoundary,
    ) -> Result<SessionTransition, ServiceError> {
        match self.state {
            SessionState::Starting
            | SessionState::Running
            | SessionState::Degraded
            | SessionState::Stopping => {
                self.active_shell = None;
                Ok(self.transition(
                    SessionState::Stopping,
                    SessionTransitionOutcome::FatalBoundary {
                        boundary,
                        reason: FatalBoundaryReason::BoundaryFatal,
                    },
                ))
            }
            SessionState::Stopped => Err(self.invalid_transition("handle_fatal_boundary")),
        }
    }

    pub fn request_stop(&mut self) -> Result<SessionTransition, ServiceError> {
        match self.state {
            SessionState::Starting
            | SessionState::Running
            | SessionState::Degraded
            | SessionState::Stopping => {
                self.active_shell = None;
                Ok(self.transition(
                    SessionState::Stopping,
                    SessionTransitionOutcome::StopRequested,
                ))
            }
            SessionState::Stopped => Err(self.invalid_transition("request_stop")),
        }
    }

    pub fn mark_stopped(&mut self) -> Result<SessionTransition, ServiceError> {
        match self.state {
            SessionState::Stopping | SessionState::Stopped => {
                self.active_shell = None;
                Ok(self.transition(SessionState::Stopped, SessionTransitionOutcome::Stopped))
            }
            _ => Err(self.invalid_transition("mark_stopped")),
        }
    }

    fn transition(
        &mut self,
        to: SessionState,
        outcome: SessionTransitionOutcome,
    ) -> SessionTransition {
        let from = self.state;
        self.state = to;
        self.transition_seq += 1;
        SessionTransition {
            from,
            to,
            outcome,
            sequence: self.transition_seq,
        }
    }

    fn invalid_transition(&self, event: &'static str) -> ServiceError {
        ServiceError::invalid_session_transition(event, self.state.as_str())
    }
}

impl Default for SessionController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
