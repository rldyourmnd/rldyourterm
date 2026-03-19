// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::events::{CoreEvent, DisplayClearMode};
use crate::grid::{Attrs, Color, UnderlineStyle};

use super::{MouseFormat, MouseMode, TerminalState};

#[test]
fn cursor_save_restore_preserves_pen() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[1;31m\x1b7\x1b[0m\x1b8");
    assert!(state.pen.bold());
    assert_eq!(state.pen.fg, Color::Indexed(1));
}

#[test]
fn cursor_save_restore_preserves_origin_mode() {
    let mut state = TerminalState::new(10, 5, 5);
    state.scroll_region = Some((1, 3));
    state.origin_mode = true;
    let _ = state.feed(b"\x1b7");
    state.origin_mode = false;
    let _ = state.feed(b"\x1b8");
    assert!(state.origin_mode);
}

#[test]
fn sgr_hidden_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[8m");
    assert!(state.pen.hidden());
    let _ = state.feed(b"\x1b[28m");
    assert!(!state.pen.hidden());
}

#[test]
fn sgr_blink_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[5m");
    assert!(state.pen.blink());
    let _ = state.feed(b"\x1b[25m");
    assert!(!state.pen.blink());
    // SGR 6 (rapid blink) also sets blink
    let _ = state.feed(b"\x1b[6m");
    assert!(state.pen.blink());
    // SGR 0 resets all
    let _ = state.feed(b"\x1b[0m");
    assert!(!state.pen.blink());
    assert!(!state.pen.hidden());
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
fn cursor_backward_tab_uses_previous_tab_stops() {
    let mut state = TerminalState::new(40, 1, 5);
    state.cursor.col = 24;
    let _ = state.feed(b"\x1b[Z");
    assert_eq!(state.cursor.col, 16);

    let _ = state.feed(b"\x1b[2Z");
    assert_eq!(state.cursor.col, 0);
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
fn alternate_screen_restores_per_screen_modes() {
    let mut state = TerminalState::new(10, 5, 5);
    state.scroll_region = Some((1, 3));
    state.origin_mode = true;
    let _ = state.feed(
        b"\x1b[?2004h\x1b=\x1b[?1h\x1b[?7l\x1b[?1002h\x1b[?1006h\x1b[?12h\x1b[?1004h\x1b[?2026h\x1b[5 q\x1b[>3u",
    );

    let _ = state.feed(b"\x1b[?1049h");
    let _ = state.feed(
        b"\x1b[?2004l\x1b>\x1b[?1l\x1b[?7h\x1b[?1000h\x1b[?1006l\x1b[?12l\x1b[?1004l\x1b[?2026l\x1b[2 q\x1b[>1u\x1b[?6l",
    );
    let _ = state.feed(b"\x1b[?1049l");

    assert!(state.bracketed_paste_enabled());
    assert!(state.application_keypad_mode_enabled());
    assert!(state.application_cursor_keys_enabled());
    assert!(!state.auto_wrap_enabled());
    assert!(state.origin_mode);
    assert_eq!(state.mouse_mode(), MouseMode::ButtonTrack);
    assert_eq!(state.mouse_format(), MouseFormat::Sgr);
    assert!(state.cursor_blink);
    assert_eq!(state.cursor_shape(), 5);
    assert!(state.focus_reporting_enabled());
    assert!(state.synchronized_output_enabled());
    assert_eq!(state.kitty_keyboard_flags(), 3);
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
fn device_status_report_respects_origin_mode_offset() {
    let mut state = TerminalState::new(10, 5, 5);
    state.scroll_region = Some((1, 3));
    state.origin_mode = true;
    state.cursor.row = 2;
    state.cursor.col = 4;
    let events = state.feed(b"\x1b[6n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[2;5R"))
    );
}

#[test]
fn origin_mode_relative_cursor_movement_stays_within_scroll_region() {
    let mut state = TerminalState::new(10, 5, 5);
    state.scroll_region = Some((1, 3));
    state.origin_mode = true;
    state.cursor.row = 2;
    state.cursor.col = 4;

    let _ = state.feed(b"\x1b[10A");
    assert_eq!(state.cursor.row, 1);

    let _ = state.feed(b"\x1b[10B");
    assert_eq!(state.cursor.row, 3);
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
fn clear_display_uses_current_pen_background() {
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"ABCDEF");
    let _ = state.feed(b"\x1b[41m\x1b[2J");

    for row in 0..2u16 {
        for col in 0..3u16 {
            let cell = state.grid.get_cell(row, col).expect("cleared cell");
            assert_eq!(cell.ch, ' ');
            assert_eq!(cell.attrs.bg, Color::Indexed(1));
        }
    }
}

#[test]
fn clear_line_and_erase_chars_use_current_pen_background() {
    let mut line_clear_state = TerminalState::new(5, 1, 5);
    let _ = line_clear_state.feed(b"ABCDE");
    line_clear_state.cursor.col = 2;
    line_clear_state.cursor.wrap_pending = false;
    let _ = line_clear_state.feed(b"\x1b[42m\x1b[K");
    for col in 2..5u16 {
        let cell = line_clear_state
            .grid
            .get_cell(0, col)
            .expect("line-cleared cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(2));
    }

    let mut erase_state = TerminalState::new(5, 1, 5);
    let _ = erase_state.feed(b"ABCDE");
    erase_state.cursor.col = 1;
    erase_state.cursor.wrap_pending = false;
    let _ = erase_state.feed(b"\x1b[44m\x1b[2X");
    for col in 1..3u16 {
        let cell = erase_state.grid.get_cell(0, col).expect("erased cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(4));
    }
}

#[test]
fn insert_delete_chars_and_lines_use_current_pen_background() {
    let mut insert_chars_state = TerminalState::new(5, 1, 5);
    let _ = insert_chars_state.feed(b"ABCDE");
    insert_chars_state.cursor.col = 1;
    insert_chars_state.cursor.wrap_pending = false;
    let _ = insert_chars_state.feed(b"\x1b[45m\x1b[2@");
    for col in 1..3u16 {
        let cell = insert_chars_state
            .grid
            .get_cell(0, col)
            .expect("inserted blank cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(5));
    }

    let mut delete_chars_state = TerminalState::new(5, 1, 5);
    let _ = delete_chars_state.feed(b"ABCDE");
    delete_chars_state.cursor.col = 1;
    delete_chars_state.cursor.wrap_pending = false;
    let _ = delete_chars_state.feed(b"\x1b[46m\x1b[2P");
    for col in 3..5u16 {
        let cell = delete_chars_state
            .grid
            .get_cell(0, col)
            .expect("deleted trailing blank cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(6));
    }

    let mut insert_lines_state = TerminalState::new(3, 3, 5);
    let _ = insert_lines_state.feed(b"AAABBBCCC");
    insert_lines_state.cursor.row = 1;
    let _ = insert_lines_state.feed(b"\x1b[42m\x1b[L");
    for col in 0..3u16 {
        let cell = insert_lines_state
            .grid
            .get_cell(1, col)
            .expect("inserted line blank cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(2));
    }

    let mut delete_lines_state = TerminalState::new(3, 3, 5);
    let _ = delete_lines_state.feed(b"AAABBBCCC");
    delete_lines_state.cursor.row = 1;
    let _ = delete_lines_state.feed(b"\x1b[42m\x1b[M");
    for col in 0..3u16 {
        let cell = delete_lines_state
            .grid
            .get_cell(2, col)
            .expect("deleted line trailing blank cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(2));
    }
}

#[test]
fn line_feed_scroll_uses_current_pen_background() {
    let mut state = TerminalState::new(3, 2, 5);
    let _ = state.feed(b"ABCDEF");
    state.cursor.row = 1;
    state.cursor.col = 0;
    state.cursor.wrap_pending = false;
    let _ = state.feed(b"\x1b[41m\n");

    for col in 0..3u16 {
        let cell = state.grid.get_cell(1, col).expect("scrolled blank cell");
        assert_eq!(cell.ch, ' ');
        assert_eq!(cell.attrs.bg, Color::Indexed(1));
    }
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
fn xtwinops_character_size_query_emits_terminal_response() {
    let mut state = TerminalState::new(10, 5, 5);
    let events = state.feed(b"\x1b[18t");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[8;5;10t"))
    );
}

#[test]
fn xtwinops_pixel_size_query_emits_terminal_response_when_metadata_exists() {
    let mut state = TerminalState::new(10, 5, 5);
    state.set_viewport_pixels(800, 600);
    let events = state.feed(b"\x1b[14t");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[4;600;800t")
        )
    );
}

#[test]
fn xtwinops_pixel_size_query_is_silent_without_metadata() {
    let mut state = TerminalState::new(10, 5, 5);
    let events = state.feed(b"\x1b[14t");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { .. }))
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
    let _ = state.feed(b"\x1b[?1006h");
    let _ = state.feed(b"\x1b[?47h");
    // Alternate screen should be active, grid cleared
    assert_eq!(state.grid.row_string(0).expect("alt row 0"), "    ");
    // Cursor is NOT saved (simple mode)
    let _ = state.feed(b"\x1b[?1006lXY");
    let _ = state.feed(b"\x1b[?47l");
    // Main screen restored
    assert_eq!(state.grid.row_string(0).expect("main row 0"), "ABCD");
    // Cursor should be restored to position before enter (simple doesn't save)
    assert_eq!(state.cursor.row, cursor_before.row);
    assert_eq!(state.mouse_format(), MouseFormat::Sgr);
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

// ── Hyperlink state tracking (OSC 8) ───────────────────────

#[test]
fn osc_8_hyperlink_state_tracks_current_link() {
    let mut state = TerminalState::new(80, 24, 5);
    assert!(state.current_hyperlink().is_none());
    let _ = state.feed(b"\x1b]8;;https://example.com\x07");
    assert_eq!(state.current_hyperlink(), Some("https://example.com"));
    let _ = state.feed(b"\x1b]8;;\x07");
    assert!(state.current_hyperlink().is_none());
}

// ── DA2 / XTVERSION / DECRQM dispatch ─────────────────────

#[test]
fn da2_emits_terminal_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b[>c");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[>0;0;0c"))
    );
}

#[test]
fn xtversion_emits_terminal_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b[>q");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1bP>|rldyourterm 0.1.0\x1b\\"))
    );
}

#[test]
fn decrqm_reports_set_mode() {
    let mut state = TerminalState::new(10, 4, 5);
    // Enable bracketed paste
    let _ = state.feed(b"\x1b[?2004h");
    let events = state.feed(b"\x1b[?2004$p");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?2004;1$y")
        )
    );
}

#[test]
fn decrqm_reports_reset_mode() {
    let mut state = TerminalState::new(10, 4, 5);
    // Mode 2004 is off by default
    let events = state.feed(b"\x1b[?2004$p");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?2004;2$y")
        )
    );
}

#[test]
fn decrqm_reports_grapheme_cluster_mode_state() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b[?2027$p");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?2027;2$y")
        )
    );

    let _ = state.feed(b"\x1b[?2027h");
    assert!(state.grapheme_cluster_mode);
    let events = state.feed(b"\x1b[?2027$p");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?2027;1$y")
        )
    );
}

#[test]
fn alternate_screen_restores_grapheme_cluster_mode() {
    let mut state = TerminalState::new(10, 4, 5);
    let _ = state.feed(b"\x1b[?2027h");
    assert!(state.grapheme_cluster_mode);

    let _ = state.feed(b"\x1b[?1049h");
    let _ = state.feed(b"\x1b[?2027l");
    assert!(!state.grapheme_cluster_mode);
    let _ = state.feed(b"\x1b[?1049l");

    assert!(state.grapheme_cluster_mode);
}

#[test]
fn decrqm_reports_unrecognized_mode() {
    let mut state = TerminalState::new(10, 4, 5);
    // Mode 9999 is not recognized
    let events = state.feed(b"\x1b[?9999$p");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?9999;0$y")
        )
    );
}

#[test]
fn query_foreground_color_emits_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b]10;?\x07");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b]10;rgb:d8d8/d8d8/d8d8\x1b\\")
    ));
}

#[test]
fn query_background_color_emits_response() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b]11;?\x07");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b]11;rgb:1414/1b1b/1f1f\x1b\\")
    ));
}

#[test]
fn dcs_tmux_passthrough_applies_inner_window_title() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1bPtmux;\x1b\x1b]0;tmux wrapped\x07\x1b\\");
    assert_eq!(state.window_title(), "tmux wrapped");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "tmux wrapped")
        )
    );
}

#[test]
fn decrqss_reports_current_sgr_state() {
    let mut state = TerminalState::new(10, 4, 5);
    let _ = state.feed(b"\x1b[1;31m");
    let events = state.feed(b"\x1bP$qm\x1b\\");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1bP1$r1;31m\x1b\\")
    ));
}

#[test]
fn decrqss_reports_current_scroll_region() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[2;4r");
    let events = state.feed(b"\x1bP$qr\x1b\\");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1bP1$r2;4r\x1b\\")
    ));
}

#[test]
fn decrqss_reports_current_cursor_style() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[5 q");
    let events = state.feed(b"\x1bP$q q\x1b\\");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1bP1$r5 q\x1b\\")
    ));
}

#[test]
fn decrqss_rejects_unsupported_requests() {
    let mut state = TerminalState::new(10, 5, 5);
    let events = state.feed(b"\x1bP$q\"p\x1b\\");
    assert!(
        events.iter().any(
            |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1bP0$r\x1b\\")
        )
    );
}

#[test]
fn osc_4_query_palette_color_emits_current_entry() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b]4;1;?\x07");
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b]4;1;rgb:aaaa/0000/0000\x1b\\")
    ));
}

#[test]
fn osc_4_set_then_query_uses_updated_palette_entry() {
    let mut state = TerminalState::new(10, 4, 5);
    let events = state.feed(b"\x1b]4;1;rgb:12/34/56;1;?\x07");
    assert_eq!(state.palette_color(1), 0x123456);
    assert!(events.iter().any(
        |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b]4;1;rgb:1212/3434/5656\x1b\\")
    ));
}

#[test]
fn osc_104_resets_specific_palette_entry() {
    let mut state = TerminalState::new(10, 4, 5);
    let _ = state.feed(b"\x1b]4;1;rgb:12/34/56\x07");
    assert_eq!(state.palette_color(1), 0x123456);

    let _ = state.feed(b"\x1b]104;1\x07");
    assert_eq!(state.palette_color(1), 0x00_aa0000);
}

#[test]
fn osc_104_without_params_resets_entire_palette() {
    let mut state = TerminalState::new(10, 4, 5);
    let _ = state.feed(b"\x1b]4;1;rgb:12/34/56;2;rgb:65/43/21\x07");
    assert_eq!(state.palette_color(1), 0x123456);
    assert_eq!(state.palette_color(2), 0x654321);

    let _ = state.feed(b"\x1b]104\x07");
    assert_eq!(state.palette_color(1), 0x00_aa0000);
    assert_eq!(state.palette_color(2), 0x00_00aa00);
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
    let _events = state.feed(b"\x1b]52;c;SGVsbG8=\x07");

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
    assert!(state.pen.double_underline());
    assert!(!state.pen.underline());
    assert_eq!(state.pen.underline_style(), UnderlineStyle::Double);
    // SGR 24 resets both underline and double_underline
    let _ = state.feed(b"\x1b[24m");
    assert!(!state.pen.double_underline());
    assert!(!state.pen.underline());
    assert_eq!(state.pen.underline_style(), UnderlineStyle::None);
}

#[test]
fn sgr_colon_underline_variants_switch_styles() {
    let mut state = TerminalState::new(10, 5, 5);

    let _ = state.feed(b"\x1b[4:3m");
    assert_eq!(state.pen.underline_style(), UnderlineStyle::Curly);

    let _ = state.feed(b"\x1b[4:4m");
    assert_eq!(state.pen.underline_style(), UnderlineStyle::Dotted);

    let _ = state.feed(b"\x1b[4:5m");
    assert_eq!(state.pen.underline_style(), UnderlineStyle::Dashed);

    let _ = state.feed(b"\x1b[4:0m");
    assert_eq!(state.pen.underline_style(), UnderlineStyle::None);
}

#[test]
fn sgr_overline_sets_and_resets() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[53m");
    assert!(state.pen.overline());
    let _ = state.feed(b"\x1b[55m");
    assert!(!state.pen.overline());
}

#[test]
fn sgr_underline_color_sets_rgb() {
    let mut state = TerminalState::new(10, 5, 5);
    // SGR 58;2;255;0;128 sets underline color to RGB(255,0,128)
    let _ = state.feed(b"\x1b[58;2;255;0;128m");
    assert_eq!(state.pen.underline_color, Color::Rgb(255, 0, 128));
}

#[test]
fn sgr_colon_extended_colors_set_fg_bg_and_underline_color() {
    let mut state = TerminalState::new(10, 5, 5);

    let _ = state.feed(b"\x1b[38:2::12:34:56;48:5:42;58:2::255:0:128m");
    assert_eq!(state.pen.fg, Color::Rgb(12, 34, 56));
    assert_eq!(state.pen.bg, Color::Indexed(42));
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

// --- DECSCUSR cursor shape ---

#[test]
fn decscusr_sets_cursor_shape() {
    let mut state = TerminalState::new(10, 5, 5);
    assert_eq!(state.cursor_shape(), 0);
    let _ = state.feed(b"\x1b[5 q");
    assert_eq!(state.cursor_shape(), 5);
    let _ = state.feed(b"\x1b[2 q");
    assert_eq!(state.cursor_shape(), 2);
    let _ = state.feed(b"\x1b[0 q");
    assert_eq!(state.cursor_shape(), 0);
}

#[test]
fn kitty_keyboard_push_sets_flags() {
    let mut state = TerminalState::new(10, 5, 5);
    assert_eq!(state.kitty_keyboard_flags(), 0);
    let _ = state.feed(b"\x1b[>1u");
    assert_eq!(state.kitty_keyboard_flags(), 1);
}

#[test]
fn kitty_keyboard_push_higher_flags() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[>31u");
    assert_eq!(state.kitty_keyboard_flags(), 31);
}

#[test]
fn kitty_keyboard_pop_resets_flags() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[>1u");
    assert_eq!(state.kitty_keyboard_flags(), 1);
    let _ = state.feed(b"\x1b[<u");
    assert_eq!(state.kitty_keyboard_flags(), 0);
}

#[test]
fn kitty_keyboard_query_responds() {
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[>3u");
    let events = state.feed(b"\x1b[?u");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?3u"))
    );
}

#[test]
fn kitty_keyboard_query_responds_zero_when_disabled() {
    let mut state = TerminalState::new(10, 5, 5);
    let events = state.feed(b"\x1b[?u");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?0u"))
    );
}

#[test]
fn decstbm_zero_bottom_does_not_underflow() {
    // CSI 0;0r sends both params as 0. The bottom param (0) must not underflow
    // when converted from 1-based to 0-based via subtraction.
    let mut state = TerminalState::new(10, 5, 5);
    let _ = state.feed(b"\x1b[0;0r");
    // With both params defaulting, scroll_region should be cleared (full screen)
    assert_eq!(state.scroll_region, None);
    // Cursor must be homed
    assert_eq!(state.cursor.row, 0);
    assert_eq!(state.cursor.col, 0);
}
