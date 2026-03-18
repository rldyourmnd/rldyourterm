// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary};

pub(crate) fn render_mode_token(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Cpu => "cpu",
        RenderMode::Gpu => "gpu",
        RenderMode::Auto => "auto",
    }
}

pub(crate) fn on_off_token(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

pub(crate) fn session_boundary_token(boundary: SessionBoundary) -> &'static str {
    match boundary {
        SessionBoundary::StartupSpawn => "startup-spawn",
        SessionBoundary::PtyRead => "pty-read",
        SessionBoundary::PtyWrite => "pty-write",
        SessionBoundary::PtyResize => "pty-resize",
        SessionBoundary::PtyWait => "pty-wait",
        SessionBoundary::PtyWriterAcquire => "pty-writer-acquire",
        SessionBoundary::Stop => "stop",
    }
}

pub(crate) fn fatal_boundary_reason_token(reason: FatalBoundaryReason) -> &'static str {
    match reason {
        FatalBoundaryReason::BoundaryFatal => "boundary-fatal",
        FatalBoundaryReason::RecoverableBudgetExhausted => "recoverable-budget-exhausted",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_mode_token_values() {
        assert_eq!(render_mode_token(RenderMode::Cpu), "cpu");
        assert_eq!(render_mode_token(RenderMode::Gpu), "gpu");
        assert_eq!(render_mode_token(RenderMode::Auto), "auto");
    }

    #[test]
    fn on_off_token_values() {
        assert_eq!(on_off_token(true), "on");
        assert_eq!(on_off_token(false), "off");
    }

    #[test]
    fn session_boundary_token_exhaustive() {
        assert_eq!(
            session_boundary_token(SessionBoundary::StartupSpawn),
            "startup-spawn"
        );
        assert_eq!(session_boundary_token(SessionBoundary::PtyRead), "pty-read");
        assert_eq!(
            session_boundary_token(SessionBoundary::PtyWrite),
            "pty-write"
        );
        assert_eq!(
            session_boundary_token(SessionBoundary::PtyResize),
            "pty-resize"
        );
        assert_eq!(session_boundary_token(SessionBoundary::PtyWait), "pty-wait");
        assert_eq!(
            session_boundary_token(SessionBoundary::PtyWriterAcquire),
            "pty-writer-acquire"
        );
        assert_eq!(session_boundary_token(SessionBoundary::Stop), "stop");
    }

    #[test]
    fn fatal_boundary_reason_token_exhaustive() {
        assert_eq!(
            fatal_boundary_reason_token(FatalBoundaryReason::BoundaryFatal),
            "boundary-fatal"
        );
        assert_eq!(
            fatal_boundary_reason_token(FatalBoundaryReason::RecoverableBudgetExhausted),
            "recoverable-budget-exhausted"
        );
    }
}
