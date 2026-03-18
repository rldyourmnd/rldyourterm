// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::events::{DisplayClearMode, IngestDegradeReason};

use super::{
    MAX_CSI_LEN, MAX_OSC_LEN, MAX_SGR_PARAMS, Parser, ParserAction, SgrParams, ShellMarkerKind,
};

#[test]
fn dcs_followed_by_normal_text() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1bP+q71756572792d6f732d6e616d65\x1b\\Hello");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Hello"))
    );
}

#[test]
fn incomplete_dcs_discarded_on_resync() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1bPsome_payload");
    assert!(actions.is_empty());
    let resync = parser.resync_after_truncation();
    assert!(resync.is_empty());
    let actions = parser.feed(b"A");
    assert_eq!(actions, vec![ParserAction::Print('A')]);
}

#[test]
fn sgr_bare_m_means_reset() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[m");
    assert_eq!(
        actions,
        vec![ParserAction::SetGraphicsRendition(SgrParams::default())]
    );
}

#[test]
fn osc_st_terminator_no_stray_backslash() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]0;Title\x1b\\");
    assert_eq!(
        actions,
        vec![ParserAction::SetWindowTitle("Title".to_string())]
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, ParserAction::Print('\\')))
    );
}

#[test]
fn osc_st_followed_by_normal_text() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]0;Hello\x1b\\ABC");
    assert_eq!(
        actions,
        vec![
            ParserAction::SetWindowTitle("Hello".to_string()),
            ParserAction::PrintText("ABC".into()),
        ]
    );
}

#[test]
fn parses_reverse_index() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1bM");
    assert_eq!(actions, vec![ParserAction::ReverseIndex]);
}

#[test]
fn parses_next_line() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1bE");
    assert_eq!(actions, vec![ParserAction::NextLine]);
}

#[test]
fn parses_application_keypad_mode() {
    let mut parser = Parser::default();
    let enable = parser.feed(b"\x1b=");
    assert_eq!(enable, vec![ParserAction::ApplicationKeypadMode(true)]);
    let disable = parser.feed(b"\x1b>");
    assert_eq!(disable, vec![ParserAction::ApplicationKeypadMode(false)]);
}

#[test]
fn parses_clear_scrollback() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[3J");
    assert_eq!(
        actions,
        vec![ParserAction::ClearDisplay(DisplayClearMode::Scrollback)]
    );
}

#[test]
fn parses_bracketed_paste_mode() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?2004h\x1b[?2004l");
    assert_eq!(
        actions,
        vec![
            ParserAction::BracketedPasteMode(true),
            ParserAction::BracketedPasteMode(false),
        ]
    );
}

#[test]
fn parses_application_cursor_keys() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?1h\x1b[?1l");
    assert_eq!(
        actions,
        vec![
            ParserAction::ApplicationCursorKeys(true),
            ParserAction::ApplicationCursorKeys(false),
        ]
    );
}

#[test]
fn parses_auto_wrap_mode() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?7h\x1b[?7l");
    assert_eq!(
        actions,
        vec![
            ParserAction::AutoWrapMode(true),
            ParserAction::AutoWrapMode(false),
        ]
    );
}

#[test]
fn parses_primary_da_query() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[c");
    assert_eq!(actions, vec![ParserAction::SendPrimaryDA]);
    let actions = parser.feed(b"\x1b[0c");
    assert_eq!(actions, vec![ParserAction::SendPrimaryDA]);
}

#[test]
fn parses_device_status_report() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[6n");
    assert_eq!(actions, vec![ParserAction::SendDeviceStatusReport]);
}

#[test]
fn parses_device_ok_query() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5n");
    assert_eq!(actions, vec![ParserAction::SendDeviceOk]);
}

// ── Coverage gap tests: boundary conditions ────────────────

#[test]
fn csi_at_exactly_max_len_is_accepted() {
    let mut parser = Parser::default();
    // Build a CSI with exactly MAX_CSI_LEN param bytes + final byte.
    // Use repeated digit chars to fill the buffer as a single large numeric param
    // (stays within MAX_CSI_PARAMS=32 limit).
    let mut payload = vec![0x1B, b'['];
    let param_bytes = MAX_CSI_LEN - 1;
    payload.extend(std::iter::repeat_n(b'1', param_bytes));
    payload.push(b'm');
    let actions = parser.feed(&payload);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
        "CSI at exactly MAX_CSI_LEN should be accepted, got: {actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, ParserAction::IngestDegraded { .. })),
        "CSI at exactly MAX_CSI_LEN should not degrade"
    );
}

#[test]
fn csi_one_byte_over_max_len_degrades() {
    let mut parser = Parser::default();
    let mut payload = vec![0x1B, b'['];
    payload.extend(std::iter::repeat_n(b'1', MAX_CSI_LEN));
    payload.push(b'm');
    let actions = parser.feed(&payload);
    assert!(
        actions.iter().any(|a| matches!(
            a,
            ParserAction::IngestDegraded {
                reason: IngestDegradeReason::CsiSequenceTooLong,
                ..
            }
        )),
        "CSI one byte over MAX_CSI_LEN should degrade"
    );
}

#[test]
fn osc_at_exactly_max_len_is_accepted() {
    let mut parser = Parser::default();
    let prefix = b"0;";
    let fill_len = MAX_OSC_LEN - prefix.len();
    let mut payload = vec![0x1B, b']'];
    payload.extend_from_slice(prefix);
    payload.extend(std::iter::repeat_n(b'T', fill_len));
    payload.push(0x07);
    let actions = parser.feed(&payload);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
        "OSC at exactly MAX_OSC_LEN should be accepted, got: {actions:?}"
    );
}

#[test]
fn osc_one_byte_over_max_len_is_discarded() {
    let mut parser = Parser::default();
    let prefix = b"0;";
    let fill_len = MAX_OSC_LEN - prefix.len() + 1;
    let mut payload = vec![0x1B, b']'];
    payload.extend_from_slice(prefix);
    payload.extend(std::iter::repeat_n(b'T', fill_len));
    payload.push(0x07);
    let actions = parser.feed(&payload);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
        "OSC one byte over MAX_OSC_LEN should be discarded"
    );
}

#[test]
fn sgr_with_exactly_max_params() {
    let mut parser = Parser::default();
    let params_str = vec!["1"; MAX_SGR_PARAMS].join(";");
    let seq = format!("\x1b[{params_str}m");
    let actions = parser.feed(seq.as_bytes());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(sgr) if sgr.as_slice().len() == MAX_SGR_PARAMS)),
        "SGR with exactly MAX_SGR_PARAMS should preserve all params"
    );
}

#[test]
fn sgr_with_params_beyond_max_truncates_silently() {
    let mut parser = Parser::default();
    let params_str = vec!["1"; MAX_SGR_PARAMS + 4].join(";");
    let seq = format!("\x1b[{params_str}m");
    let actions = parser.feed(seq.as_bytes());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
        "SGR with excess params should still parse"
    );
}

#[test]
fn mixed_utf8_and_csi_across_feed_boundaries() {
    let mut parser = Parser::default();
    let actions1 = parser.feed(&[0xE2, 0x82]);
    assert!(actions1.is_empty(), "Incomplete UTF-8 should buffer");
    let actions2 = parser.feed(&[0xAC, 0x1B, b'[']);
    assert!(
        actions2.contains(&ParserAction::Print('€')),
        "€ should be printed after completion"
    );
    let actions3 = parser.feed(b"31mA");
    assert!(
        actions3
            .iter()
            .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
        "CSI should complete across feeds"
    );
    assert!(
        actions3.contains(&ParserAction::Print('A')),
        "'A' should print after CSI"
    );
}

#[test]
fn empty_csi_params_use_defaults() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[H");
    assert_eq!(
        actions,
        vec![ParserAction::CursorPosition { row: 0, col: 0 }]
    );
    let actions = parser.feed(b"\x1b[;H");
    assert_eq!(
        actions,
        vec![ParserAction::CursorPosition { row: 0, col: 0 }]
    );
}

#[test]
fn multi_param_private_csi_dispatches_all_modes() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?1;25h");
    assert!(actions.contains(&ParserAction::ApplicationCursorKeys(true)));
    assert!(actions.contains(&ParserAction::SetCursorVisible(true)));
}

#[test]
fn multi_param_private_csi_reset() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?1;25;2004l");
    assert_eq!(actions.len(), 3);
    assert!(actions.contains(&ParserAction::ApplicationCursorKeys(false)));
    assert!(actions.contains(&ParserAction::SetCursorVisible(false)));
    assert!(actions.contains(&ParserAction::BracketedPasteMode(false)));
}

// ── OSC 7: Current Working Directory ────────────────────────

#[test]
fn osc_7_sets_cwd_from_file_uri() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]7;file://hostname/home/user/project\x07");
    assert_eq!(
        actions,
        vec![ParserAction::SetCurrentWorkingDirectory(
            "/home/user/project".to_string()
        )]
    );
}

#[test]
fn osc_7_sets_cwd_from_raw_path() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]7;/tmp/test\x07");
    assert_eq!(
        actions,
        vec![ParserAction::SetCurrentWorkingDirectory(
            "/tmp/test".to_string()
        )]
    );
}

#[test]
fn osc_7_with_st_terminator() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]7;file://localhost/usr/bin\x1b\\");
    assert_eq!(
        actions,
        vec![ParserAction::SetCurrentWorkingDirectory(
            "/usr/bin".to_string()
        )]
    );
}

#[test]
fn osc_7_empty_path_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]7;\x07");
    assert!(actions.is_empty());
}

// ── OSC 52: Clipboard Set ───────────────────────────────────

#[test]
fn osc_52_clipboard_set() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]52;c;SGVsbG8=\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ClipboardSet {
            selection: 'c',
            base64_data: "SGVsbG8=".to_string(),
        }]
    );
}

#[test]
fn osc_52_clipboard_query_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]52;c;?\x07");
    assert!(actions.is_empty());
}

#[test]
fn osc_52_empty_data_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]52;c;\x07");
    assert!(actions.is_empty());
}

#[test]
fn osc_52_oversized_payload_ignored() {
    let mut parser = Parser::default();
    // 100KB decoded = ~133KB base64 encoded
    let large_b64 = "A".repeat(140_000);
    let mut payload = b"\x1b]52;c;".to_vec();
    payload.extend(large_b64.as_bytes());
    payload.push(0x07);
    let actions = parser.feed(&payload);
    assert!(actions.is_empty());
}

#[test]
fn osc_52_primary_selection() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]52;p;dGVzdA==\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ClipboardSet {
            selection: 'p',
            base64_data: "dGVzdA==".to_string(),
        }]
    );
}

// ── OSC 133: Shell Markers ──────────────────────────────────

#[test]
fn osc_133_prompt_start() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;A\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ShellMarker(ShellMarkerKind::PromptStart)]
    );
}

#[test]
fn osc_133_command_start() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;B\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ShellMarker(ShellMarkerKind::CommandStart)]
    );
}

#[test]
fn osc_133_output_start() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;C\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ShellMarker(ShellMarkerKind::OutputStart)]
    );
}

#[test]
fn osc_133_output_end() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;D\x07");
    assert_eq!(
        actions,
        vec![ParserAction::ShellMarker(ShellMarkerKind::OutputEnd)]
    );
}

#[test]
fn osc_133_unknown_marker_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;Z\x07");
    assert!(actions.is_empty());
}

#[test]
fn osc_133_with_st_terminator() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]133;A\x1b\\");
    assert_eq!(
        actions,
        vec![ParserAction::ShellMarker(ShellMarkerKind::PromptStart)]
    );
}

// --- DECSCUSR (CSI Ps SP q) tests ---

#[test]
fn decscusr_blinking_bar() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5 q");
    assert_eq!(actions, vec![ParserAction::SetCursorShape(5)]);
}

#[test]
fn decscusr_steady_block() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[2 q");
    assert_eq!(actions, vec![ParserAction::SetCursorShape(2)]);
}

#[test]
fn decscusr_reset_default() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[0 q");
    assert_eq!(actions, vec![ParserAction::SetCursorShape(0)]);
}

#[test]
fn decscusr_no_param_defaults_to_zero() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[ q");
    assert_eq!(actions, vec![ParserAction::SetCursorShape(0)]);
}

#[test]
fn decscusr_clamped_to_six() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[99 q");
    assert_eq!(actions, vec![ParserAction::SetCursorShape(6)]);
}

#[test]
fn decscusr_followed_by_text() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5 qHello");
    assert!(actions.contains(&ParserAction::SetCursorShape(5)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Hello"))
    );
}

#[test]
fn unknown_intermediate_byte_is_unsupported() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[1!p");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::UnsupportedSequence(_)))
    );
}

// ── DA2 (CSI > c) ──────────────────────────────────────────

#[test]
fn da2_sends_secondary_device_attributes() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>c");
    assert_eq!(actions, vec![ParserAction::SendSecondaryDA]);
    // Also works with explicit param 0
    let actions = parser.feed(b"\x1b[>0c");
    assert_eq!(actions, vec![ParserAction::SendSecondaryDA]);
}

// ── XTVERSION (CSI > q) ────────────────────────────────────

#[test]
fn xtversion_sends_terminal_version() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>q");
    assert_eq!(actions, vec![ParserAction::SendXtversion]);
}

// ── DECRQM (CSI ? Ps $ p) ──────────────────────────────────

#[test]
fn decrqm_reports_mode_status() {
    let mut parser = Parser::default();
    // Query mode 1 (application cursor keys)
    let actions = parser.feed(b"\x1b[?1$p");
    assert_eq!(actions, vec![ParserAction::RequestModeReport(1)]);
    // Query mode 2004 (bracketed paste)
    let actions = parser.feed(b"\x1b[?2004$p");
    assert_eq!(actions, vec![ParserAction::RequestModeReport(2004)]);
}

// ── OSC 8: Hyperlinks ──────────────────────────────────────

#[test]
fn osc_8_hyperlink_start_and_end() {
    let mut parser = Parser::default();
    // Start hyperlink with URI
    let actions = parser.feed(b"\x1b]8;;https://example.com\x07");
    assert_eq!(
        actions,
        vec![ParserAction::HyperlinkStart {
            uri: "https://example.com".to_string(),
        }]
    );
    // End hyperlink (empty URI)
    let actions = parser.feed(b"\x1b]8;;\x07");
    assert_eq!(actions, vec![ParserAction::HyperlinkEnd]);
}

// ── OSC 10/11: Color queries ───────────────────────────────

#[test]
fn osc_10_queries_foreground_color() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]10;?\x07");
    assert_eq!(actions, vec![ParserAction::QueryForegroundColor]);
}

#[test]
fn osc_11_queries_background_color() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]11;?\x07");
    assert_eq!(actions, vec![ParserAction::QueryBackgroundColor]);
}

#[test]
fn osc_10_set_color_is_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]10;#ffffff\x07");
    assert!(
        actions.is_empty(),
        "OSC 10 set should be ignored, got: {actions:?}"
    );
}

#[test]
fn osc_11_set_color_is_ignored() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]11;rgb:0000/0000/0000\x07");
    assert!(
        actions.is_empty(),
        "OSC 11 set should be ignored, got: {actions:?}"
    );
}

// --- Kitty keyboard protocol ---

#[test]
fn kitty_keyboard_push_mode() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>1u");
    assert_eq!(actions, vec![ParserAction::PushKittyKeyboardMode(1)]);
}

#[test]
fn kitty_keyboard_push_mode_flags_31() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>31u");
    assert_eq!(actions, vec![ParserAction::PushKittyKeyboardMode(31)]);
}

#[test]
fn kitty_keyboard_push_mode_no_param_defaults_to_zero() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>u");
    assert_eq!(actions, vec![ParserAction::PushKittyKeyboardMode(0)]);
}

#[test]
fn kitty_keyboard_pop_mode() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[<u");
    assert_eq!(actions, vec![ParserAction::PopKittyKeyboardMode(1)]);
}

#[test]
fn kitty_keyboard_pop_mode_with_count() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[<3u");
    assert_eq!(actions, vec![ParserAction::PopKittyKeyboardMode(3)]);
}

#[test]
fn kitty_keyboard_pop_mode_count_1_explicit() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[<1u");
    assert_eq!(actions, vec![ParserAction::PopKittyKeyboardMode(1)]);
}

#[test]
fn kitty_keyboard_pop_malformed_params_rejected() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[<?u");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::UnsupportedSequence(_))),
        "malformed pop should be unsupported, got: {actions:?}"
    );
}

#[test]
fn kitty_keyboard_query_mode() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?u");
    assert_eq!(actions, vec![ParserAction::QueryKittyKeyboardMode]);
}

#[test]
fn kitty_keyboard_push_followed_by_text() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[>1uHello");
    assert!(actions.contains(&ParserAction::PushKittyKeyboardMode(1)));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Hello"))
    );
}
