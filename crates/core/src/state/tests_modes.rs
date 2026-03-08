use crate::events::{CoreEvent, DisplayClearMode};
use crate::grid::{Attrs, Color};

use super::TerminalState;

#[test]
fn cursor_save_restore_preserves_pen() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[1;31m\x1b7\x1b[0m\x1b8");
    assert!(state.pen.bold);
    assert_eq!(state.pen.fg, Color::Indexed(1));
}

#[test]
fn alternate_screen_enter_leave_roundtrip() {
    let mut state = TerminalState::new(4, 2, 5);
    let _ = state.feed(b"ABCD");
    let _ = state.feed(b"\x1b[?1049h");
    assert_eq!(state.grid.row_string(0).expect("alt row 0"), "    ");

    let _ = state.feed(b"XY");
    let _ = state.feed(b"\x1b[?1049l");
    assert_eq!(state.grid.row_string(0).expect("main row 0"), "ABCD");
}

#[test]
fn scroll_region_confines_scrolling() {
    let mut state = TerminalState::new(3, 5, 10);
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..3u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Set scroll region rows 1..3 (0-indexed)
    let _ = state.feed(b"\x1b[2;4r");
    // Move to row 3 (bottom of region) and send LF
    state.cursor.row = 3;
    state.cursor.col = 0;
    let _ = state.feed(b"\n");

    assert_eq!(state.grid.row_string(0).expect("row 0"), "AAA");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "CCC");
    assert_eq!(state.grid.row_string(2).expect("row 2"), "DDD");
    assert_eq!(state.grid.row_string(3).expect("row 3"), "   ");
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEE");
}

#[test]
fn resize_preserves_content() {
    let mut state = TerminalState::new(4, 3, 10);
    let _ = state.feed(b"AB");
    state.resize(6, 4);
    assert_eq!(state.grid.width(), 6);
    assert_eq!(state.grid.height(), 4);
    assert_eq!(state.grid.get_char(0, 0).expect("cell"), 'A');
    assert_eq!(state.grid.get_char(0, 1).expect("cell"), 'B');
}

#[test]
fn resize_clamps_cursor() {
    let mut state = TerminalState::new(10, 10, 10);
    state.cursor.row = 8;
    state.cursor.col = 9;
    state.resize(5, 5);
    assert_eq!(state.cursor.row, 4);
    assert_eq!(state.cursor.col, 4);
}

#[test]
fn cursor_visibility_via_csi_25() {
    let mut state = TerminalState::new(10, 2, 5);
    assert!(state.cursor.visible);
    let events = state.feed(b"\x1b[?25l");
    assert!(!state.cursor.visible);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::CursorVisibilityChanged { visible: false }))
    );
    let events = state.feed(b"\x1b[?25h");
    assert!(state.cursor.visible);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::CursorVisibilityChanged { visible: true }))
    );
}

#[test]
fn window_title_from_osc() {
    let mut state = TerminalState::new(10, 2, 5);
    let events = state.feed(b"\x1b]0;My Title\x07");
    assert_eq!(state.window_title(), "My Title");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "My Title"))
    );
}

#[test]
fn repeated_window_title_does_not_emit_duplicate_event() {
    let mut state = TerminalState::new(10, 2, 5);
    let first = state.feed(b"\x1b]0;My Title\x07");
    let second = state.feed(b"\x1b]0;My Title\x07");

    assert!(
        first
            .iter()
            .any(|e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "My Title"))
    );
    assert!(
        !second
            .iter()
            .any(|e| matches!(e, CoreEvent::WindowTitleChanged { .. }))
    );
}

#[test]
fn scroll_region_print_wrap_stays_in_region() {
    let mut state = TerminalState::new(4, 8, 10);
    for row in 0..8u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..4u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Set scroll region rows 3..6 (1-indexed: 4;7r -> 0-indexed 3..6)
    let _ = state.feed(b"\x1b[4;7r");
    // Move cursor to bottom of region, last column
    state.cursor.row = 6;
    state.cursor.col = 3;
    // Print two chars: 'X' at last column sets wrap_pending,
    // 'Y' triggers the deferred wrap + region scroll.
    let events = state.feed(b"XY");
    // After deferred wrap + scroll, cursor is at (6, 1)
    assert_eq!(state.cursor.row, 6);
    assert_eq!(state.cursor.col, 1);
    // Invariant: region-local wrap must not emit global scroll events.
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::LineWrapped { row: 6 }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CoreEvent::GridScrolled { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CoreEvent::ScrollbackTrimmed { .. }))
    );

    // Rows outside region remain unchanged.
    assert_eq!(state.grid.row_string(2).expect("row 2"), "CCCC");
    assert_eq!(state.grid.row_string(7).expect("row 7"), "HHHH");
    // Region scrolled up by one: DDDD dropped, each line shifted up.
    // Row 6 had GGGX (X written before scroll), moved to row 5.
    assert_eq!(state.grid.row_string(3).expect("row 3"), "EEEE");
    assert_eq!(state.grid.row_string(4).expect("row 4"), "FFFF");
    assert_eq!(state.grid.row_string(5).expect("row 5"), "GGGX");
    assert_eq!(state.grid.row_string(6).expect("row 6"), "Y   ");
}

#[test]
fn cursor_restore_is_non_consuming() {
    let mut state = TerminalState::new(10, 10, 5);
    // Save cursor at (5, 10) - ESC 7
    state.cursor.row = 5;
    state.cursor.col = 9;
    let _ = state.feed(b"\x1b7");
    // Restore - ESC 8
    state.cursor.row = 0;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b8");
    assert_eq!(state.cursor.row, 5);
    assert_eq!(state.cursor.col, 9);
    // Restore again - should still work (non-consuming)
    state.cursor.row = 0;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b8");
    assert_eq!(state.cursor.row, 5);
    assert_eq!(state.cursor.col, 9);
}

#[test]
fn cursor_restore_survives_resize() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 5;
    state.cursor.col = 9;
    let _ = state.feed(b"\x1b7");
    // Resize smaller
    state.resize(3, 3);
    // Restore - should clamp to (2, 2)
    let _ = state.feed(b"\x1b8");
    assert_eq!(state.cursor.row, 2);
    assert_eq!(state.cursor.col, 2);
    // Resize back to original
    state.resize(10, 10);
    // Restore again - should get original (5, 9) since saved_cursor is preserved
    let _ = state.feed(b"\x1b8");
    assert_eq!(state.cursor.row, 5);
    assert_eq!(state.cursor.col, 9);
}

#[test]
fn scroll_region_single_line() {
    let mut state = TerminalState::new(10, 5, 5);
    // ESC[3;3r -> 1-line region at row 2 (0-indexed)
    let _ = state.feed(b"\x1b[3;3r");
    assert_eq!(state.scroll_region, Some((2, 2)));
}

#[test]
fn bracketed_paste_mode_toggle() {
    let mut state = TerminalState::new(10, 2, 5);
    assert!(!state.bracketed_paste_enabled());
    let _ = state.feed(b"\x1b[?2004h");
    assert!(state.bracketed_paste_enabled());
    let _ = state.feed(b"\x1b[?2004l");
    assert!(!state.bracketed_paste_enabled());
}

#[test]
fn application_cursor_keys_mode() {
    let mut state = TerminalState::new(10, 2, 5);
    assert!(!state.application_cursor_keys_enabled());
    let _ = state.feed(b"\x1b[?1h");
    assert!(state.application_cursor_keys_enabled());
    let _ = state.feed(b"\x1b[?1l");
    assert!(!state.application_cursor_keys_enabled());
}

#[test]
fn application_keypad_mode_toggle() {
    let mut state = TerminalState::new(10, 2, 5);
    assert!(!state.application_keypad_mode_enabled());
    let _ = state.feed(b"\x1b=");
    assert!(state.application_keypad_mode_enabled());
    let _ = state.feed(b"\x1b>");
    assert!(!state.application_keypad_mode_enabled());
}

#[test]
fn auto_wrap_mode() {
    let mut state = TerminalState::new(10, 2, 5);
    assert!(state.auto_wrap_enabled());
    let _ = state.feed(b"\x1b[?7l");
    assert!(!state.auto_wrap_enabled());
    let _ = state.feed(b"\x1b[?7h");
    assert!(state.auto_wrap_enabled());
}

#[test]
fn no_wrap_mode_keeps_cursor_at_last_column_without_line_wrap() {
    let mut state = TerminalState::new(3, 1, 5);
    let _ = state.feed(b"\x1b[?7l");

    let events = state.feed(b"abcd");

    assert_eq!(state.grid.row_string(0).expect("row 0"), "abd");
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 2);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CoreEvent::LineWrapped { .. }))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, CoreEvent::GridScrolled { .. }))
    );
}

#[test]
fn osc_st_terminator_sets_title() {
    let mut state = TerminalState::new(10, 2, 5);
    let events = state.feed(b"\x1b]0;Title ST\x1b\\");
    assert_eq!(state.window_title(), "Title ST");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "Title ST"))
    );
}

#[test]
fn primary_da_emits_terminal_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b[c");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?1;2c"))
    );
}

#[test]
fn device_status_report_emits_cursor_position() {
    let mut state = TerminalState::new(10, 4, 5);
    // Move cursor to row 2, col 5 (0-based) via CursorPosition (1-based params)
    state.feed(b"\x1b[3;6H");
    let events = state.feed(b"\x1b[6n");
    // Response should be 1-based: row=3, col=6
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[3;6R"))
    );
}

#[test]
fn reverse_index_scrolls_down_at_top_of_region() {
    let mut state = TerminalState::new(4, 5, 10);
    // Set scroll region rows 1..3 (0-indexed)
    let _ = state.feed(b"\x1b[2;4r");
    // Move cursor to top of region
    state.cursor.row = 1;
    state.cursor.col = 2;
    // Reverse index at top of region should scroll down
    let _ = state.feed(b"\x1bM");
    assert_eq!(state.cursor.row, 1);
    assert_eq!(state.cursor.col, 2);
}

#[test]
fn reverse_index_moves_cursor_up() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 5;
    state.cursor.col = 3;
    let _ = state.feed(b"\x1bM");
    assert_eq!(state.cursor.row, 4);
    assert_eq!(state.cursor.col, 3);
}

#[test]
fn next_line_moves_down_and_to_col_zero() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 3;
    state.cursor.col = 7;
    let _ = state.feed(b"\x1bE");
    assert_eq!(state.cursor.row, 4);
    assert_eq!(state.cursor.col, 0);
}

#[test]
fn clear_scrollback_empties_scrollback_buffer() {
    let mut state = TerminalState::new(3, 2, 10);
    // Fill scrollback
    let _ = state.feed(b"abcdefghi");
    assert!(!state.scrollback.is_empty());
    // Clear scrollback
    let events = state.feed(b"\x1b[3J");
    assert!(state.scrollback.is_empty());
    assert!(events.iter().any(|e| matches!(
        e,
        CoreEvent::DisplayCleared {
            mode: DisplayClearMode::Scrollback
        }
    )));
}

#[test]
fn cursor_next_line_moves_down_and_to_col_zero() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 2;
    state.cursor.col = 5;
    let _ = state.feed(b"\x1b[3E");
    assert_eq!(state.cursor.row, 5);
    assert_eq!(state.cursor.col, 0);
}

#[test]
fn cursor_previous_line_moves_up_and_to_col_zero() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 5;
    state.cursor.col = 7;
    let _ = state.feed(b"\x1b[2F");
    assert_eq!(state.cursor.row, 3);
    assert_eq!(state.cursor.col, 0);
}

#[test]
fn vertical_position_absolute_sets_row() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 0;
    state.cursor.col = 5;
    let _ = state.feed(b"\x1b[4d");
    assert_eq!(state.cursor.row, 3);
    assert_eq!(state.cursor.col, 5);
}

#[test]
fn device_ok_emits_terminal_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b[5n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[0n"))
    );
}

#[test]
fn deferred_wrap_sets_flag_at_last_column() {
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"abc");
    // After 'c' at last column (col 2), cursor stays at col 2 with wrap_pending
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 2);
    assert!(state.cursor.wrap_pending);
}

#[test]
fn deferred_wrap_cr_clears_without_wrapping() {
    // This is the exact fish right-prompt scenario:
    // print to last column, then CR should stay on same row.
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"abc");
    assert!(state.cursor.wrap_pending);
    // CR clears wrap_pending and moves to col 0, same row
    let _ = state.feed(b"\r");
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 0);
    assert!(!state.cursor.wrap_pending);
}

#[test]
fn deferred_wrap_cuf_clears_without_wrapping() {
    let mut state = TerminalState::new(5, 2, 5);
    let _ = state.feed(b"abcde");
    assert!(state.cursor.wrap_pending);
    // CUF(2) clears wrap_pending and moves forward (clamped to width)
    let _ = state.feed(b"\x1b[2C");
    assert_eq!(state.cursor.row, 0);
    assert!(!state.cursor.wrap_pending);
}

#[test]
fn deferred_wrap_next_char_triggers_wrap() {
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"abc");
    assert!(state.cursor.wrap_pending);
    assert_eq!(state.cursor.row, 0);
    // Next printable char triggers the deferred wrap
    let _ = state.feed(b"d");
    assert_eq!(state.cursor.row, 1);
    assert_eq!(state.cursor.col, 1);
    assert!(!state.cursor.wrap_pending);
    assert_eq!(state.grid.row_string(0).expect("row 0"), "abc");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "d  ");
}

#[test]
fn deferred_wrap_fish_right_prompt_pattern() {
    // Simulates the fish shell right-prompt pattern that caused the staircase bug:
    // print chars to fill the last column, then CR + CUF to reposition.
    let mut state = TerminalState::new(10, 3, 5);
    // Fill row 0 to the last column
    let _ = state.feed(b"0123456789");
    assert!(state.cursor.wrap_pending);
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 9);
    // CR (like fish does after drawing right prompt)
    let _ = state.feed(b"\r");
    assert_eq!(state.cursor.row, 0); // Must stay on same row!
    assert_eq!(state.cursor.col, 0);
    assert!(!state.cursor.wrap_pending);
    // CUF to reposition cursor (like fish does to go back to command line)
    let _ = state.feed(b"\x1b[5C");
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 5);
    assert!(!state.cursor.wrap_pending);
}

#[test]
fn deferred_wrap_lf_clears_without_wrapping() {
    let mut state = TerminalState::new(3, 3, 5);
    let _ = state.feed(b"abc");
    assert!(state.cursor.wrap_pending);
    // LF clears wrap_pending and moves down, col stays at 2
    let _ = state.feed(b"\n");
    assert_eq!(state.cursor.row, 1);
    assert_eq!(state.cursor.col, 2);
    assert!(!state.cursor.wrap_pending);
}

#[test]
fn deferred_wrap_resize_clears_flag() {
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"abc");
    assert!(state.cursor.wrap_pending);
    state.resize(5, 3);
    assert!(!state.cursor.wrap_pending);
}
