use std::io::{self, ErrorKind, Write};

use anyhow::{Result, anyhow};
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{
    FatalBoundaryReason, SessionBoundary, SessionController, SessionTransitionOutcome,
};

// ---------------------------------------------------------------------------
// PTY boundary policy
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Display / token helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Key encoding (xterm escape sequences)
// ---------------------------------------------------------------------------

pub(crate) fn xterm_modifier_param(shift: bool, alt: bool, ctrl: bool) -> u8 {
    1 + u8::from(shift) + (u8::from(alt) << 1) + (u8::from(ctrl) << 2)
}

pub(crate) fn csi_modified(final_byte: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, final_byte as char).into_bytes()
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

pub(crate) fn tilde_modified(n: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[{n};{mod_param}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

pub(crate) fn fkey_ss3_modified(letter: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, letter as char).into_bytes()
    } else {
        vec![0x1b, b'O', letter]
    }
}

pub(crate) fn encode_ctrl_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) - b'a' + 1)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// IO utilities
// ---------------------------------------------------------------------------

pub(crate) fn write_all_and_flush(writer: &mut dyn Write, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    writer.flush()
}

pub(crate) fn is_disconnect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionAborted
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::UnexpectedEof
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_modifier_param_combinations() {
        assert_eq!(xterm_modifier_param(false, false, false), 1);
        assert_eq!(xterm_modifier_param(true, false, false), 2);
        assert_eq!(xterm_modifier_param(false, true, false), 3);
        assert_eq!(xterm_modifier_param(true, true, false), 4);
        assert_eq!(xterm_modifier_param(false, false, true), 5);
        assert_eq!(xterm_modifier_param(true, false, true), 6);
        assert_eq!(xterm_modifier_param(false, true, true), 7);
        assert_eq!(xterm_modifier_param(true, true, true), 8);
    }

    #[test]
    fn csi_modified_plain_and_with_modifier() {
        assert_eq!(csi_modified(b'A', 1, false), b"\x1b[A");
        assert_eq!(csi_modified(b'A', 5, true), b"\x1b[1;5A");
    }

    #[test]
    fn tilde_modified_plain_and_with_modifier() {
        assert_eq!(tilde_modified(3, 1, false), b"\x1b[3~");
        assert_eq!(tilde_modified(3, 5, true), b"\x1b[3;5~");
    }

    #[test]
    fn fkey_ss3_plain_and_with_modifier() {
        assert_eq!(fkey_ss3_modified(b'P', 1, false), b"\x1bOP");
        assert_eq!(fkey_ss3_modified(b'P', 5, true), b"\x1b[1;5P");
    }

    #[test]
    fn encode_ctrl_letter_mappings() {
        assert_eq!(encode_ctrl_letter('a'), Some(1));
        assert_eq!(encode_ctrl_letter('c'), Some(3));
        assert_eq!(encode_ctrl_letter('z'), Some(26));
        assert_eq!(encode_ctrl_letter('A'), Some(1));
        assert_eq!(encode_ctrl_letter('1'), None);
    }

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
