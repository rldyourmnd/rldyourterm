// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::events::{CoreEvent, DisplayClearMode, IngestDegradeReason};
use crate::grid::{Attrs, Color};

use super::{FEED_CHUNK_BYTES, MAX_FEED_BYTES_PER_CALL, TerminalState};

#[test]
fn feed_wraps_and_scrolls_into_scrollback() {
    let mut state = TerminalState::new(3, 2, 10);
    // With deferred wrap, 'i' at last column sets wrap_pending
    // but does NOT trigger the second scroll until the next char.
    let events = state.feed(b"abcdefghi");

    assert_eq!(state.grid.row_string(0).expect("row 0"), "def");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "ghi");
    assert_eq!(state.scrollback.iter().collect::<Vec<_>>(), vec!["abc"]);
    assert!(state.cursor.wrap_pending);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, CoreEvent::GridScrolled { .. }))
            .count(),
        1
    );

    // Feeding one more char triggers the deferred wrap and second scroll
    let events2 = state.feed(b"j");
    assert_eq!(state.grid.row_string(0).expect("row 0 after j"), "ghi");
    assert_eq!(state.grid.row_string(1).expect("row 1 after j"), "j  ");
    assert_eq!(
        state.scrollback.iter().collect::<Vec<_>>(),
        vec!["abc", "def"]
    );
    assert!(!state.cursor.wrap_pending);
    assert_eq!(
        events2
            .iter()
            .filter(|event| matches!(event, CoreEvent::GridScrolled { .. }))
            .count(),
        1
    );
}

#[test]
fn scrollback_trim_event_is_emitted_at_cap_boundary() {
    let mut state = TerminalState::new(2, 1, 1);
    // With deferred wrap, 5 chars triggers 2 scrolls:
    // 'b' sets wrap_pending, 'c' triggers 1st scroll ("ab" -> scrollback),
    // 'd' sets wrap_pending, 'e' triggers 2nd scroll ("cd" -> scrollback, "ab" trimmed).
    let events = state.feed(b"abcde");

    assert_eq!(state.scrollback.len(), 1);
    assert_eq!(state.scrollback.get(0), Some("cd"));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::ScrollbackTrimmed { dropped: 1 }))
    );
}

#[test]
fn clear_display_all_sequence_clears_grid() {
    let mut state = TerminalState::new(4, 2, 5);
    let _ = state.feed(b"ab");
    let events = state.feed(b"\x1b[2J");

    assert_eq!(state.grid.row_string(0).expect("row 0"), "    ");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "    ");
    assert!(events.iter().any(|event| matches!(
        event,
        CoreEvent::DisplayCleared {
            mode: DisplayClearMode::All
        }
    )));
}

#[test]
fn unsupported_sequence_is_reported_without_panicking() {
    let mut state = TerminalState::new(4, 1, 5);
    // Use a truly unsupported private mode
    let events = state.feed(b"\x1b[?9999h");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
    );
}

#[test]
fn malformed_utf8_feed_is_safe() {
    let mut state = TerminalState::new(8, 1, 5);
    let _ = state.feed(&[0xF0, 0x28, 0x8C, 0x28]);

    assert_eq!(state.grid.get_char(0, 0).expect("cell"), '\u{FFFD}');
}

#[test]
fn oversized_feed_is_bounded_and_reported() {
    let mut state = TerminalState::new(1, 1, 1);
    let bytes = vec![b'x'; MAX_FEED_BYTES_PER_CALL + 17];

    let events = state.feed(&bytes);

    assert!(events.iter().any(|event| matches!(
        event,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::InputFeedTooLarge,
            accepted,
            dropped
        } if *accepted == MAX_FEED_BYTES_PER_CALL && *dropped == 17
    )));
}

#[test]
fn feed_into_matches_feed_behavior() {
    let payload = b"abc\x1b[31mZ\x1b[0m\r\n\x1b]0;wave\x07";

    let mut via_feed = TerminalState::new(8, 2, 8);
    let mut via_feed_into = TerminalState::new(8, 2, 8);

    let expected_events = via_feed.feed(payload);
    let mut reused_events = vec![CoreEvent::Bell];
    via_feed_into.feed_into(payload, &mut reused_events);

    assert_eq!(reused_events, expected_events);
    assert_eq!(via_feed.cursor, via_feed_into.cursor);
    assert_eq!(via_feed.pen, via_feed_into.pen);
    assert_eq!(via_feed.window_title(), via_feed_into.window_title());
    assert_eq!(
        via_feed.scrollback.iter().collect::<Vec<_>>(),
        via_feed_into.scrollback.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        via_feed.grid.row_string(0).expect("row 0"),
        via_feed_into.grid.row_string(0).expect("row 0")
    );
    assert_eq!(
        via_feed.grid.row_string(1).expect("row 1"),
        via_feed_into.grid.row_string(1).expect("row 1")
    );
}

#[test]
fn feed_into_clears_and_reuses_output_buffer() {
    let mut state = TerminalState::new(4, 1, 4);
    let mut events = vec![CoreEvent::Bell];

    state.feed_into(b"A", &mut events);
    assert!(
        events.iter().all(|event| !matches!(event, CoreEvent::Bell)),
        "reused event buffer must be cleared before collecting new events"
    );
    let reused_capacity = events.capacity();

    state.feed_into(b"", &mut events);
    assert!(events.is_empty());
    assert_eq!(events.capacity(), reused_capacity);

    state.feed_into(b"B", &mut events);
    assert_eq!(events.capacity(), reused_capacity);
}

#[test]
fn feed_terminal_responses_into_exposes_only_response_payloads() {
    let mut state = TerminalState::new(4, 1, 4);
    let mut responses = vec![b"stale".to_vec()];

    state.feed_terminal_responses_into(b"\x1b[5nplain", &mut responses);
    assert_eq!(responses, vec![b"\x1b[0n".to_vec()]);

    let reused_capacity = responses.capacity();
    state.feed_terminal_responses_into(b"plain text", &mut responses);
    assert!(responses.is_empty());
    assert_eq!(responses.capacity(), reused_capacity);
}

#[test]
fn parser_degrade_action_maps_to_core_event() {
    let mut state = TerminalState::new(4, 2, 4);
    let events = state.apply_action(crate::parser::ParserAction::IngestDegraded {
        reason: IngestDegradeReason::CsiSequenceTooLong,
        accepted: 64,
        dropped: 3,
    });

    assert_eq!(
        events,
        vec![CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted: 64,
            dropped: 3,
        }]
    );
}

#[test]
fn burst_oversized_csi_is_discarded_and_keeps_events_bounded() {
    let mut state = TerminalState::new(8, 2, 8);
    let mut bytes = vec![0x1B, b'['];
    bytes.extend(std::iter::repeat_n(b'1', FEED_CHUNK_BYTES * 2));
    bytes.push(b'A');
    bytes.push(b'Z');

    let events = state.feed(&bytes);

    assert!(events.iter().any(|event| matches!(
        event,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted,
            dropped
        } if *accepted > 0 && *dropped > 0
    )));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
    );
    assert_eq!(state.grid.get_char(0, 0).expect("cell"), 'Z');
    assert_eq!(state.grid.row_string(0).expect("row 0"), "Z       ");
}

#[test]
fn oversized_feed_truncation_resyncs_parser_before_next_feed() {
    let mut state = TerminalState::new(4, 2, 10);
    let mut bytes = vec![b'x'; MAX_FEED_BYTES_PER_CALL - 1];
    bytes.push(0x1B);
    bytes.push(b'[');

    let first_events = state.feed(&bytes);
    assert!(first_events.iter().any(|event| matches!(
        event,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::InputFeedTooLarge,
            accepted,
            dropped
        } if *accepted == MAX_FEED_BYTES_PER_CALL && *dropped == 1
    )));
    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
    );

    let _second_events = state.feed(b"A");
    // Parser resynced: 'A' appears in the grid (exact position depends on scroll state)
    let row0 = state.grid.row_string(0).expect("row 0");
    let row1 = state.grid.row_string(1).expect("row 1");
    assert!(
        row0.contains('A') || row1.contains('A'),
        "expected 'A' somewhere in grid after resync, got row0={row0:?}, row1={row1:?}"
    );
    assert!(
        !_second_events
            .iter()
            .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
    );
}

#[test]
fn truncated_overlong_csi_still_emits_csi_degrade_event() {
    let mut state = TerminalState::new(4, 2, 10);
    let mut bytes = vec![0x1B, b'['];
    bytes.extend(std::iter::repeat_n(
        b'1',
        MAX_FEED_BYTES_PER_CALL + FEED_CHUNK_BYTES,
    ));

    let events = state.feed(&bytes);

    assert!(events.iter().any(|event| matches!(
        event,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CoreEvent::IngestDegraded {
            reason: IngestDegradeReason::InputFeedTooLarge,
            ..
        }
    )));
}

#[test]
fn sgr_sets_pen_attributes() {
    let mut state = TerminalState::new(10, 2, 5);
    // ESC[1;31m = bold + red fg
    let _ = state.feed(b"\x1b[1;31mA");
    assert!(state.pen.bold());
    assert_eq!(state.pen.fg, Color::Indexed(1));
    let cell = state.grid.get_cell(0, 0).expect("cell");
    assert_eq!(cell.ch, 'A');
    assert!(cell.attrs.bold());
    assert_eq!(cell.attrs.fg, Color::Indexed(1));
}

#[test]
fn sgr_reset_clears_pen() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[1;31mA\x1b[0mB");
    assert_eq!(state.pen, Attrs::default());
    let cell = state.grid.get_cell(0, 1).expect("cell B");
    assert_eq!(cell.attrs, Attrs::default());
}

#[test]
fn sgr_256_color() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[38;5;196mR");
    assert_eq!(state.pen.fg, Color::Indexed(196));
}

#[test]
fn sgr_truecolor() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[38;2;255;128;0mO");
    assert_eq!(state.pen.fg, Color::Rgb(255, 128, 0));
}

#[test]
fn sgr_256_color_rejects_out_of_range_index() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[38;5;256mX");
    assert_eq!(state.pen.fg, Color::Default);
}

#[test]
fn sgr_truecolor_rejects_out_of_range_component() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[38;2;256;0;0mX");
    assert_eq!(state.pen.fg, Color::Default);
}

#[test]
fn sgr_invalid_color_does_not_eat_subsequent_params() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed(b"\x1b[38;5;256;1mX");
    assert_eq!(state.pen.fg, Color::Default);
    assert!(state.pen.bold());
}

#[test]
fn combining_mark_composes_via_nfc() {
    let mut state = TerminalState::new(10, 2, 5);
    let _ = state.feed("e\u{0301}".as_bytes());
    assert_eq!(state.cursor.col, 1);
    assert_eq!(state.grid.get_char(0, 0), Ok('\u{00E9}')); // é (NFC composed)
}

#[test]
fn combining_mark_that_cannot_compose_is_dropped() {
    let mut state = TerminalState::new(10, 2, 5);
    // 'a' + combining tilde + combining dot below + combining ring above
    // NFC of these multiple marks does not reduce to a single codepoint
    let _ = state.feed("a\u{0303}\u{0323}\u{030A}".as_bytes());
    assert_eq!(state.cursor.col, 1);
    // First combining mark composes: a + tilde → ã (U+00E3)
    assert_eq!(state.grid.get_char(0, 0), Ok('\u{00E3}'));
}

#[test]
fn tab_advances_to_next_stop() {
    let mut state = TerminalState::new(20, 1, 5);
    let _ = state.feed(b"AB\t");
    assert_eq!(state.cursor.col, 8);
}
