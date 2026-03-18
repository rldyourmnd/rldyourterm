// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundarySeverity {
    Recoverable,
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryStage {
    Start,
    Run,
    Degrade,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryClassification {
    pub stage: BoundaryStage,
    pub severity: BoundarySeverity,
}

impl BoundaryClassification {
    pub const fn new(stage: BoundaryStage, severity: BoundarySeverity) -> Self {
        Self { stage, severity }
    }

    pub const fn is_recoverable(self) -> bool {
        matches!(self.severity, BoundarySeverity::Recoverable)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ServiceError {
    #[error("invalid session transition: event={event}, state={state}")]
    InvalidSessionTransition {
        event: &'static str,
        state: &'static str,
    },
    #[error("service boundary failure: stage={stage:?}, severity={severity:?}, details={details}")]
    Boundary {
        stage: BoundaryStage,
        severity: BoundarySeverity,
        details: String,
    },
    #[error("render error: {0}")]
    Render(String),
}

impl ServiceError {
    pub fn invalid_session_transition(event: &'static str, state: &'static str) -> Self {
        Self::InvalidSessionTransition { event, state }
    }

    pub fn boundary(
        stage: BoundaryStage,
        severity: BoundarySeverity,
        details: impl Into<String>,
    ) -> Self {
        Self::Boundary {
            stage,
            severity,
            details: details.into(),
        }
    }
}
