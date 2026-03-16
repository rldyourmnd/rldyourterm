// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::scrollback::Scrollback;

use super::{ANSI_PALETTE, Attrs, Cell, Color, Grid};

#[test]
fn scroll_up_returns_removed_rows_and_clears_bottom() {
    let mut grid = Grid::new(3, 3);
    for (idx, ch) in ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i']
        .iter()
        .copied()
        .enumerate()
    {
        let row = (idx / 3) as u16;
        let col = (idx % 3) as u16;
        grid.put_char(row, col, ch, Attrs::default())
            .expect("valid put");
    }

    let removed = grid.scroll_up(1);
    assert_eq!(removed, vec!["abc".to_string()]);
    assert_eq!(grid.row_string(0).expect("row 0"), "def");
    assert_eq!(grid.row_string(1).expect("row 1"), "ghi");
    assert_eq!(grid.row_string(2).expect("row 2"), "   ");
}

#[test]
fn put_char_out_of_bounds_is_error() {
    let mut grid = Grid::new(2, 2);
    let err = grid
        .put_char(10, 1, 'x', Attrs::default())
        .expect_err("must fail");
    let msg = err.to_string();
    assert!(msg.contains("invalid grid position"));
}

#[test]
fn put_char_stores_attrs() {
    let mut grid = Grid::new(4, 2);
    let attrs = Attrs::default().with_fg(Color::Indexed(1)).with_bold();
    grid.put_char(0, 0, 'A', attrs).expect("valid put");
    let cell = grid.get_cell(0, 0).expect("valid get");
    assert_eq!(cell.ch, 'A');
    assert_eq!(cell.attrs.fg, Color::Indexed(1));
    assert!(cell.attrs.bold());
}

#[test]
fn clear_resets_attrs_to_default() {
    let mut grid = Grid::new(2, 2);
    let attrs = Attrs::default().with_fg(Color::Rgb(255, 0, 0));
    grid.put_char(0, 0, 'X', attrs).expect("valid put");
    grid.clear();
    let cell = grid.get_cell(0, 0).expect("valid get");
    assert_eq!(cell.attrs, Attrs::default());
    assert_eq!(cell.ch, ' ');
}

#[test]
fn row_cells_returns_slice() {
    let mut grid = Grid::new(3, 2);
    grid.put_char(0, 1, 'B', Attrs::default())
        .expect("valid put");
    let cells = grid.row_cells(0).expect("valid row");
    assert_eq!(cells.len(), 3);
    assert_eq!(cells[1].ch, 'B');
}

#[test]
fn resize_preserves_content() {
    let mut grid = Grid::new(3, 2);
    grid.put_char(0, 0, 'A', Attrs::default())
        .expect("valid put");
    grid.put_char(1, 2, 'Z', Attrs::default())
        .expect("valid put");

    grid.resize(5, 3);
    assert_eq!(grid.width(), 5);
    assert_eq!(grid.height(), 3);
    assert_eq!(grid.get_char(0, 0).expect("cell"), 'A');
    assert_eq!(grid.get_char(1, 2).expect("cell"), 'Z');
    assert_eq!(grid.get_char(2, 0).expect("cell"), ' ');
}

#[test]
fn resize_smaller_truncates() {
    let mut grid = Grid::new(4, 4);
    grid.put_char(0, 0, 'A', Attrs::default())
        .expect("valid put");
    grid.put_char(3, 3, 'Z', Attrs::default())
        .expect("valid put");

    grid.resize(2, 2);
    assert_eq!(grid.width(), 2);
    assert_eq!(grid.height(), 2);
    assert_eq!(grid.get_char(0, 0).expect("cell"), 'A');
    assert!(grid.get_char(3, 3).is_err());
}

#[test]
fn palette_standard_colors() {
    assert_eq!(ANSI_PALETTE[0], 0x00_000000);
    assert_eq!(ANSI_PALETTE[1], 0x00_aa0000);
    assert_eq!(ANSI_PALETTE[15], 0x00_ffffff);
}

#[test]
fn palette_grayscale_ramp() {
    assert_eq!(ANSI_PALETTE[232], 0x00_080808);
    assert_eq!(ANSI_PALETTE[255], 0x00_eeeeee);
}

#[test]
fn insert_lines_shifts_rows_down() {
    let mut grid = Grid::new(3, 4);
    for row in 0..4u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..3u16 {
            grid.put_char(row, col, ch, Attrs::default())
                .expect("valid put");
        }
    }
    grid.insert_lines(1, 1, 3);
    assert_eq!(grid.row_string(0).expect("row 0"), "AAA");
    assert_eq!(grid.row_string(1).expect("row 1"), "   ");
    assert_eq!(grid.row_string(2).expect("row 2"), "BBB");
    assert_eq!(grid.row_string(3).expect("row 3"), "CCC");
}

#[test]
fn delete_lines_shifts_rows_up() {
    let mut grid = Grid::new(3, 4);
    for row in 0..4u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..3u16 {
            grid.put_char(row, col, ch, Attrs::default())
                .expect("valid put");
        }
    }
    grid.delete_lines(1, 1, 3);
    assert_eq!(grid.row_string(0).expect("row 0"), "AAA");
    assert_eq!(grid.row_string(1).expect("row 1"), "CCC");
    assert_eq!(grid.row_string(2).expect("row 2"), "DDD");
    assert_eq!(grid.row_string(3).expect("row 3"), "   ");
}

#[test]
fn erase_chars_clears_range() {
    let mut grid = Grid::new(5, 1);
    for col in 0..5u16 {
        grid.put_char(0, col, (b'A' + col as u8) as char, Attrs::default())
            .expect("valid put");
    }
    grid.erase_chars(0, 1, 2);
    assert_eq!(grid.row_string(0).expect("row 0"), "A  DE");
}

#[test]
fn insert_chars_shifts_right() {
    let mut grid = Grid::new(5, 1);
    for col in 0..5u16 {
        grid.put_char(0, col, (b'A' + col as u8) as char, Attrs::default())
            .expect("valid put");
    }
    grid.insert_chars(0, 1, 2);
    assert_eq!(grid.row_string(0).expect("row 0"), "A  BC");
}

#[test]
fn delete_chars_shifts_left() {
    let mut grid = Grid::new(5, 1);
    for col in 0..5u16 {
        grid.put_char(0, col, (b'A' + col as u8) as char, Attrs::default())
            .expect("valid put");
    }
    grid.delete_chars(0, 1, 2);
    assert_eq!(grid.row_string(0).expect("row 0"), "ADE  ");
}

#[test]
fn default_cell_has_default_attrs() {
    let cell = Cell::default();
    assert_eq!(cell.ch, ' ');
    assert_eq!(cell.attrs, Attrs::default());
}

#[test]
fn new_grid_starts_all_dirty() {
    let grid = Grid::new(3, 4);
    assert!(grid.has_dirty_rows());
    assert_eq!(grid.dirty_rows().len(), 4);
    assert!(grid.dirty_rows().iter().all(|&d| d));
}

#[test]
fn take_dirty_rows_clears_flags() {
    let mut grid = Grid::new(3, 4);
    let dirty = grid.take_dirty_rows();
    assert_eq!(dirty, vec![0, 1, 2, 3]);
    assert!(!grid.has_dirty_rows());
}

#[test]
fn put_char_marks_only_target_row_dirty() {
    let mut grid = Grid::new(5, 3);
    grid.take_dirty_rows();
    assert!(!grid.has_dirty_rows());

    grid.put_char(1, 2, 'X', Attrs::default())
        .expect("valid put");
    assert!(grid.has_dirty_rows());
    assert!(!grid.dirty_rows()[0]);
    assert!(grid.dirty_rows()[1]);
    assert!(!grid.dirty_rows()[2]);
}

#[test]
fn scroll_up_marks_all_dirty() {
    let mut grid = Grid::new(3, 3);
    grid.take_dirty_rows();
    grid.scroll_up(1);
    assert!(grid.dirty_rows().iter().all(|&d| d));
}

#[test]
fn scroll_up_increments_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.take_dirty_rows();
    assert_eq!(grid.scroll_count(), 0);
    grid.scroll_up(1);
    assert_eq!(grid.scroll_count(), 1);
    grid.scroll_up(2);
    assert_eq!(grid.scroll_count(), 3);
}

#[test]
fn take_dirty_rows_resets_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.scroll_up(3);
    assert_eq!(grid.scroll_count(), 3);
    grid.take_dirty_rows();
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn clear_dirty_rows_resets_flags_without_alloc() {
    let mut grid = Grid::new(10, 5);
    grid.put_char(0, 0, 'A', Attrs::default())
        .expect("put row 0 col 0");
    assert!(grid.has_dirty_rows());
    grid.clear_dirty_rows();
    assert!(!grid.has_dirty_rows());
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn clear_resets_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.scroll_up(2);
    assert_eq!(grid.scroll_count(), 2);
    grid.clear();
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn resize_resets_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.scroll_up(2);
    assert_eq!(grid.scroll_count(), 2);
    grid.resize(10, 8);
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn scroll_up_region_resets_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.scroll_up(2);
    assert_eq!(grid.scroll_count(), 2);
    grid.scroll_up_region(1, 1, 3);
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn scroll_down_region_resets_scroll_count() {
    let mut grid = Grid::new(10, 5);
    grid.scroll_up(2);
    assert_eq!(grid.scroll_count(), 2);
    grid.scroll_down_region(1, 1, 3);
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn scroll_region_full_height_at_top_does_not_underflow() {
    // Regression: scroll_{up,down}_region had usize underflow when
    // lines >= region_height and region_top == 0.
    let attrs = Attrs::default();

    let mut grid = Grid::new(10, 4);
    let _ = grid.put_char(0, 0, 'A', attrs);

    // Scroll entire region (lines == region_height) starting at row 0
    grid.scroll_up_region_discard(4, 0, 3);
    assert_eq!(grid.row_string(0).unwrap_or_default().trim(), "");

    let mut grid2 = Grid::new(10, 4);
    let _ = grid2.put_char(0, 0, 'A', attrs);
    grid2.scroll_down_region(4, 0, 3);
    assert_eq!(grid2.row_string(0).unwrap_or_default().trim(), "");

    let mut grid3 = Grid::new(10, 4);
    let _ = grid3.put_char(0, 0, 'A', attrs);
    let removed = grid3.scroll_up_region(4, 0, 3);
    assert_eq!(removed.len(), 4);
    assert_eq!(grid3.row_string(0).unwrap_or_default().trim(), "");
}

#[test]
fn resize_resets_dirty_to_new_height() {
    let mut grid = Grid::new(3, 3);
    grid.take_dirty_rows();
    grid.resize(5, 6);
    assert_eq!(grid.dirty_rows().len(), 6);
    assert!(grid.dirty_rows().iter().all(|&d| d));
}

// ── Reflow tests ────────────────────────────────────────────

#[test]
fn reflow_shrink_width_wraps_long_line() {
    let mut grid = Grid::new(6, 3);
    // Write "ABCDEF" across row 0
    for (i, ch) in "ABCDEF".chars().enumerate() {
        let _ = grid.put_char(0, i as u16, ch, Attrs::default());
    }
    let mut scrollback = Scrollback::new(100);
    let (cr, cc) = grid.resize_with_reflow(3, 3, 0, 5, &mut scrollback);
    // Row 0 should have "ABC", row 1 should have "DEF" (wrapped)
    assert_eq!(grid.get_char(0, 0).unwrap(), 'A');
    assert_eq!(grid.get_char(0, 2).unwrap(), 'C');
    assert_eq!(grid.get_char(1, 0).unwrap(), 'D');
    assert_eq!(grid.get_char(1, 2).unwrap(), 'F');
    assert!(grid.is_row_wrapped(1));
    assert!(!grid.is_row_wrapped(0));
    // Cursor was at col 5 in width 6 → should be at (1, 2) in width 3
    assert_eq!(cr, 1);
    assert_eq!(cc, 2);
}

#[test]
fn reflow_expand_width_merges_wrapped_rows() {
    let mut grid = Grid::new(3, 4);
    // Row 0: "ABC"
    for (i, ch) in "ABC".chars().enumerate() {
        let _ = grid.put_char(0, i as u16, ch, Attrs::default());
    }
    // Row 1: "DEF" (wrapped continuation)
    for (i, ch) in "DEF".chars().enumerate() {
        let _ = grid.put_char(1, i as u16, ch, Attrs::default());
    }
    grid.set_row_wrapped(1, true);

    let mut scrollback = Scrollback::new(100);
    let (cr, cc) = grid.resize_with_reflow(6, 4, 1, 2, &mut scrollback);
    // Merged into single row: "ABCDEF"
    assert_eq!(grid.get_char(0, 0).unwrap(), 'A');
    assert_eq!(grid.get_char(0, 3).unwrap(), 'D');
    assert_eq!(grid.get_char(0, 5).unwrap(), 'F');
    // Row 1 should be empty
    assert_eq!(grid.get_char(1, 0).unwrap(), ' ');
    assert!(!grid.is_row_wrapped(1));
    // Cursor at (1,2) in old grid → offset 5 in logical line → (0,5) in new
    assert_eq!(cr, 0);
    assert_eq!(cc, 5);
}

#[test]
fn reflow_overflow_pushes_to_scrollback() {
    let mut grid = Grid::new(4, 2);
    // Row 0: "ABCD"
    for (i, ch) in "ABCD".chars().enumerate() {
        let _ = grid.put_char(0, i as u16, ch, Attrs::default());
    }
    // Row 1: "EFGH"
    for (i, ch) in "EFGH".chars().enumerate() {
        let _ = grid.put_char(1, i as u16, ch, Attrs::default());
    }

    let mut scrollback = Scrollback::new(100);
    // Shrink to width 2, height 2 → each 4-char line becomes 2 rows, total 4 rows for 2 slots
    let (_cr, _cc) = grid.resize_with_reflow(2, 2, 1, 3, &mut scrollback);
    // 2 overflow rows pushed to scrollback
    assert_eq!(scrollback.len(), 2);
    // Visible grid should have last 2 rows
    assert_eq!(grid.get_char(0, 0).unwrap(), 'E');
    assert_eq!(grid.get_char(1, 0).unwrap(), 'G');
}

#[test]
fn reflow_preserves_hard_line_breaks() {
    let mut grid = Grid::new(4, 3);
    // Row 0: "AB" (not full width, not wrapped)
    let _ = grid.put_char(0, 0, 'A', Attrs::default());
    let _ = grid.put_char(0, 1, 'B', Attrs::default());
    // Row 1: "CD" (separate logical line - hard break)
    let _ = grid.put_char(1, 0, 'C', Attrs::default());
    let _ = grid.put_char(1, 1, 'D', Attrs::default());
    // wrapped[1] = false (default, hard break)

    let mut scrollback = Scrollback::new(100);
    let (_cr, _cc) = grid.resize_with_reflow(8, 3, 0, 0, &mut scrollback);
    // Two separate logical lines, each on its own row (not merged)
    assert_eq!(grid.get_char(0, 0).unwrap(), 'A');
    assert_eq!(grid.get_char(0, 1).unwrap(), 'B');
    assert_eq!(grid.get_char(0, 2).unwrap(), ' '); // not merged with row 1
    assert_eq!(grid.get_char(1, 0).unwrap(), 'C');
    assert_eq!(grid.get_char(1, 1).unwrap(), 'D');
}

#[test]
fn reflow_same_size_is_noop() {
    let mut grid = Grid::new(5, 3);
    let _ = grid.put_char(0, 0, 'X', Attrs::default());
    let mut scrollback = Scrollback::new(100);
    let (cr, cc) = grid.resize_with_reflow(5, 3, 0, 0, &mut scrollback);
    assert_eq!(cr, 0);
    assert_eq!(cc, 0);
    assert_eq!(grid.get_char(0, 0).unwrap(), 'X');
}

#[test]
fn reflow_wrapped_flag_propagates_through_scroll() {
    let mut grid = Grid::new(3, 3);
    grid.set_row_wrapped(2, true);
    assert!(grid.is_row_wrapped(2));
    // Scroll up should shift wrapped flags
    grid.scroll_up_discard(1);
    // Row 2 → row 1, new row 2 is cleared
    assert!(grid.is_row_wrapped(1));
    assert!(!grid.is_row_wrapped(2));
}

#[test]
fn reflow_cursor_on_continuation_cell_snaps_to_wide_column() {
    let mut grid = Grid::new(10, 2);
    // Place a wide char at (0, 4) with continuation at (0, 5)
    let _ = grid.put_char_with_width(0, 4, '\u{6F22}', Attrs::default(), 2);
    let mut scrollback = Scrollback::new(100);
    // Cursor at col=5 (continuation cell); resize to 8 to trigger reflow
    let (cr, cc) = grid.resize_with_reflow(8, 2, 0, 5, &mut scrollback);
    assert_eq!(cr, 0);
    // Cursor should snap to col 4 (owning wide cell's start position)
    assert_eq!(cc, 4);
}

#[test]
fn reflow_cursor_on_trailing_blank_clamps_after_trim() {
    let mut grid = Grid::new(10, 2);
    let _ = grid.put_char(0, 0, 'A', Attrs::default());
    let _ = grid.put_char(0, 1, 'B', Attrs::default());
    let mut scrollback = Scrollback::new(100);
    // Cursor at col=9 (trailing blank); resize to 5 to trigger reflow
    let (cr, cc) = grid.resize_with_reflow(5, 2, 0, 9, &mut scrollback);
    assert_eq!(cr, 0);
    // After trimming trailing blanks, cursor should clamp within content bounds
    assert!(cc <= 1, "cursor col {cc} should clamp to content end");
}
