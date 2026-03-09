// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};

use super::{MAX_CSI_LEN, MAX_OSC_LEN, Parser, ParserAction, SgrParams};

#[test]
fn parses_printable_and_basic_controls() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"ab\r\n\x07\x08");

    assert_eq!(
        actions,
        vec![
            ParserAction::PrintText("ab".into()),
            ParserAction::CarriageReturn,
            ParserAction::LineFeed,
            ParserAction::Bell,
            ParserAction::Backspace,
        ]
    );
}

#[test]
fn parses_supported_csi_subset() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[2A\x1b[10;3H\x1b[2J\x1b[1K");

    assert_eq!(
        actions,
        vec![
            ParserAction::CursorUp(2),
            ParserAction::CursorPosition { row: 9, col: 2 },
            ParserAction::ClearDisplay(DisplayClearMode::All),
            ParserAction::ClearLine(LineClearMode::Left),
        ]
    );
}

#[test]
fn supports_split_escape_sequence_across_feeds() {
    let mut parser = Parser::default();

    assert!(parser.feed(b"\x1b[12").is_empty());
    let actions = parser.feed(b"D");
    assert_eq!(actions, vec![ParserAction::CursorBack(12)]);
}

#[test]
fn incomplete_utf8_is_buffered_and_completed_on_next_feed() {
    let mut parser = Parser::default();

    assert!(parser.feed(&[0xE2, 0x82]).is_empty());
    let actions = parser.feed(&[0xAC]);

    assert_eq!(actions, vec![ParserAction::Print('\u{20AC}')]);
}

#[test]
fn malformed_utf8_never_panics_and_emits_replacement_char() {
    let mut parser = Parser::default();
    let actions = parser.feed(&[0xF0, 0x28, 0x8C, 0x28]);

    assert!(
        actions
            .iter()
            .any(|action| matches!(action, ParserAction::Print('\u{FFFD}')))
    );
}

#[test]
fn parses_cursor_visibility() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?25l\x1b[?25h");
    assert_eq!(
        actions,
        vec![
            ParserAction::SetCursorVisible(false),
            ParserAction::SetCursorVisible(true),
        ]
    );
}

#[test]
fn parses_alternate_screen() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?1049h\x1b[?1049l");
    assert_eq!(
        actions,
        vec![
            ParserAction::AlternateScreenEnter,
            ParserAction::AlternateScreenLeave,
        ]
    );
}

#[test]
fn parses_sgr_reset() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[0m");
    assert_eq!(
        actions,
        vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
            &[Some(0)]
        ))]
    );
}

#[test]
fn parses_sgr_bold_red() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[1;31m");
    assert_eq!(
        actions,
        vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
            &[Some(1), Some(31)]
        ))]
    );
}

#[test]
fn parses_sgr_256_color() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[38;5;196m");
    assert_eq!(
        actions,
        vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
            &[Some(38), Some(5), Some(196),]
        ))]
    );
}

#[test]
fn parses_sgr_truecolor() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[38;2;255;128;0m");
    assert_eq!(
        actions,
        vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
            &[Some(38), Some(2), Some(255), Some(128), Some(0),]
        ))]
    );
}

#[test]
fn parses_tab() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\t");
    assert_eq!(actions, vec![ParserAction::Tab]);
}

#[test]
fn parses_cursor_save_restore_esc() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b7\x1b8");
    assert_eq!(
        actions,
        vec![
            ParserAction::CursorSavePosition,
            ParserAction::CursorRestorePosition,
        ]
    );
}

#[test]
fn parses_cursor_save_restore_csi() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[s\x1b[u");
    assert_eq!(
        actions,
        vec![
            ParserAction::CursorSavePosition,
            ParserAction::CursorRestorePosition,
        ]
    );
}

#[test]
fn parses_osc_window_title() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]0;Hello World\x07");
    assert_eq!(
        actions,
        vec![ParserAction::SetWindowTitle("Hello World".to_string())]
    );
}

#[test]
fn parses_osc_title_code_2() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b]2;Title\x07");
    assert_eq!(
        actions,
        vec![ParserAction::SetWindowTitle("Title".to_string())]
    );
}

#[test]
fn parses_csi_horizontal_absolute() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[10G");
    assert_eq!(actions, vec![ParserAction::CursorHorizontalAbsolute(9)]);
}

#[test]
fn parses_cursor_next_line() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[3E");
    assert_eq!(actions, vec![ParserAction::CursorNextLine(3)]);
}

#[test]
fn parses_cursor_previous_line() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[2F");
    assert_eq!(actions, vec![ParserAction::CursorPreviousLine(2)]);
}

#[test]
fn parses_vertical_position_absolute() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5d");
    assert_eq!(actions, vec![ParserAction::VerticalPositionAbsolute(4)]);
}

#[test]
fn parses_insert_delete_lines() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[3L\x1b[2M");
    assert_eq!(
        actions,
        vec![ParserAction::InsertLines(3), ParserAction::DeleteLines(2)]
    );
}

#[test]
fn parses_scroll_up_down() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5S\x1b[3T");
    assert_eq!(
        actions,
        vec![ParserAction::ScrollUp(5), ParserAction::ScrollDown(3)]
    );
}

#[test]
fn parses_erase_insert_delete_chars() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[4X\x1b[2@\x1b[3P");
    assert_eq!(
        actions,
        vec![
            ParserAction::EraseCharacters(4),
            ParserAction::InsertCharacters(2),
            ParserAction::DeleteCharacters(3),
        ]
    );
}

#[test]
fn parses_scroll_region() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5;20r");
    assert_eq!(
        actions,
        vec![ParserAction::SetScrollRegion {
            top: 4,
            bottom: Some(19)
        }]
    );
}

#[test]
fn parses_scroll_region_with_empty_bottom_as_none() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[5;r");
    assert_eq!(
        actions,
        vec![ParserAction::SetScrollRegion {
            top: 4,
            bottom: None
        }]
    );
}

#[test]
fn oversized_csi_sequence_is_degraded_without_panicking() {
    let mut parser = Parser::default();

    let mut payload = vec![0x1B, b'['];
    payload.extend(std::iter::repeat_n(b'1', MAX_CSI_LEN + 3));
    assert!(parser.feed(&payload).is_empty());

    let actions = parser.feed(b"AZ");

    assert_eq!(actions.len(), 3);
    assert!(matches!(
        actions[0],
        ParserAction::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted,
            dropped
        } if accepted == MAX_CSI_LEN && dropped == 4
    ));
    assert!(matches!(actions[1], ParserAction::UnsupportedSequence(_)));
    assert_eq!(actions[2], ParserAction::Print('Z'));
}

#[test]
fn resync_after_truncation_resets_escape_state() {
    let mut parser = Parser::default();
    assert!(parser.feed(b"\x1b[12").is_empty());

    let actions = parser.resync_after_truncation();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], ParserAction::UnsupportedSequence(_)));

    assert_eq!(parser.feed(b"A"), vec![ParserAction::Print('A')]);
}

#[test]
fn resync_after_truncation_flushes_incomplete_utf8_tail() {
    let mut parser = Parser::default();
    assert!(parser.feed(&[0xE2, 0x82]).is_empty());

    assert_eq!(
        parser.resync_after_truncation(),
        vec![ParserAction::Print('\u{FFFD}')]
    );
    assert_eq!(parser.feed(b"B"), vec![ParserAction::Print('B')]);
}

#[test]
fn unknown_private_mode_is_unsupported() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1b[?9999h");
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], ParserAction::UnsupportedSequence(_)));
}

#[test]
fn oversized_osc_is_discarded() {
    let mut parser = Parser::default();
    let mut payload = vec![0x1B, b']', b'0', b';'];
    payload.extend(std::iter::repeat_n(b'X', MAX_OSC_LEN + 100));
    payload.push(0x07);
    let actions = parser.feed(&payload);
    assert!(actions.is_empty());
}

#[test]
fn dcs_payload_is_silently_absorbed() {
    let mut parser = Parser::default();
    let actions = parser.feed(b"\x1bP+q696e646e\x1b\\");
    assert!(actions.is_empty());
}
