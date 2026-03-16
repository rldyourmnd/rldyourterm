// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_core::{Attrs, Color, TerminalState};
use rldyourterm_integration_tests::{feed_bytes, row, term, term_full};

// ── Cursor movement (CSI) ───────────────────────────────────

#[test]
fn cuu_moves_cursor_up() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[12;5H"); // row 11, col 4
    feed_bytes(&mut t, b"\x1b[3A"); // CUU 3 - move up 3
    assert_eq!(t.cursor.row, 8);
    assert_eq!(t.cursor.col, 4); // col unchanged
}

#[test]
fn cud_moves_cursor_down() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[5;10H"); // row 4, col 9
    feed_bytes(&mut t, b"\x1b[7B"); // CUD 7 - move down 7
    assert_eq!(t.cursor.row, 11);
    assert_eq!(t.cursor.col, 9); // col unchanged
}

#[test]
fn cuf_moves_cursor_forward() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[1;1H"); // origin
    feed_bytes(&mut t, b"\x1b[15C"); // CUF 15 - move right 15
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 15);
}

#[test]
fn cub_moves_cursor_backward() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[1;20H"); // row 0, col 19
    feed_bytes(&mut t, b"\x1b[5D"); // CUB 5 - move left 5
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 14);
}

#[test]
fn cup_positions_cursor() {
    let mut t = TerminalState::new(80, 24, 100);
    // CUP with row=10, col=30 (1-indexed)
    feed_bytes(&mut t, b"\x1b[10;30H");
    assert_eq!(t.cursor.row, 9); // 0-indexed
    assert_eq!(t.cursor.col, 29); // 0-indexed

    // CUP to different position
    feed_bytes(&mut t, b"\x1b[1;1H");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);

    // CUP to last row/col
    feed_bytes(&mut t, b"\x1b[24;80H");
    assert_eq!(t.cursor.row, 23);
    assert_eq!(t.cursor.col, 79);
}

#[test]
fn cup_default_params_go_to_origin() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[15;40H"); // move away from origin
    assert_eq!(t.cursor.row, 14);
    assert_eq!(t.cursor.col, 39);
    // CSI H with no params = home (1;1)
    feed_bytes(&mut t, b"\x1b[H");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn cup_clamps_to_grid_bounds() {
    let mut t = TerminalState::new(80, 24, 100);
    // Row and col far exceed grid dimensions
    feed_bytes(&mut t, b"\x1b[9999;9999H");
    assert_eq!(t.cursor.row, 23); // clamped to last row
    assert_eq!(t.cursor.col, 79); // clamped to last col
}

#[test]
fn cursor_save_restore_decsc_decrc() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[8;15H"); // row 7, col 14
    // Set bold pen before save
    feed_bytes(&mut t, b"\x1b[1m");
    feed_bytes(&mut t, b"\x1b7"); // DECSC (ESC 7)
    // Move cursor and change pen
    feed_bytes(&mut t, b"\x1b[1;1H\x1b[0m");
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
    // Restore
    feed_bytes(&mut t, b"\x1b8"); // DECRC (ESC 8)
    assert_eq!(t.cursor.row, 7);
    assert_eq!(t.cursor.col, 14);
    // Pen should be restored - verify by printing and checking attributes
    feed_bytes(&mut t, b"X");
    let cells = t.grid.row_cells(7).unwrap();
    assert!(
        cells[14].attrs.bold(),
        "bold should be restored after DECRC"
    );
}

// ── Erase operations ────────────────────────────────────────

#[test]
fn erase_display_from_cursor_ed0() {
    let mut t = TerminalState::new(80, 24, 100);
    // Fill first 5 rows with text
    for i in 0..5 {
        feed_bytes(&mut t, format!("Row {} content\r\n", i).as_bytes());
    }
    // Position cursor at row 2, col 4
    feed_bytes(&mut t, b"\x1b[3;5H");
    // ED 0: erase from cursor to end of display
    feed_bytes(&mut t, b"\x1b[0J");
    // Row 0 and 1 should be intact
    assert_eq!(row(&t, 0), "Row 0 content");
    assert_eq!(row(&t, 1), "Row 1 content");
    // Row 2: cols 0-3 preserved ("Row "), col 4 onward cleared.
    // row() trims trailing spaces, so "Row " becomes "Row".
    let r2 = row(&t, 2);
    assert!(
        r2.starts_with("Row"),
        "first chars of row 2 should remain: got '{}'",
        r2
    );
    // Verify cells directly: cols 0-3 should have 'R','o','w',' '
    let cells = t.grid.row_cells(2).unwrap();
    assert_eq!(cells[0].ch, 'R');
    assert_eq!(cells[1].ch, 'o');
    assert_eq!(cells[2].ch, 'w');
    // Col 4 onward should be blank
    assert_eq!(cells[4].ch, ' ');
    assert_eq!(cells[5].ch, ' ');
    // Rows 3-4 should be fully cleared
    assert_eq!(row(&t, 3), "");
    assert_eq!(row(&t, 4), "");
}

#[test]
fn erase_display_to_cursor_ed1() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Row {} content\r\n", i).as_bytes());
    }
    // Position cursor at row 2, col 5
    feed_bytes(&mut t, b"\x1b[3;6H");
    // ED 1: erase from start of display to cursor
    feed_bytes(&mut t, b"\x1b[1J");
    // Rows 0-1 should be fully cleared
    assert_eq!(row(&t, 0), "");
    assert_eq!(row(&t, 1), "");
    // Row 2: cols 0-5 cleared (inclusive), remaining may have content
    let r2 = row(&t, 2);
    // First 6 chars should be spaces/blank
    let cells = t.grid.row_cells(2).unwrap();
    for (col, cell) in cells.iter().enumerate().take(6) {
        assert_eq!(cell.ch, ' ', "row 2 col {} should be blank after ED 1", col);
    }
    // Cols after cursor should remain
    assert!(
        r2.contains("content"),
        "text after cursor should survive ED 1"
    );
    // Rows 3-4 should remain intact
    assert_eq!(row(&t, 3), "Row 3 content");
    assert_eq!(row(&t, 4), "Row 4 content");
}

#[test]
fn erase_display_entire_ed2() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Row {} content\r\n", i).as_bytes());
    }
    // ED 2: erase entire display
    feed_bytes(&mut t, b"\x1b[2J");
    for r in 0..24 {
        assert_eq!(row(&t, r), "", "row {} should be cleared after ED 2", r);
    }
}

#[test]
fn erase_in_line_from_cursor_el0() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"Hello World Test");
    // Move to col 6
    feed_bytes(&mut t, b"\x1b[1;7H");
    // EL 0: erase from cursor to end of line
    feed_bytes(&mut t, b"\x1b[0K");
    // row() trims trailing spaces, so "Hello " becomes "Hello"
    assert_eq!(row(&t, 0), "Hello");
    // Verify cells directly: cols 0-5 preserved, col 6+ cleared
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].ch, 'H');
    assert_eq!(cells[4].ch, 'o');
    assert_eq!(cells[5].ch, ' '); // original space between "Hello" and "World"
    assert_eq!(cells[6].ch, ' '); // erased
    assert_eq!(cells[10].ch, ' '); // erased
}

#[test]
fn erase_in_line_to_cursor_el1() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"Hello World Test");
    // Move to col 6
    feed_bytes(&mut t, b"\x1b[1;7H");
    // EL 1: erase from start of line to cursor (inclusive)
    feed_bytes(&mut t, b"\x1b[1K");
    let r = row(&t, 0);
    let cells = t.grid.row_cells(0).unwrap();
    for (col, cell) in cells.iter().enumerate().take(7) {
        assert_eq!(cell.ch, ' ', "col {} should be blank after EL 1", col);
    }
    // Text after cursor position should remain
    assert!(r.contains("orld Test"), "text after cursor should survive");
}

#[test]
fn erase_in_line_entire_el2() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"Hello World Test");
    // Move to middle of line
    feed_bytes(&mut t, b"\x1b[1;5H");
    // EL 2: erase entire line
    feed_bytes(&mut t, b"\x1b[2K");
    assert_eq!(row(&t, 0), "");
    // Cursor should remain at same position
    assert_eq!(t.cursor.col, 4);
}

#[test]
fn erase_characters_ech() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Move to col 3
    feed_bytes(&mut t, b"\x1b[1;4H");
    // ECH 4: erase 4 characters from cursor position
    feed_bytes(&mut t, b"\x1b[4X");
    let cells = t.grid.row_cells(0).unwrap();
    // Cols 0-2: A, B, C preserved
    assert_eq!(cells[0].ch, 'A');
    assert_eq!(cells[1].ch, 'B');
    assert_eq!(cells[2].ch, 'C');
    // Cols 3-6: erased (blank)
    for (col, cell) in cells.iter().enumerate().take(7).skip(3) {
        assert_eq!(cell.ch, ' ', "col {} should be erased by ECH", col);
    }
    // Cols 7-9: H, I, J preserved (ECH does not shift content)
    assert_eq!(cells[7].ch, 'H');
    assert_eq!(cells[8].ch, 'I');
    assert_eq!(cells[9].ch, 'J');
    // Cursor should not move
    assert_eq!(t.cursor.col, 3);
}

// ── Insert/Delete operations ────────────────────────────────

#[test]
fn insert_lines_il() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Move to row 2 (0-indexed: row 1)
    feed_bytes(&mut t, b"\x1b[2;1H");
    // IL 2: insert 2 blank lines at cursor row
    feed_bytes(&mut t, b"\x1b[2L");
    // Row 0 should remain "Line 0"
    assert_eq!(row(&t, 0), "Line 0");
    // Rows 1-2 should be blank (newly inserted)
    assert_eq!(row(&t, 1), "");
    assert_eq!(row(&t, 2), "");
    // Previous row 1 content ("Line 1") shifted down to row 3
    assert_eq!(row(&t, 3), "Line 1");
    assert_eq!(row(&t, 4), "Line 2");
}

#[test]
fn delete_lines_dl() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Move to row 2 (0-indexed: row 1)
    feed_bytes(&mut t, b"\x1b[2;1H");
    // DL 2: delete 2 lines at cursor row
    feed_bytes(&mut t, b"\x1b[2M");
    // Row 0 still "Line 0"
    assert_eq!(row(&t, 0), "Line 0");
    // Previous rows 3-4 (Lines 2,3) shifted up to rows 1-2
    assert_eq!(row(&t, 1), "Line 3");
    assert_eq!(row(&t, 2), "Line 4");
    // Vacated bottom rows should be blank
    assert_eq!(row(&t, 3), "");
}

#[test]
fn insert_characters_ich() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Move to col 3
    feed_bytes(&mut t, b"\x1b[1;4H");
    // ICH 2: insert 2 blank characters at cursor
    feed_bytes(&mut t, b"\x1b[2@");
    // Chars shift right: ABC__DEFGH (J falls off the 80-col boundary)
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].ch, 'A');
    assert_eq!(cells[1].ch, 'B');
    assert_eq!(cells[2].ch, 'C');
    assert_eq!(cells[3].ch, ' '); // inserted blank
    assert_eq!(cells[4].ch, ' '); // inserted blank
    assert_eq!(cells[5].ch, 'D');
    assert_eq!(cells[6].ch, 'E');
    assert_eq!(cells[7].ch, 'F');
}

#[test]
fn delete_characters_dch() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"ABCDEFGHIJ");
    // Move to col 3
    feed_bytes(&mut t, b"\x1b[1;4H");
    // DCH 3: delete 3 characters at cursor
    feed_bytes(&mut t, b"\x1b[3P");
    // ABC + GHI J then blanks at end
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].ch, 'A');
    assert_eq!(cells[1].ch, 'B');
    assert_eq!(cells[2].ch, 'C');
    assert_eq!(cells[3].ch, 'G');
    assert_eq!(cells[4].ch, 'H');
    assert_eq!(cells[5].ch, 'I');
    assert_eq!(cells[6].ch, 'J');
    // Vacated end should be blank
    assert_eq!(cells[7].ch, ' ');
}

// ── Scroll operations ───────────────────────────────────────

#[test]
fn scroll_up_su() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Move cursor to safe position
    feed_bytes(&mut t, b"\x1b[1;1H");
    // SU 2: scroll up 2 lines (content moves up, blank lines appear at bottom)
    feed_bytes(&mut t, b"\x1b[2S");
    // Lines 0-1 scrolled off, Line 2 is now at row 0
    assert_eq!(row(&t, 0), "Line 2");
    assert_eq!(row(&t, 1), "Line 3");
    assert_eq!(row(&t, 2), "Line 4");
}

#[test]
fn scroll_down_sd() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    feed_bytes(&mut t, b"\x1b[1;1H");
    // SD 2: scroll down 2 lines (content moves down, blank lines appear at top)
    feed_bytes(&mut t, b"\x1b[2T");
    // Rows 0-1 should be blank
    assert_eq!(row(&t, 0), "");
    assert_eq!(row(&t, 1), "");
    // Previous row 0 content is now at row 2
    assert_eq!(row(&t, 2), "Line 0");
    assert_eq!(row(&t, 3), "Line 1");
}

#[test]
fn scroll_up_within_region() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..10 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Set scroll region to rows 3-7 (1-indexed)
    feed_bytes(&mut t, b"\x1b[3;7r");
    // SU 2 within region
    feed_bytes(&mut t, b"\x1b[2S");
    // Rows outside region (0, 1) should be unchanged
    assert_eq!(row(&t, 0), "Line 0");
    assert_eq!(row(&t, 1), "Line 1");
    // Inside region: rows 2-4 shift up by 2
    assert_eq!(row(&t, 2), "Line 4");
    assert_eq!(row(&t, 3), "Line 5");
    assert_eq!(row(&t, 4), "Line 6");
    // Bottom of region should be blank after scroll
    assert_eq!(row(&t, 5), "");
    assert_eq!(row(&t, 6), "");
    // Rows below region unchanged
    assert_eq!(row(&t, 7), "Line 7");
}

#[test]
fn scroll_down_within_region() {
    let mut t = TerminalState::new(80, 24, 100);
    for i in 0..10 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Set scroll region to rows 3-7 (1-indexed)
    feed_bytes(&mut t, b"\x1b[3;7r");
    // SD 2 within region
    feed_bytes(&mut t, b"\x1b[2T");
    // Rows outside region unchanged
    assert_eq!(row(&t, 0), "Line 0");
    assert_eq!(row(&t, 1), "Line 1");
    // Top of region should have blank rows from scroll down
    assert_eq!(row(&t, 2), "");
    assert_eq!(row(&t, 3), "");
    // Previous content shifts down within region
    assert_eq!(row(&t, 4), "Line 2");
    assert_eq!(row(&t, 5), "Line 3");
    assert_eq!(row(&t, 6), "Line 4");
    // Rows below region unchanged
    assert_eq!(row(&t, 7), "Line 7");
}

// ── SGR attributes ──────────────────────────────────────────

#[test]
fn sgr_all_basic_colors() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set each foreground color 30-37 and write a char
    for code in 30..=37 {
        feed_bytes(&mut t, format!("\x1b[{}mX", code).as_bytes());
    }
    let cells = t.grid.row_cells(0).unwrap();
    for (i, code) in (30..=37).enumerate() {
        assert_eq!(
            cells[i].attrs.fg,
            Color::Indexed((code - 30) as u8),
            "fg color for SGR {} should be Indexed({})",
            code,
            code - 30
        );
    }

    // Background colors 40-47
    feed_bytes(&mut t, b"\x1b[0m\r\n");
    for code in 40..=47 {
        feed_bytes(&mut t, format!("\x1b[{}mY", code).as_bytes());
    }
    let cells = t.grid.row_cells(1).unwrap();
    for (i, code) in (40..=47).enumerate() {
        assert_eq!(
            cells[i].attrs.bg,
            Color::Indexed((code - 40) as u8),
            "bg color for SGR {} should be Indexed({})",
            code,
            code - 40
        );
    }
}

#[test]
fn sgr_256_color_index() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set fg to color index 196 (bright red in 256 palette)
    feed_bytes(&mut t, b"\x1b[38;5;196mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].attrs.fg, Color::Indexed(196));

    // Set bg to color index 45
    feed_bytes(&mut t, b"\x1b[48;5;45mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[1].attrs.bg, Color::Indexed(45));
}

#[test]
fn sgr_rgb_color() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set fg to RGB(100, 200, 50)
    feed_bytes(&mut t, b"\x1b[38;2;100;200;50mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].attrs.fg, Color::Rgb(100, 200, 50));

    // Set bg to RGB(10, 20, 30)
    feed_bytes(&mut t, b"\x1b[48;2;10;20;30mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[1].attrs.bg, Color::Rgb(10, 20, 30));
}

#[test]
fn sgr_reset_clears_all() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set multiple attributes
    feed_bytes(&mut t, b"\x1b[1;3;4;7;38;2;255;0;0mStyled");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.bold());
    assert!(cells[0].attrs.italic());
    assert!(cells[0].attrs.underline());
    assert!(cells[0].attrs.inverse());
    assert_eq!(cells[0].attrs.fg, Color::Rgb(255, 0, 0));

    // SGR 0: reset all
    feed_bytes(&mut t, b"\x1b[0mPlain");
    let cells = t.grid.row_cells(0).unwrap();
    let plain_start = 6; // "Styled" is 6 chars
    assert_eq!(cells[plain_start].attrs, Attrs::default());
}

#[test]
fn sgr_combined_attributes() {
    let mut t = TerminalState::new(80, 24, 100);
    // Bold + italic + underline + strikethrough in one sequence
    feed_bytes(&mut t, b"\x1b[1;3;4;9mX");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.bold());
    assert!(cells[0].attrs.italic());
    assert!(cells[0].attrs.underline());
    assert!(cells[0].attrs.strikethrough());
    assert!(!cells[0].attrs.dim()); // not set
    assert!(!cells[0].attrs.inverse()); // not set
}

#[test]
fn sgr_underline_color() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set underline + underline color (SGR 58;2;R;G;B)
    feed_bytes(&mut t, b"\x1b[4;58;2;0;128;255mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.underline());
    assert_eq!(cells[0].attrs.underline_color, Color::Rgb(0, 128, 255));

    // Reset underline color (SGR 59)
    feed_bytes(&mut t, b"\x1b[59mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[1].attrs.underline_color, Color::Default);
    // Underline itself should still be active
    assert!(cells[1].attrs.underline());
}

#[test]
fn sgr_bright_colors() {
    let mut t = TerminalState::new(80, 24, 100);
    // Bright foreground colors 90-97
    for code in 90..=97 {
        feed_bytes(&mut t, format!("\x1b[{}mX", code).as_bytes());
    }
    let cells = t.grid.row_cells(0).unwrap();
    for (i, code) in (90..=97).enumerate() {
        assert_eq!(
            cells[i].attrs.fg,
            Color::Indexed((code - 90 + 8) as u8),
            "bright fg SGR {} should map to Indexed({})",
            code,
            code - 90 + 8
        );
    }

    // Bright background colors 100-107
    feed_bytes(&mut t, b"\x1b[0m\r\n");
    for code in 100..=107 {
        feed_bytes(&mut t, format!("\x1b[{}mY", code).as_bytes());
    }
    let cells = t.grid.row_cells(1).unwrap();
    for (i, code) in (100..=107).enumerate() {
        assert_eq!(
            cells[i].attrs.bg,
            Color::Indexed((code - 100 + 8) as u8),
            "bright bg SGR {} should map to Indexed({})",
            code,
            code - 100 + 8
        );
    }
}

#[test]
fn sgr_individual_attribute_resets() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set all attributes
    feed_bytes(&mut t, b"\x1b[1;2;3;4;5;7;8;9m");

    // SGR 22: reset bold and dim
    feed_bytes(&mut t, b"\x1b[22mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(!cells[0].attrs.bold(), "bold should be reset by SGR 22");
    assert!(!cells[0].attrs.dim(), "dim should be reset by SGR 22");
    assert!(cells[0].attrs.italic(), "italic should remain after SGR 22");
    assert!(
        cells[0].attrs.underline(),
        "underline should remain after SGR 22"
    );

    // SGR 23: reset italic
    feed_bytes(&mut t, b"\x1b[23mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(!cells[1].attrs.italic(), "italic should be reset by SGR 23");

    // SGR 24: reset underline
    feed_bytes(&mut t, b"\x1b[24mC");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(
        !cells[2].attrs.underline(),
        "underline should be reset by SGR 24"
    );

    // SGR 27: reset inverse
    feed_bytes(&mut t, b"\x1b[27mD");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(
        !cells[3].attrs.inverse(),
        "inverse should be reset by SGR 27"
    );

    // SGR 29: reset strikethrough
    feed_bytes(&mut t, b"\x1b[29mE");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(
        !cells[4].attrs.strikethrough(),
        "strikethrough should be reset by SGR 29"
    );
}

#[test]
fn sgr_double_underline_and_overline() {
    let mut t = TerminalState::new(80, 24, 100);
    // SGR 21: double underline (also resets single underline)
    feed_bytes(&mut t, b"\x1b[4m"); // single underline
    feed_bytes(&mut t, b"\x1b[21mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.double_underline());
    assert!(
        !cells[0].attrs.underline(),
        "single underline cleared by SGR 21"
    );

    // SGR 53: overline
    feed_bytes(&mut t, b"\x1b[53mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[1].attrs.overline());

    // SGR 55: reset overline
    feed_bytes(&mut t, b"\x1b[55mC");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(
        !cells[2].attrs.overline(),
        "overline should be reset by SGR 55"
    );
}

// ── OSC sequences ───────────────────────────────────────────

#[test]
fn osc_0_sets_window_title() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]0;Test Terminal Title\x07");
    assert_eq!(t.window_title(), "Test Terminal Title");
}

#[test]
fn osc_2_sets_window_title() {
    let mut t = term();
    // OSC 2 should also set window title
    feed_bytes(&mut t, b"\x1b]2;Another Title\x07");
    assert_eq!(t.window_title(), "Another Title");
    // Verify it overwrites previous OSC 0 title
    feed_bytes(&mut t, b"\x1b]0;First\x07");
    assert_eq!(t.window_title(), "First");
    feed_bytes(&mut t, b"\x1b]2;Second\x07");
    assert_eq!(t.window_title(), "Second");
}

#[test]
fn osc_52_clipboard_bel_and_st_terminator() {
    let mut t = term();
    // Base64 for "Hello" is SGVsbG8=
    feed_bytes(&mut t, b"\x1b]52;c;SGVsbG8=\x07");
    let clip = t.take_pending_clipboard();
    assert_eq!(clip, Some(('c', "SGVsbG8=".to_string())));
    // Should be consumed after take
    assert_eq!(t.take_pending_clipboard(), None);

    // Test with ST terminator instead of BEL
    feed_bytes(&mut t, b"\x1b]52;p;dGVzdA==\x1b\\");
    let clip = t.take_pending_clipboard();
    assert_eq!(clip, Some(('p', "dGVzdA==".to_string())));
}

#[test]
fn osc_7_current_directory() {
    let mut t = term();
    feed_bytes(&mut t, b"\x1b]7;file://localhost/tmp/test\x07");
    assert_eq!(t.cwd(), "/tmp/test");

    // Update to a different directory
    feed_bytes(&mut t, b"\x1b]7;file://host/home/user\x07");
    assert_eq!(t.cwd(), "/home/user");
}

// ── DECSTBM / scroll region ────────────────────────────────

#[test]
fn decstbm_sets_scroll_region() {
    let mut t = TerminalState::new(80, 24, 100);
    // Fill rows with identifiable content
    for i in 0..10 {
        feed_bytes(&mut t, format!("R{}\r\n", i).as_bytes());
    }
    // Set scroll region to rows 3-8 (1-indexed)
    feed_bytes(&mut t, b"\x1b[3;8r");
    // Cursor should be homed by DECSTBM
    assert_eq!(t.cursor.row, 0);
    assert_eq!(t.cursor.col, 0);
    // Move cursor into the region and fill it with newlines to trigger scrolling
    feed_bytes(&mut t, b"\x1b[8;1H");
    for _ in 0..3 {
        feed_bytes(&mut t, b"\n");
    }
    // Rows outside region (0-1, 8-23) should be unaffected
    assert_eq!(row(&t, 0), "R0");
    assert_eq!(row(&t, 1), "R1");
    assert_eq!(row(&t, 8), "R8");
}

#[test]
fn decstbm_reset_clears_region() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set a scroll region
    feed_bytes(&mut t, b"\x1b[5;15r");
    // Reset with no params (CSI r)
    feed_bytes(&mut t, b"\x1b[r");
    // Fill the terminal beyond its height to verify full-screen scrolling.
    // Each "Line N\r\n" occupies one row. On a 24-row grid, 30 lines means
    // 30 - 24 = 6 newlines trigger scrolls, plus the final \r\n after Line 29
    // also scrolls. Row 0 ends up with "Line 7".
    for i in 0..30 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // Verify full-screen scrolling works (Line 0 is no longer visible)
    assert_ne!(
        row(&t, 0),
        "Line 0",
        "Line 0 should have scrolled off after reset"
    );
    // Row 0 should contain some later line - the important invariant
    // is that scrolling operates on the full screen, not a sub-region
    assert!(
        row(&t, 0).starts_with("Line "),
        "row 0 should contain scrolled line content"
    );
}

#[test]
fn decstbm_cursor_stays_in_region() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set scroll region to rows 5-10 (1-indexed), i.e. 0-indexed rows 4-9
    feed_bytes(&mut t, b"\x1b[5;10r");
    // Move cursor to bottom of the scroll region
    feed_bytes(&mut t, b"\x1b[10;1H"); // row 9 (0-indexed)
    // Write enough lines to trigger scrolling within region
    for i in 0..5 {
        feed_bytes(&mut t, format!("SR{}\r\n", i).as_bytes());
    }
    // Cursor should remain within or at the bottom of the scroll region
    assert!(
        t.cursor.row <= 9,
        "cursor row {} should not exceed scroll region bottom (9)",
        t.cursor.row
    );
    // Content above and below the region should be empty (never written)
    assert_eq!(row(&t, 0), "");
    assert_eq!(row(&t, 1), "");
    assert_eq!(row(&t, 10), "");
}

// ── Additional cursor movement sequences ────────────────────

#[test]
fn cursor_horizontal_absolute_cha() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[5;20H"); // row 4, col 19
    // CHA: CSI G - move to column (1-indexed)
    feed_bytes(&mut t, b"\x1b[35G");
    assert_eq!(t.cursor.row, 4); // row unchanged
    assert_eq!(t.cursor.col, 34); // col set to 34 (0-indexed)
}

#[test]
fn cursor_next_line_cnl() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[5;20H"); // row 4, col 19
    // CNL: CSI E - cursor next line (moves down N lines, to col 0)
    feed_bytes(&mut t, b"\x1b[3E");
    assert_eq!(t.cursor.row, 7);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn cursor_previous_line_cpl() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[10;20H"); // row 9, col 19
    // CPL: CSI F - cursor previous line (moves up N lines, to col 0)
    feed_bytes(&mut t, b"\x1b[4F");
    assert_eq!(t.cursor.row, 5);
    assert_eq!(t.cursor.col, 0);
}

#[test]
fn vertical_position_absolute_vpa() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[5;20H"); // row 4, col 19
    // VPA: CSI d - set row absolutely (1-indexed), col unchanged
    feed_bytes(&mut t, b"\x1b[15d");
    assert_eq!(t.cursor.row, 14);
    assert_eq!(t.cursor.col, 19); // col unchanged
}

// ── Reverse index ───────────────────────────────────────────

#[test]
fn reverse_index_at_top_scrolls_down() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"First line\r\n");
    feed_bytes(&mut t, b"Second line");
    // Move to top row
    feed_bytes(&mut t, b"\x1b[1;1H");
    assert_eq!(t.cursor.row, 0);
    // RI (ESC M): reverse index - at top of screen, should scroll content down
    feed_bytes(&mut t, b"\x1bM");
    assert_eq!(t.cursor.row, 0); // cursor stays at row 0
    // Row 0 should now be blank (new line scrolled in from above)
    assert_eq!(row(&t, 0), "");
    // Previous row 0 content should be at row 1
    assert_eq!(row(&t, 1), "First line");
    assert_eq!(row(&t, 2), "Second line");
}

#[test]
fn reverse_index_mid_screen_moves_up() {
    let mut t = TerminalState::new(80, 24, 100);
    feed_bytes(&mut t, b"\x1b[5;10H"); // row 4, col 9
    feed_bytes(&mut t, b"\x1bM"); // RI
    assert_eq!(t.cursor.row, 3); // moved up one row
    assert_eq!(t.cursor.col, 9); // col unchanged
}

// ── Scrollback interaction ──────────────────────────────────

#[test]
fn scroll_up_pushes_to_scrollback() {
    let mut t = term_full(80, 5, 100);
    for i in 0..5 {
        feed_bytes(&mut t, format!("Line {}\r\n", i).as_bytes());
    }
    // After 5 lines + 5 newlines on a 5-row terminal, line 0 is in scrollback
    assert!(
        !t.scrollback.is_empty(),
        "scrollback should have received lines from overflow"
    );
    let first = t.scrollback.get(0);
    assert!(first.is_some(), "first scrollback entry should exist");
}

// ── Edge case: default SGR foreground/background reset ──────

#[test]
fn sgr_default_fg_bg_reset() {
    let mut t = TerminalState::new(80, 24, 100);
    // Set fg and bg
    feed_bytes(&mut t, b"\x1b[31;42m");
    // SGR 39: reset fg to default
    feed_bytes(&mut t, b"\x1b[39mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[0].attrs.fg, Color::Default);
    assert_eq!(cells[0].attrs.bg, Color::Indexed(2)); // bg still green

    // SGR 49: reset bg to default
    feed_bytes(&mut t, b"\x1b[49mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert_eq!(cells[1].attrs.bg, Color::Default);
}

// ── Blink and hidden attributes ─────────────────────────────

#[test]
fn sgr_blink_and_hidden() {
    let mut t = TerminalState::new(80, 24, 100);
    // SGR 5: slow blink
    feed_bytes(&mut t, b"\x1b[5mA");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[0].attrs.blink());

    // SGR 6: rapid blink (treated same as slow blink)
    feed_bytes(&mut t, b"\x1b[6mB");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[1].attrs.blink());

    // SGR 25: reset blink
    feed_bytes(&mut t, b"\x1b[25mC");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(!cells[2].attrs.blink());

    // SGR 8: hidden
    feed_bytes(&mut t, b"\x1b[8mD");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(cells[3].attrs.hidden());

    // SGR 28: reset hidden
    feed_bytes(&mut t, b"\x1b[28mE");
    let cells = t.grid.row_cells(0).unwrap();
    assert!(!cells[4].attrs.hidden());
}
