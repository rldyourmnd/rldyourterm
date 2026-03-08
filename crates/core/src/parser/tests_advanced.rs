use crate::events::{DisplayClearMode, IngestDegradeReason};

use super::{MAX_CSI_LEN, MAX_OSC_LEN, MAX_SGR_PARAMS, Parser, ParserAction, SgrParams};

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
    let mut payload = vec![0x1B, b'['];
    let param_bytes = MAX_CSI_LEN - 1;
    for i in 0..param_bytes {
        if i % 2 == 0 {
            payload.push(b'1');
        } else {
            payload.push(b';');
        }
    }
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
