use crate::events::{CoreEvent, DisplayClearMode};
use crate::grid::{Attrs, Color};

use super::{MouseFormat, MouseMode, TerminalState};

#[test]
fn cursor_save_restore_preserves_pen() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[1;31m\x1b7\x1b[0m\x1b8");
    assert!(state.pen.bold);
    assert_eq!(state.pen.fg, Color::Indexed(1));
}

#[test]
fn sgr_hidden_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[8m");
    assert!(state.pen.hidden);
    let _ = state.feed(b"\x1b[28m");
    assert!(!state.pen.hidden);
}

#[test]
fn sgr_blink_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[5m");
    assert!(state.pen.blink);
    let _ = state.feed(b"\x1b[25m");
    assert!(!state.pen.blink);
    // SGR 6 (rapid blink) also sets blink
    let _ = state.feed(b"\x1b[6m");
    assert!(state.pen.blink);
    // SGR 0 resets all
    let _ = state.feed(b"\x1b[0m");
    assert!(!state.pen.blink);
    assert!(!state.pen.hidden);
}

#[test]
fn csi_b_repeats_last_printed_char() {
    let mut state = TerminalState::new(10, 1, 5);
    // Print 'A' then repeat it 3 times via CSI 3 b
    let _ = state.feed(b"A\x1b[3b");
    assert_eq!(state.grid.row_string(0).expect("row 0"), "AAAA      ");
}

#[test]
fn csi_b_without_prior_print_is_noop() {
    let mut state = TerminalState::new(10, 1, 5);
    // No prior print, REP should do nothing
    let _ = state.feed(b"\x1b[5b");
    assert_eq!(state.grid.row_string(0).expect("row 0"), "          ");
}

#[test]
fn csi_b_default_repeats_once() {
    let mut state = TerminalState::new(10, 1, 5);
    // CSI b with no param defaults to 1 repeat
    let _ = state.feed(b"X\x1b[b");
    assert_eq!(state.grid.row_string(0).expect("row 0"), "XX        ");
}

#[test]
fn hts_sets_tab_stop_at_cursor() {
    let mut state = TerminalState::new(20, 1, 5);
    // Move to col 5 and set tab stop
    state.cursor.col = 5;
    let _ = state.feed(b"\x1bH");
    // Move to col 0 and tab - should land on col 5
    state.cursor.col = 0;
    let _ = state.feed(b"\t");
    assert_eq!(state.cursor.col, 5);
}

#[test]
fn tbc_clears_current_tab_stop() {
    let mut state = TerminalState::new(20, 1, 5);
    // Default tab at col 8. Move there and clear.
    state.cursor.col = 8;
    let _ = state.feed(b"\x1b[0g");
    // Tab from col 0 should now skip col 8 and go to col 16
    state.cursor.col = 0;
    let _ = state.feed(b"\t");
    assert_eq!(state.cursor.col, 16);
}

#[test]
fn tbc_clears_all_tab_stops() {
    let mut state = TerminalState::new(20, 1, 5);
    // Clear all tab stops
    let _ = state.feed(b"\x1b[3g");
    // Tab should go to last column (no stops left)
    state.cursor.col = 0;
    let _ = state.feed(b"\t");
    assert_eq!(state.cursor.col, 19);
}

#[test]
fn tab_uses_default_8_column_stops() {
    let mut state = TerminalState::new(40, 1, 5);
    state.cursor.col = 0;
    let _ = state.feed(b"\t");
    assert_eq!(state.cursor.col, 8);
    let _ = state.feed(b"\t");
    assert_eq!(state.cursor.col, 16);
}

#[test]
fn wide_char_occupies_two_columns() {
    let mut state = TerminalState::new(10, 1, 5);
    // CJK character U+4E16 ('世') is width 2
    let _ = state.feed("世".as_bytes());
    assert_eq!(state.cursor.col, 2);
    assert_eq!(state.grid.row_string(0).expect("row 0"), "世        ");
    // Check cell widths
    let cells = state.grid.row_cells(0).expect("cells");
    assert_eq!(cells[0].width, 2);
    assert_eq!(cells[0].ch, '世');
    assert_eq!(cells[1].width, 0); // continuation
}

#[test]
fn wide_char_at_last_column_wraps() {
    let mut state = TerminalState::new(5, 2, 5);
    // Print 4 narrow chars, then a wide char (needs 2 cols but only 1 left)
    let _ = state.feed("ABCD世".as_bytes());
    // Wide char should wrap to next line
    assert_eq!(state.grid.row_string(0).expect("row 0"), "ABCD ");
    assert_eq!(state.cursor.row, 1);
    assert_eq!(state.cursor.col, 2);
}

#[test]
fn overwrite_continuation_clears_wide_char() {
    let mut state = TerminalState::new(10, 1, 5);
    let _ = state.feed("世".as_bytes());
    // Overwrite col 1 (continuation) with narrow char
    state.cursor.col = 1;
    let _ = state.feed(b"X");
    let cells = state.grid.row_cells(0).expect("cells");
    // Col 0 should now be blank (wide char cleared)
    assert_eq!(cells[0].ch, ' ');
    assert_eq!(cells[0].width, 1);
    // Col 1 should be 'X'
    assert_eq!(cells[1].ch, 'X');
    assert_eq!(cells[1].width, 1);
}

#[test]
fn row_string_skips_continuation_cells() {
    let mut state = TerminalState::new(10, 1, 5);
    let _ = state.feed("AB世CD".as_bytes());
    // Should be "AB世CD" not "AB世 CD" (continuation cell skipped)
    let s = state.grid.row_string(0).expect("row 0");
    assert_eq!(s, "AB世CD    ");
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
fn insert_lines_outside_scroll_region_is_noop() {
    let mut state = TerminalState::new(5, 5, 10);
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..5u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Set scroll region rows 1..3 (0-indexed)
    let _ = state.feed(b"\x1b[2;4r");

    // Move cursor above scroll region (row 0) and insert lines
    state.cursor.row = 0;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b[1L");

    // Grid should be unchanged - IL is no-op outside scroll region
    assert_eq!(state.grid.row_string(0).expect("row 0"), "AAAAA");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "BBBBB");
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEEEE");

    // Move cursor below scroll region (row 4) and insert lines
    state.cursor.row = 4;
    let _ = state.feed(b"\x1b[1L");

    // Grid should still be unchanged
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEEEE");
}

#[test]
fn delete_lines_outside_scroll_region_is_noop() {
    let mut state = TerminalState::new(5, 5, 10);
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..5u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Set scroll region rows 1..3 (0-indexed)
    let _ = state.feed(b"\x1b[2;4r");

    // Move cursor above scroll region (row 0) and delete lines
    state.cursor.row = 0;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b[1M");

    // Grid should be unchanged - DL is no-op outside scroll region
    assert_eq!(state.grid.row_string(0).expect("row 0"), "AAAAA");
    assert_eq!(state.grid.row_string(1).expect("row 1"), "BBBBB");
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEEEE");

    // Move cursor below scroll region (row 4) and delete lines
    state.cursor.row = 4;
    let _ = state.feed(b"\x1b[1M");

    // Grid should still be unchanged
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEEEE");
}

#[test]
fn insert_lines_inside_scroll_region_works() {
    let mut state = TerminalState::new(5, 5, 10);
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..5u16 {
            let _ = state.grid.put_char(row, col, ch, Attrs::default());
        }
    }
    // Set scroll region rows 1..3 (0-indexed)
    let _ = state.feed(b"\x1b[2;4r");

    // Move cursor inside scroll region (row 1) and insert 1 line
    state.cursor.row = 1;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b[1L");

    // Row 0 and row 4 are outside region, unchanged
    assert_eq!(state.grid.row_string(0).expect("row 0"), "AAAAA");
    assert_eq!(state.grid.row_string(4).expect("row 4"), "EEEEE");
    // Row 1 should now be blank (inserted), B pushed down, D (row 3 content) lost
    assert_eq!(state.grid.row_string(1).expect("row 1"), "     ");
    assert_eq!(state.grid.row_string(2).expect("row 2"), "BBBBB");
    assert_eq!(state.grid.row_string(3).expect("row 3"), "CCCCC");
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

#[test]
fn mouse_mode_basic_enables_and_disables() {
    let mut state = TerminalState::new(10, 5, 5);
    assert_eq!(state.mouse_mode(), MouseMode::Off);
    let _ = state.feed(b"\x1b[?1000h");
    assert_eq!(state.mouse_mode(), MouseMode::Basic);
    let _ = state.feed(b"\x1b[?1000l");
    assert_eq!(state.mouse_mode(), MouseMode::Off);
}

#[test]
fn mouse_mode_button_track_and_any_event() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[?1002h");
    assert_eq!(state.mouse_mode(), MouseMode::ButtonTrack);
    let _ = state.feed(b"\x1b[?1003h");
    assert_eq!(state.mouse_mode(), MouseMode::AnyEvent);
    let _ = state.feed(b"\x1b[?1003l");
    assert_eq!(state.mouse_mode(), MouseMode::Off);
}

#[test]
fn mouse_sgr_format_enables_and_disables() {
    let mut state = TerminalState::new(10, 5, 5);
    assert_eq!(state.mouse_format(), MouseFormat::Normal);
    let _ = state.feed(b"\x1b[?1006h");
    assert_eq!(state.mouse_format(), MouseFormat::Sgr);
    let _ = state.feed(b"\x1b[?1006l");
    assert_eq!(state.mouse_format(), MouseFormat::Normal);
}

#[test]
fn mouse_mode_combined_enable() {
    let mut state = TerminalState::new(10, 5, 5);
    // Some programs enable mouse mode and SGR in a single CSI
    let _ = state.feed(b"\x1b[?1000;1006h");
    assert_eq!(state.mouse_mode(), MouseMode::Basic);
    assert_eq!(state.mouse_format(), MouseFormat::Sgr);
}

#[test]
fn alternate_screen_simple_mode_47() {
    let mut state = TerminalState::new(4, 2, 5);
    let _ = state.feed(b"ABCD");
    let cursor_before = state.cursor;
    let _ = state.feed(b"\x1b[?47h");
    // Alternate screen should be active, grid cleared
    assert_eq!(state.grid.row_string(0).expect("alt row 0"), "    ");
    // Cursor is NOT saved (simple mode)
    let _ = state.feed(b"XY");
    let _ = state.feed(b"\x1b[?47l");
    // Main screen restored
    assert_eq!(state.grid.row_string(0).expect("main row 0"), "ABCD");
    // Cursor should be restored to position before enter (simple doesn't save)
    assert_eq!(state.cursor.row, cursor_before.row);
}

#[test]
fn focus_reporting_toggle() {
    let mut state = TerminalState::new(10, 5, 5);
    assert!(!state.focus_reporting_enabled());
    let _ = state.feed(b"\x1b[?1004h");
    assert!(state.focus_reporting_enabled());
    let _ = state.feed(b"\x1b[?1004l");
    assert!(!state.focus_reporting_enabled());
}

#[test]
fn synchronized_output_toggle() {
    let mut state = TerminalState::new(10, 5, 5);
    assert!(!state.synchronized_output_enabled());
    let _ = state.feed(b"\x1b[?2026h");
    assert!(state.synchronized_output_enabled());
    let _ = state.feed(b"\x1b[?2026l");
    assert!(!state.synchronized_output_enabled());
}

#[test]
fn cursor_save_restore_dec_mode_1048() {
    let mut state = TerminalState::new(10, 10, 5);
    state.cursor.row = 3;
    state.cursor.col = 7;
    let _ = state.feed(b"\x1b[?1048h");
    state.cursor.row = 0;
    state.cursor.col = 0;
    let _ = state.feed(b"\x1b[?1048l");
    assert_eq!(state.cursor.row, 3);
    assert_eq!(state.cursor.col, 7);
}

// ── OSC integration tests ──────────────────────────────────

#[test]
fn osc_7_stores_cwd_and_deduplicates() {
    let mut state = TerminalState::new(80, 24, 5);
    let events = state.feed(b"\x1b]7;file://host/home/user/project\x07");
    assert_eq!(state.cwd(), "/home/user/project");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::CurrentWorkingDirectoryChanged { .. }))
    );

    // Sending same CWD again should not emit another event
    let events = state.feed(b"\x1b]7;file://host/home/user/project\x07");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, CoreEvent::CurrentWorkingDirectoryChanged { .. }))
    );

    // Different CWD should emit
    let events = state.feed(b"\x1b]7;/tmp\x07");
    assert_eq!(state.cwd(), "/tmp");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::CurrentWorkingDirectoryChanged { .. }))
    );
}

#[test]
fn osc_52_stores_pending_clipboard() {
    let mut state = TerminalState::new(80, 24, 5);
    let events = state.feed(b"\x1b]52;c;SGVsbG8=\x07");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::ClipboardSetRequested { .. }))
    );

    let pending = state.take_pending_clipboard();
    assert_eq!(pending, Some(('c', "SGVsbG8=".to_string())));
    // Second take should return None
    assert_eq!(state.take_pending_clipboard(), None);
}

#[test]
fn cwd_and_window_title_are_independent() {
    let mut state = TerminalState::new(80, 24, 5);
    let _ = state.feed(b"\x1b]0;My Title\x07");
    let _ = state.feed(b"\x1b]7;/home/user\x07");
    assert_eq!(state.window_title(), "My Title");
    assert_eq!(state.cwd(), "/home/user");
}

#[test]
fn sgr_double_underline_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[21m");
    assert!(state.pen.double_underline);
    assert!(!state.pen.underline);
    // SGR 24 resets both underline and double_underline
    let _ = state.feed(b"\x1b[24m");
    assert!(!state.pen.double_underline);
    assert!(!state.pen.underline);
}

#[test]
fn sgr_overline_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[53m");
    assert!(state.pen.overline);
    let _ = state.feed(b"\x1b[55m");
    assert!(!state.pen.overline);
}

#[test]
fn sgr_underline_color_sets_rgb() {
    let mut state = TerminalState::new(10, 5, 5);
    // SGR 58;2;255;0;128 sets underline color to RGB(255,0,128)
    let _ = state.feed(b"\x1b[58;2;255;0;128m");
    assert_eq!(state.pen.underline_color, Color::Rgb(255, 0, 128));
}

#[test]
fn sgr_underline_color_resets_to_default() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[58;2;255;0;128m");
    assert_eq!(state.pen.underline_color, Color::Rgb(255, 0, 128));
    // SGR 59 resets underline color to default
    let _ = state.feed(b"\x1b[59m");
    assert_eq!(state.pen.underline_color, Color::Default);
}
