
use super::{
    JoinThreadOutcome, READ_PUMP_FLUSH_INTERVAL, READ_PUMP_FLUSH_MAX_BYTES, derive_poll_timeouts,
    dispatch_runtime_palette_command, ensure_single_window, fatal_pty_boundary_failure,
    frame_budget_millis, is_stdout_disconnect_error, join_thread_with_timeout,
    should_flush_read_pump, tty_stdio_requirement_message,
};
use crate::runtime_shared::input::{
    encode_crossterm_key_event as encode_key_event,
    is_runtime_palette_shortcut_crossterm as is_runtime_palette_shortcut,
};
use crate::shared::{PtyBoundaryPolicyDecision, classify_pty_boundary_failure};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{FatalBoundaryReason, SessionBoundary, SessionController};
use rldyourterm_settings::SettingsService;
use std::{io, thread, time::Duration};

#[test]
fn encodes_basic_control_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(vec![b'\r'])
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        Some(vec![0x7f])
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Some(vec![b'\t'])
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(vec![0x1b])
    );
}

#[test]
fn encodes_navigation_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        Some(b"\x1b[B".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(b"\x1b[C".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
        Some(b"\x1b[D".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
        Some(b"\x1b[H".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        Some(b"\x1b[F".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
        Some(b"\x1b[3~".to_vec())
    );
}

#[test]
fn key_event_uses_ctrl_letter_encoding() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Some(vec![0x03])
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(
            KeyCode::Char('Q'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )),
        Some(vec![0x11])
    );
}

#[test]
fn enforces_single_window_invariant() {
    assert!(ensure_single_window(1).is_ok());

    let error = ensure_single_window(2).expect_err("window_count=2 must fail");
    assert!(error.to_string().contains("single-window mode"));
}

#[test]
fn derives_adaptive_poll_timeouts_from_runtime_hints() {
    let (gpu_min, gpu_max) = derive_poll_timeouts(RenderMode::Gpu, 144_000);
    let (cpu_min, cpu_max) = derive_poll_timeouts(RenderMode::Cpu, 60_000);

    assert!(gpu_min <= cpu_min);
    assert!(gpu_max < cpu_max);
    assert!(gpu_min <= gpu_max);
    assert!(cpu_min <= cpu_max);
}

#[test]
fn falls_back_to_default_refresh_for_zero_hint() {
    assert_eq!(frame_budget_millis(0), frame_budget_millis(60_000));
}

#[test]
fn detects_palette_shortcut_with_ctrl_or_super_shift_p() {
    assert!(is_runtime_palette_shortcut(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT
    )));
    assert!(is_runtime_palette_shortcut(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::SUPER | KeyModifiers::SHIFT
    )));
    assert!(!is_runtime_palette_shortcut(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL
    )));
}

#[test]
fn palette_dispatch_updates_mode_state() {
    let mut settings = SettingsService::default();

    let result = dispatch_runtime_palette_command(&mut settings, "mode gpu");

    assert_eq!(result.updated_mode, Some(RenderMode::Gpu));
    assert_eq!(settings.state().mode, RenderMode::Gpu);
    assert!(result.message.contains("mode=gpu"));
}

#[test]
fn palette_dispatch_toggles_diagnostics_state() {
    let mut settings = SettingsService::default();

    let on_result = dispatch_runtime_palette_command(&mut settings, "debug on");
    assert!(settings.state().debug_mode);
    assert!(on_result.message.contains("diagnostics=on"));

    let off_result = dispatch_runtime_palette_command(&mut settings, "debug off");
    assert!(!settings.state().debug_mode);
    assert!(off_result.message.contains("diagnostics=off"));
}

#[test]
fn pty_write_boundary_policy_stays_recoverable_with_remaining_budget() {
    let mut session_policy = SessionController::with_recoverable_budget(2);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let decision = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("recoverable write boundary should classify");

    assert_eq!(
        decision,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 1,
        }
    );
}

#[test]
fn pty_write_boundary_policy_escalates_after_budget_exhaustion() {
    let mut session_policy = SessionController::with_recoverable_budget(1);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let first = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("first write boundary should stay recoverable");
    assert_eq!(
        first,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 0,
        }
    );

    let second = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWrite)
        .expect("second write boundary should escalate after budget exhaustion");
    assert_eq!(
        second,
        PtyBoundaryPolicyDecision::Fatal {
            reason: FatalBoundaryReason::RecoverableBudgetExhausted,
        }
    );
}

#[test]
fn pty_read_boundary_policy_stays_recoverable_with_remaining_budget() {
    let mut session_policy = SessionController::with_recoverable_budget(2);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let decision = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyRead)
        .expect("recoverable read boundary should classify");

    assert_eq!(
        decision,
        PtyBoundaryPolicyDecision::Continue {
            attempt: 1,
            remaining_budget: 1,
        }
    );
}

#[test]
fn pty_wait_boundary_policy_is_always_fatal() {
    let mut session_policy = SessionController::with_recoverable_budget(3);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let decision = classify_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWait)
        .expect("wait boundary should classify");

    assert_eq!(
        decision,
        PtyBoundaryPolicyDecision::Fatal {
            reason: FatalBoundaryReason::BoundaryFatal,
        }
    );
}

#[test]
fn fatal_pty_boundary_failure_uses_explicit_error_path() {
    let mut session_policy = SessionController::with_recoverable_budget(3);
    session_policy
        .mark_running()
        .expect("session should enter running state");

    let error =
        fatal_pty_boundary_failure(&mut session_policy, SessionBoundary::PtyWait, "wait failed");

    assert!(error.to_string().contains("fatal PTY boundary failure"));
    assert!(error.to_string().contains("boundary=pty-wait"));
}

#[test]
fn encodes_ctrl_arrow_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL)),
        Some(b"\x1b[1;5D".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL)),
        Some(b"\x1b[1;5C".to_vec())
    );
}

#[test]
fn encodes_shift_arrow_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)),
        Some(b"\x1b[1;2A".to_vec())
    );
}

#[test]
fn encodes_alt_arrow_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)),
        Some(b"\x1b[1;3D".to_vec())
    );
}

#[test]
fn encodes_f_keys_and_nav_keys() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
        Some(b"\x1bOP".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        Some(b"\x1b[15~".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Insert, KeyModifiers::NONE)),
        Some(b"\x1b[2~".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
        Some(b"\x1b[5~".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        Some(b"\x1b[6~".to_vec())
    );
}

#[test]
fn encodes_backtab_and_alt_backspace() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(b"\x1b[Z".to_vec())
    );
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
        Some(b"\x1b\x7f".to_vec())
    );
}

#[test]
fn encodes_alt_char_with_esc_prefix() {
    assert_eq!(
        encode_key_event(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT)),
        Some(b"\x1bf".to_vec())
    );
}

#[test]
fn join_thread_with_timeout_returns_joined_for_finished_thread() {
    let handle = thread::spawn(|| {});
    let outcome = join_thread_with_timeout(
        handle,
        Duration::from_millis(100),
        Duration::from_millis(1),
        "test_joined",
    );
    assert_eq!(outcome, JoinThreadOutcome::Joined);
}

#[test]
fn join_thread_with_timeout_returns_timed_out_for_busy_thread() {
    let handle = thread::spawn(|| thread::sleep(Duration::from_millis(50)));
    let outcome = join_thread_with_timeout(
        handle,
        Duration::from_millis(1),
        Duration::from_millis(1),
        "test_timeout",
    );
    assert_eq!(outcome, JoinThreadOutcome::TimedOut);
    thread::sleep(Duration::from_millis(60));
}

#[test]
fn join_thread_with_timeout_detects_panicking_thread() {
    let handle = thread::spawn(|| panic!("panic for test"));
    let outcome = join_thread_with_timeout(
        handle,
        Duration::from_millis(100),
        Duration::from_millis(1),
        "test_panic",
    );
    assert_eq!(outcome, JoinThreadOutcome::Panicked);
}

#[test]
fn read_pump_flushes_on_line_terminator() {
    assert!(should_flush_read_pump(
        b"prompt line\n",
        1,
        Duration::from_millis(0)
    ));
    assert!(should_flush_read_pump(
        b"\rprogress update",
        1,
        Duration::from_millis(0)
    ));
}

#[test]
fn read_pump_flushes_on_buffer_or_latency_budget() {
    assert!(should_flush_read_pump(
        b"chunk",
        READ_PUMP_FLUSH_MAX_BYTES,
        Duration::from_millis(0)
    ));
    assert!(should_flush_read_pump(
        b"chunk",
        1,
        READ_PUMP_FLUSH_INTERVAL
    ));
}

#[test]
fn read_pump_keeps_short_chunks_buffered_between_budget_limits() {
    assert!(!should_flush_read_pump(
        b"chunk",
        READ_PUMP_FLUSH_MAX_BYTES - 1,
        READ_PUMP_FLUSH_INTERVAL
            .checked_sub(Duration::from_millis(1))
            .expect("flush interval is non-zero"),
    ));
}

#[test]
fn stdout_disconnect_classifier_recognizes_pipe_close_errors() {
    assert!(is_stdout_disconnect_error(&io::Error::new(
        io::ErrorKind::BrokenPipe,
        "broken pipe"
    )));
    assert!(is_stdout_disconnect_error(&io::Error::new(
        io::ErrorKind::NotConnected,
        "not connected"
    )));
    assert!(!is_stdout_disconnect_error(&io::Error::new(
        io::ErrorKind::PermissionDenied,
        "permission denied"
    )));
}

#[test]
fn tty_stdio_requirement_message_reports_both_stream_flags() {
    let detail = tty_stdio_requirement_message(false, true);
    assert!(detail.contains("stdin_is_terminal=false"));
    assert!(detail.contains("stdout_is_terminal=true"));
}
