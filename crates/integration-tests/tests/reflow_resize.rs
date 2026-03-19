// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::TerminalState;
use rldyourterm_integration_tests::{feed_bytes, row};

// ── Basic reflow on shrink ──────────────────────────────────

#[test]
fn shrink_wraps_long_line() {
    let mut t = TerminalState::new(20, 5, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJKLMNOPQRST"); // exactly 20 chars
    // Shrink to width 10
    t.resize(10, 5);
    // Line should reflow into 2 rows
    assert_eq!(row(&t, 0), "ABCDEFGHIJ");
    assert_eq!(row(&t, 1), "KLMNOPQRST");
}

#[test]
fn shrink_preserves_multiple_logical_lines() {
    let mut t = TerminalState::new(20, 5, 100);
    feed_bytes(&mut t, b"First line\r\n");
    feed_bytes(&mut t, b"Second longer line!!\r\n");
    // Shrink to 10 columns
    t.resize(10, 10);
    // "First line" is exactly 10 chars, fits one row
    assert_eq!(row(&t, 0), "First line");
    // "Second longer line!!" is 20 chars, wraps to 2 rows
    assert_eq!(row(&t, 1), "Second lon");
    assert_eq!(row(&t, 2), "ger line!!");
}

#[test]
fn shrink_pushes_overflow_to_scrollback() {
    let mut t = TerminalState::new(20, 3, 100);
    // Fill 3 rows
    feed_bytes(&mut t, b"AAAAAAAAAAAAAAAAAAAA\r\n"); // 20 chars
    feed_bytes(&mut t, b"BBBBBBBBBBBBBBBBBBBB\r\n"); // 20 chars
    feed_bytes(&mut t, b"CCCCCCCCCCCCCCCCCCCC"); // 20 chars on row 2

    // Shrink to 10 cols - each line becomes 2 rows = 6 rows needed, only 3 available
    t.resize(10, 3);
    // Overflow should go to scrollback
    assert!(!t.scrollback.is_empty(), "overflow must push to scrollback");
    // Grid should show the last 3 rows of the reflowed content
    let visible: Vec<String> = (0..3).map(|r| row(&t, r)).collect();
    // One of the visible rows should contain 'C's
    assert!(
        visible.iter().any(|r| r.contains('C')),
        "last content line must be visible"
    );
}

// ── Basic reflow on expand ──────────────────────────────────

#[test]
fn expand_rejoins_wrapped_lines() {
    let mut t = TerminalState::new(10, 5, 100);
    // This will soft-wrap at col 10
    feed_bytes(&mut t, b"ABCDEFGHIJKLMNOPQRST"); // 20 chars across 2 rows
    assert_eq!(row(&t, 0), "ABCDEFGHIJ");
    assert_eq!(row(&t, 1), "KLMNOPQRST");
    // Expand to 20
    t.resize(20, 5);
    // Should rejoin into single row
    assert_eq!(row(&t, 0), "ABCDEFGHIJKLMNOPQRST");
    assert_eq!(row(&t, 1), ""); // second row should be empty
}

#[test]
fn expand_does_not_rejoin_hard_newlines() {
    let mut t = TerminalState::new(10, 5, 100);
    feed_bytes(&mut t, b"Hello\r\nWorld\r\n");
    // Expand to 20
    t.resize(20, 5);
    // Lines separated by hard newline should remain separate
    assert_eq!(row(&t, 0), "Hello");
    assert_eq!(row(&t, 1), "World");
}

// ── Cursor tracking through reflow ──────────────────────────

#[test]
fn cursor_tracks_through_shrink() {
    let mut t = TerminalState::new(20, 5, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJKLMNOPQRST"); // 20 chars, cursor at col 19 (wrap pending)
    // Shrink to 10: "ABCDEFGHIJ" + "KLMNOPQRST"
    // Cursor was at col 19 in old grid = position in 2nd reflow row
    t.resize(10, 5);
    // Cursor should be on the row containing the last character
    assert!(t.cursor.row <= 1, "cursor must be within reflowed content");
}

#[test]
fn cursor_tracks_through_expand() {
    let mut t = TerminalState::new(10, 5, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJ"); // fill row 0
    feed_bytes(&mut t, b"KLMNO"); // row 1, col 5 after
    assert_eq!(t.cursor.row, 1);
    assert_eq!(t.cursor.col, 5);

    // Expand to 20: joined line "ABCDEFGHIJKLMNO" (15 chars)
    // Cursor was 1 past last content char, clamped to last char index (14) during reflow
    t.resize(20, 5);
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 14);
}

#[test]
fn cursor_at_origin_stays_at_origin() {
    let mut t = TerminalState::new(20, 5, 100);
    feed_bytes(&mut t, b"\x1b[H"); // ensure cursor at origin
    t.resize(10, 5);
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
    t.resize(40, 10);
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

// ── Wide characters during reflow ───────────────────────────

#[test]
fn wide_char_reflow_on_shrink() {
    let mut t = TerminalState::new(10, 5, 100);
    // "AB漢CD" = A(1) B(1) 漢(2) C(1) D(1) = 6 columns
    feed_bytes(&mut t, "AB漢CD".as_bytes());
    assert_eq!(row(&t, 0), "AB漢CD");
    // Shrink to 4: "AB漢" needs 4 cols (A=1, B=1, 漢=2), "CD" on next row
    t.resize(4, 5);
    assert_eq!(row(&t, 0), "AB漢");
    assert_eq!(row(&t, 1), "CD");
}

#[test]
fn wide_char_at_reflow_boundary_wraps_correctly() {
    let mut t = TerminalState::new(10, 5, 100);
    // Fill exactly: "AAAA漢" = 4 + 2 = 6 columns
    feed_bytes(&mut t, "AAAA漢".as_bytes());
    // Shrink to 5: "AAAA" fills 4 cols, wide char needs 2 but only 1 left
    // So wide char wraps to next row
    t.resize(5, 5);
    assert_eq!(row(&t, 0), "AAAA");
    assert_eq!(row(&t, 1), "漢");
}

#[test]
fn wide_char_expand_rejoins() {
    let mut t = TerminalState::new(4, 5, 100);
    // "漢字" = 2+2 = 4 columns, exactly fits
    feed_bytes(&mut t, "漢字".as_bytes());
    assert_eq!(row(&t, 0), "漢字");
    // Trigger wrap
    feed_bytes(&mut t, "AB".as_bytes());
    assert_eq!(row(&t, 1), "AB");

    // Expand to 8: should rejoin "漢字AB" on one row
    t.resize(8, 5);
    assert_eq!(row(&t, 0), "漢字AB");
}

// ── Reflow round-trip ───────────────────────────────────────

#[test]
fn shrink_expand_roundtrip_preserves_text() {
    let mut t = TerminalState::new(40, 5, 100);
    feed_bytes(&mut t, b"The quick brown fox jumps over the lazy");
    // Shrink
    t.resize(20, 5);
    // Expand back
    t.resize(40, 5);
    // Content should be preserved (might be on different row due to scrollback overflow)
    let mut all_text = String::new();
    for i in 0..t.scrollback.len() {
        all_text.push_str(&t.scrollback.get_text(i).unwrap());
    }
    for r in 0..5 {
        all_text.push_str(&row(&t, r));
    }
    assert!(
        all_text.contains("The quick brown fox jumps over the lazy"),
        "text must survive shrink-expand roundtrip"
    );
}

#[test]
fn multiple_resize_cycles() {
    let mut t = TerminalState::new(40, 10, 500);
    feed_bytes(&mut t, b"Line one content\r\n");
    feed_bytes(&mut t, b"Line two content\r\n");
    feed_bytes(&mut t, b"Line three content\r\n");

    // Multiple resize cycles
    let sizes: [(u16, u16); 6] = [(20, 5), (80, 24), (10, 3), (40, 10), (15, 8), (40, 10)];
    for (w, h) in sizes {
        t.resize(w, h);
        // Cursor must always be in bounds
        assert!(
            t.cursor.row < h,
            "cursor row {} >= height {} after resize to {}x{}",
            t.cursor.row,
            h,
            w,
            h
        );
        assert!(
            t.cursor.col < w,
            "cursor col {} >= width {} after resize to {}x{}",
            t.cursor.col,
            w,
            w,
            h
        );
    }

    // Content should still be recoverable
    let mut all = String::new();
    for i in 0..t.scrollback.len() {
        all.push_str(&t.scrollback.get_text(i).unwrap());
        all.push('\n');
    }
    for r in 0..t.grid.height() {
        all.push_str(&row(&t, r));
        all.push('\n');
    }
    assert!(
        all.contains("Line one"),
        "content should survive multiple resizes"
    );
}

// ── Reflow with empty grid ──────────────────────────────────

#[test]
fn resize_empty_grid() {
    let mut t = TerminalState::new(80, 24, 100);
    t.resize(40, 12);
    assert_eq!(t.grid.width(), 40);
    assert_eq!(t.grid.height(), 12);
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn resize_same_dimensions_is_noop() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"Hello World");
    let col_before = t.cursor.col;
    let row_before = t.cursor.row;
    t.resize(80, 24);
    assert_eq!(t.cursor.col, col_before);
    assert_eq!(t.cursor.row, row_before);
    assert_eq!(row(&t, 0), "Hello World");
}

// ── Reflow with scrollback interaction ──────────────────────

#[test]
fn reflow_overflow_populates_scrollback() {
    let mut t = TerminalState::new(40, 5, 1000);
    // Fill all 5 rows with 40-char lines
    for i in 0..5 {
        if i < 4 {
            feed_bytes(&mut t, format!("{:0>40}\r\n", i).as_bytes());
        } else {
            feed_bytes(&mut t, format!("{:0>40}", i).as_bytes());
        }
    }
    let sb_before = t.scrollback.len();
    // Shrink width: each 40-char line becomes 4 rows in 10-col grid = 20 rows needed
    t.resize(10, 5);
    // 15 rows overflow to scrollback
    assert!(
        t.scrollback.len() > sb_before,
        "reflow must push overflow into scrollback"
    );
}

#[test]
fn cursor_not_lost_in_scrollback_overflow() {
    let mut t = TerminalState::new(40, 3, 100);
    feed_bytes(&mut t, b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n");
    feed_bytes(&mut t, b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB\r\n");
    feed_bytes(&mut t, b"Cursor here");
    assert_eq!(t.cursor.row, 2);

    // Shrink to 10: each 40-char line = 4 rows, "Cursor here" = 1 row
    // Total = 4+4+1 = 9 rows, only 3 available, 6 overflow
    t.resize(10, 3);
    // Cursor should still be within grid bounds
    assert!(
        t.cursor.row < 3,
        "cursor must stay within grid after overflow"
    );
    // Cursor row should contain our text
    let cursor_row_text = row(&t, t.cursor.row);
    // "Cursor here" is 11 chars, wraps to "Cursor her" + "e" in 10-wide grid
    assert!(
        cursor_row_text.contains("Cursor her"),
        "cursor row must contain cursor-adjacent text"
    );
}
