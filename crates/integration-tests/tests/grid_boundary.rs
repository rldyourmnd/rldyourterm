// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, row, term_sized};

// ── Minimal grid dimensions ─────────────────────────────────

#[test]
fn grid_1x1_basic_operations() {
    let mut t = term_sized(1, 1);
    feed_bytes(&mut t, b"A");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0); // wrap_pending, col stays at 0 (last col)
    assert!(t.cursor.wrap_pending);
    assert_eq!(row(&t, 0), "A");
}

#[test]
fn grid_1x1_wrap_and_scroll() {
    let mut t = TerminalState::new(1, 1, 100);
    feed_bytes(&mut t, b"AB");
    // 'A' should scroll to scrollback, 'B' on screen
    assert_eq!(row(&t, 0), "B");
    assert_eq!(t.scrollback.len(), 1);
    assert_eq!(t.scrollback.get(0), Some("A"));
}

#[test]
fn grid_1x1_cursor_movement_clamped() {
    let mut t = term_sized(1, 1);
    // Try to move cursor beyond grid
    feed_bytes(&mut t, b"\x1b[99;99H");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn grid_1x1_erase_operations() {
    let mut t = term_sized(1, 1);
    feed_bytes(&mut t, b"X");
    assert_eq!(row(&t, 0), "X");
    // CSI 2K = erase entire line regardless of cursor position
    feed_bytes(&mut t, b"\x1b[2K");
    assert_eq!(row(&t, 0), "");
}

#[test]
fn grid_2x1_narrow_terminal() {
    let mut t = term_sized(2, 1);
    feed_bytes(&mut t, b"AB");
    assert_eq!(row(&t, 0), "AB");
    assert!(t.cursor.wrap_pending);
}

#[test]
fn grid_1x2_single_column() {
    let mut t = TerminalState::new(1, 2, 100);
    feed_bytes(&mut t, b"A\nB");
    assert_eq!(row(&t, 0), "A");
    assert_eq!(row(&t, 1), "B");
}

// ── Wide characters at grid boundaries ──────────────────────

#[test]
fn wide_char_at_last_column_wraps() {
    let mut t = TerminalState::new(5, 3, 100);
    // Fill 4 columns, then wide char (needs 2 cols, only 1 left)
    feed_bytes(&mut t, b"ABCD");
    feed_bytes(&mut t, "漢".as_bytes()); // width=2, wraps to next line
    // Row 0: "ABCD " (5th col stays blank)
    assert_eq!(row(&t, 0), "ABCD");
    // Row 1: wide char starts at col 0
    assert_eq!(row(&t, 1), "漢");
}

#[test]
fn wide_char_exactly_fills_row() {
    let mut t = term_sized(4, 3);
    // 4 columns, 2 wide chars = exactly full
    feed_bytes(&mut t, "漢字".as_bytes());
    assert_eq!(row(&t, 0), "漢字");
    assert!(t.cursor.wrap_pending);
    assert_eq!(t.cursor.row, 0);
}

#[test]
fn wide_char_on_1_column_grid_skips() {
    let mut t = TerminalState::new(1, 3, 100);
    // Wide char cannot fit in 1-column grid - should wrap or be skipped
    feed_bytes(&mut t, "漢".as_bytes());
    feed_bytes(&mut t, b"X");
    // X should be visible somewhere
    let mut found = false;
    for r in 0..3 {
        if row(&t, r).contains('X') {
            found = true;
            break;
        }
    }
    assert!(found, "ASCII char after wide char must be visible");
}

#[test]
fn wide_char_overwrites_at_boundary() {
    let mut t = term_sized(6, 2);
    // Fill with single-width chars
    feed_bytes(&mut t, b"ABCDEF");
    // Move back and overwrite with wide char at col 4
    feed_bytes(&mut t, b"\x1b[1;5H"); // row 1, col 5 (0-indexed: 4)
    feed_bytes(&mut t, "漢".as_bytes());
    // Cols 4-5 should have the wide char
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[4].ch, '漢');
    assert_eq!(cells[4].width, 2);
    assert_eq!(cells[5].width, 0); // continuation
}

// ── Max dimension boundaries ────────────────────────────────

#[test]
fn large_grid_dimensions() {
    let mut t = term_sized(500, 200);
    // Write at far corner
    feed_bytes(&mut t, b"\x1b[200;500H");
    assert_eq!(t.cursor.row, 199);
    assert_eq!(t.cursor.col, 499);
    feed_bytes(&mut t, b"Z");
    assert_eq!(t.grid.get_char(199, 499).unwrap(), 'Z');
}

#[test]
fn cursor_movement_clamps_to_grid() {
    let mut t = term_sized(10, 5);
    // CUU (cursor up) beyond top
    feed_bytes(&mut t, b"\x1b[999A");
    assert_eq!(t.cursor.row, 0);
    // CUD (cursor down) beyond bottom
    feed_bytes(&mut t, b"\x1b[999B");
    assert_eq!(t.cursor.row, 4);
    // CUF (cursor forward) beyond right
    feed_bytes(&mut t, b"\x1b[999C");
    assert_eq!(t.cursor.col, 9);
    // CUB (cursor back) beyond left
    feed_bytes(&mut t, b"\x1b[999D");
    assert_eq!(t.cursor.col, 0);
}

// ── Erase operations at boundaries ──────────────────────────

#[test]
fn erase_in_display_from_cursor_at_origin() {
    let mut t = term_sized(10, 5);
    // Fill grid
    for _ in 0..5 {
        feed_bytes(&mut t, b"XXXXXXXXXX\r\n");
    }
    // Home cursor, erase from cursor to end (CSI 0J)
    feed_bytes(&mut t, b"\x1b[H\x1b[0J");
    for r in 0..5 {
        assert_eq!(row(&t, r), "", "row {} should be cleared", r);
    }
}

#[test]
fn erase_in_display_to_cursor_at_end() {
    let mut t = term_sized(10, 5);
    for _ in 0..5 {
        feed_bytes(&mut t, b"XXXXXXXXXX\r\n");
    }
    // Move to last cell, erase from start to cursor (CSI 1J)
    feed_bytes(&mut t, b"\x1b[5;10H\x1b[1J");
    for r in 0..4 {
        assert_eq!(row(&t, r), "", "row {} should be cleared", r);
    }
}

#[test]
fn erase_line_variants() {
    let mut t = term_sized(10, 3);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Move to col 5, erase from cursor to end of line (CSI 0K)
    feed_bytes(&mut t, b"\x1b[1;6H\x1b[0K");
    assert_eq!(row(&t, 0), "ABCDE");

    feed_bytes(&mut t, b"\x1b[2;1HABCDEFGHIJ");
    // Erase from start of line to cursor (CSI 1K)
    feed_bytes(&mut t, b"\x1b[2;6H\x1b[1K");
    // Cols 0-5 cleared, cols 6-9 remain
    assert_eq!(row(&t, 1), "      GHIJ");

    feed_bytes(&mut t, b"\x1b[3;1HABCDEFGHIJ");
    // Erase entire line (CSI 2K)
    feed_bytes(&mut t, b"\x1b[3;5H\x1b[2K");
    assert_eq!(row(&t, 2), "");
}

// ── Insert/Delete at edges ──────────────────────────────────

#[test]
fn insert_chars_at_last_column() {
    let mut t = term_sized(10, 2);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Move to last column, insert 1 char
    feed_bytes(&mut t, b"\x1b[1;10H\x1b[1@");
    // Last char should be pushed off (J disappears), blank inserted
    let text = row(&t, 0);
    assert!(text.len() <= 10, "row should not exceed grid width");
}

#[test]
fn delete_chars_at_first_column() {
    let mut t = term_sized(10, 2);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Home cursor, delete 3 chars
    feed_bytes(&mut t, b"\x1b[1;1H\x1b[3P");
    assert_eq!(row(&t, 0), "DEFGHIJ");
}

#[test]
fn insert_lines_at_grid_bottom() {
    let mut t = term_sized(10, 5);
    for i in 0..5 {
        let ch = (b'A' + i) as char;
        feed_bytes(
            &mut t,
            format!("{}\r\n", ch.to_string().repeat(10)).as_bytes(),
        );
    }
    // Move to last row, insert 1 line
    feed_bytes(&mut t, b"\x1b[5;1H\x1b[1L");
    // Last row should now be blank (previous content scrolled off bottom)
    assert_eq!(row(&t, 4), "");
}

#[test]
fn delete_lines_at_grid_top() {
    let mut t = term_sized(10, 5);
    for i in 0..5 {
        let ch = (b'A' + i) as char;
        if i < 4 {
            feed_bytes(
                &mut t,
                format!("{}\r\n", ch.to_string().repeat(10)).as_bytes(),
            );
        } else {
            feed_bytes(&mut t, ch.to_string().repeat(10).as_bytes());
        }
    }
    // Home cursor, delete 2 lines
    feed_bytes(&mut t, b"\x1b[1;1H\x1b[2M");
    assert_eq!(row(&t, 0), "CCCCCCCCCC");
    assert_eq!(row(&t, 1), "DDDDDDDDDD");
    assert_eq!(row(&t, 2), "EEEEEEEEEE");
}

// ── Scroll region boundaries ────────────────────────────────

#[test]
fn scroll_region_single_row() {
    let mut t = term_sized(10, 5);
    // Set region to single row (row 3, 1-indexed)
    feed_bytes(&mut t, b"\x1b[3;3r");
    feed_bytes(&mut t, b"\x1b[3;1H");
    // Writing past single-row region
    feed_bytes(&mut t, b"AAAAAAAAAA\n");
    // Should not affect rows outside the region
    assert_eq!(row(&t, 0), "");
    assert_eq!(row(&t, 4), "");
}

#[test]
fn scroll_region_full_grid() {
    let mut t = term_sized(10, 5);
    // Set region to full grid (default behavior)
    feed_bytes(&mut t, b"\x1b[1;5r");
    for i in 0..10 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Should behave like normal scrolling
    assert!(row(&t, 0).starts_with("Line"));
}

#[test]
fn scroll_region_reset_by_decstbm_without_params() {
    let mut t = term_sized(10, 5);
    // Set non-default region
    feed_bytes(&mut t, b"\x1b[2;4r");
    // Reset with empty params
    feed_bytes(&mut t, b"\x1b[r");
    // Scroll region should be None (full screen)
    // Verify by scrolling - content should scroll entire grid
    for i in 0..10 {
        feed_bytes(&mut t, format!("L{}\r\n", i).as_bytes());
    }
    // First visible line should not be L0 (it scrolled off)
    assert_ne!(row(&t, 0), "L0");
}
