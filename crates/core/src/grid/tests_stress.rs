// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::scrollback::Scrollback;

use super::{Attrs, Grid};

// ── Stress tests ─────────────────────────────────────────────

#[test]
fn stress_scroll_up_100k_lines() {
    let mut grid = Grid::new(80, 50);
    for _ in 0..100_000 {
        grid.scroll_up(1);
    }
    assert_eq!(grid.height(), 50);
    assert_eq!(grid.width(), 80);
    let row = grid.row_cells(0).unwrap();
    assert_eq!(row[0].ch, ' ');
}

#[test]
fn stress_write_all_cells_large_grid() {
    let mut grid = Grid::new(200, 100);
    let attrs = Attrs::default();
    for row in 0..100u16 {
        for col in 0..200u16 {
            grid.put_char(row, col, 'A', attrs).unwrap();
        }
    }
    for row in 0..100u16 {
        let cells = grid.row_cells(row).unwrap();
        assert!(cells.iter().take(200).all(|c| c.ch == 'A'));
    }
}

#[test]
fn stress_alternating_write_and_scroll_50k() {
    let mut grid = Grid::new(80, 24);
    let attrs = Attrs::default();
    for i in 0..50_000u32 {
        let ch = char::from(b'A' + (i % 26) as u8);
        for col in 0..80u16 {
            grid.put_char(23, col, ch, attrs).unwrap();
        }
        grid.scroll_up(1);
    }
    assert_eq!(grid.height(), 24);
    assert_eq!(grid.width(), 80);
}

#[test]
fn stress_scroll_count_accumulation_and_reset() {
    let mut grid = Grid::new(80, 24);
    for _ in 0..10_000 {
        grid.scroll_up(1);
    }
    assert_eq!(grid.scroll_count(), 23);
    grid.take_dirty_rows();
    assert_eq!(grid.scroll_count(), 0);
}

#[test]
fn scroll_count_saturates_to_visible_grid_height_minus_one() {
    let mut grid = Grid::new(80, 24);
    for _ in 0..100 {
        grid.scroll_up(3);
    }
    assert_eq!(grid.scroll_count(), 23);
}

#[test]
fn stress_resize_during_active_scroll() {
    let mut grid = Grid::new(80, 24);
    for i in 0..200u16 {
        grid.scroll_up(3);
        let w = 60 + (i % 40);
        let h = 20 + (i % 10);
        grid.resize(w, h);
    }
    assert!(grid.height() > 0);
    assert!(grid.width() > 0);
}

#[test]
fn stress_mixed_region_and_full_scroll() {
    let mut grid = Grid::new(80, 24);
    for _ in 0..1000 {
        grid.scroll_up(2);
        assert_eq!(
            grid.scroll_count(),
            2,
            "scroll_up(2) should set count to exactly 2"
        );
        grid.scroll_up_region(1, 5, 20);
        assert_eq!(grid.scroll_count(), 0);
    }
}

#[test]
fn stress_dirty_tracking_accuracy() {
    let mut grid = Grid::new(80, 50);
    grid.take_dirty_rows();
    let attrs = Attrs::default();
    grid.put_char(10, 0, 'X', attrs).unwrap();
    grid.put_char(30, 5, 'Y', attrs).unwrap();
    let dirty = grid.dirty_rows();
    assert!(dirty[10]);
    assert!(dirty[30]);
    assert!(!dirty[0]);
    assert!(!dirty[20]);
}

#[test]
fn stress_edge_case_single_cell_grid() {
    let mut grid = Grid::new(1, 1);
    let attrs = Attrs::default();
    grid.put_char(0, 0, 'Z', attrs).unwrap();
    assert_eq!(grid.get_cell(0, 0).unwrap().ch, 'Z');
    grid.scroll_up(1);
    assert_eq!(grid.get_cell(0, 0).unwrap().ch, ' ');
    grid.resize(1, 1);
}

#[test]
fn stress_rapid_clear_cycles() {
    let mut grid = Grid::new(80, 24);
    let attrs = Attrs::default();
    for _ in 0..10_000 {
        grid.put_char(0, 0, 'X', attrs).unwrap();
        grid.clear();
        assert_eq!(grid.get_cell(0, 0).unwrap().ch, ' ');
    }
}

// ── Coverage gap tests ─────────────────────────────────────

#[test]
fn zero_size_grid_put_char_returns_error() {
    let mut grid = Grid::new(0, 0);
    let result = grid.put_char(0, 0, 'X', Attrs::default());
    assert!(result.is_err());
}

#[test]
fn zero_width_grid_operations_are_safe() {
    let mut grid = Grid::new(0, 5);
    assert!(grid.is_empty());
    assert_eq!(grid.width(), 0);
    assert_eq!(grid.height(), 5);
    assert!(grid.put_char(0, 0, 'X', Attrs::default()).is_err());
    grid.scroll_up(1);
    let removed = grid.scroll_up_region(1, 0, 4);
    assert!(removed.is_empty() || removed.iter().all(|s| s.is_empty()));
    grid.clear();
    grid.resize(3, 3);
    assert!(!grid.is_empty());
    assert!(grid.put_char(0, 0, 'A', Attrs::default()).is_ok());
}

#[test]
fn zero_height_grid_operations_are_safe() {
    let mut grid = Grid::new(10, 0);
    assert!(grid.is_empty());
    assert!(grid.put_char(0, 0, 'X', Attrs::default()).is_err());
    grid.scroll_up(1);
    grid.scroll_down_region(1, 0, 0);
    grid.clear();
}

#[test]
fn insert_lines_count_exceeds_region_height() {
    let mut grid = Grid::new(4, 5);
    let attrs = Attrs::default();
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..4u16 {
            grid.put_char(row, col, ch, attrs).unwrap();
        }
    }
    grid.insert_lines(1, 100, 3);
    assert_eq!(grid.row_string(0).unwrap(), "AAAA");
    assert_eq!(grid.row_string(1).unwrap(), "    ");
    assert_eq!(grid.row_string(2).unwrap(), "    ");
    assert_eq!(grid.row_string(3).unwrap(), "    ");
    assert_eq!(grid.row_string(4).unwrap(), "EEEE");
}

#[test]
fn delete_lines_count_exceeds_region_height() {
    let mut grid = Grid::new(4, 5);
    let attrs = Attrs::default();
    for row in 0..5u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..4u16 {
            grid.put_char(row, col, ch, attrs).unwrap();
        }
    }
    grid.delete_lines(1, 100, 3);
    assert_eq!(grid.row_string(0).unwrap(), "AAAA");
    assert_eq!(grid.row_string(1).unwrap(), "    ");
    assert_eq!(grid.row_string(2).unwrap(), "    ");
    assert_eq!(grid.row_string(3).unwrap(), "    ");
    assert_eq!(grid.row_string(4).unwrap(), "EEEE");
}

#[test]
fn scroll_up_region_lines_equals_region_height() {
    let mut grid = Grid::new(3, 4);
    let attrs = Attrs::default();
    for row in 0..4u16 {
        let ch = (b'A' + row as u8) as char;
        for col in 0..3u16 {
            grid.put_char(row, col, ch, attrs).unwrap();
        }
    }
    let removed = grid.scroll_up_region(3, 1, 3);
    assert_eq!(removed.len(), 3);
    assert_eq!(removed[0], "BBB");
    assert_eq!(removed[1], "CCC");
    assert_eq!(removed[2], "DDD");
    assert_eq!(grid.row_string(1).unwrap(), "   ");
    assert_eq!(grid.row_string(2).unwrap(), "   ");
    assert_eq!(grid.row_string(3).unwrap(), "   ");
    assert_eq!(grid.row_string(0).unwrap(), "AAA");
}

#[test]
fn row_cells_on_zero_width_grid_returns_empty_slice() {
    let grid = Grid::new(0, 3);
    let cells = grid.row_cells(0).unwrap();
    assert!(cells.is_empty());
}

#[test]
fn resize_from_zero_to_nonzero() {
    let mut grid = Grid::new(0, 0);
    assert!(grid.is_empty());
    grid.resize(10, 5);
    assert_eq!(grid.width(), 10);
    assert_eq!(grid.height(), 5);
    assert!(!grid.is_empty());
    grid.put_char(4, 9, 'Z', Attrs::default()).unwrap();
    assert_eq!(grid.get_char(4, 9).unwrap(), 'Z');
}

// ── Expanded stress tests ──────────────────────────────────

#[test]
fn stress_scroll_region_every_combination() {
    let height: u16 = 10;
    let mut grid = Grid::new(10, height);
    let attrs = Attrs::default();
    // Fill grid with identifiable content
    for row in 0..height {
        for col in 0..10u16 {
            let ch = (b'A' + (row % 26) as u8) as char;
            grid.put_char(row, col, ch, attrs).unwrap();
        }
    }

    // Test scroll_up_region and scroll_down_region with ALL combinations
    // of (top, bottom, lines) for a 10-row grid.
    for top in 0..height {
        for bottom in top..height {
            let region_height = bottom - top + 1;
            for lines in 0..=region_height + 1 {
                // Clone grid state so each iteration starts from known state
                let mut g_up = grid.clone();
                let removed = g_up.scroll_up_region(lines, top, bottom);
                // Should not panic; removed count <= region_height
                assert!(
                    removed.len() <= region_height as usize,
                    "scroll_up_region({lines}, {top}, {bottom}) returned {} rows, \
                     expected <= {region_height}",
                    removed.len()
                );
                // Grid dimensions must remain unchanged
                assert_eq!(g_up.width(), 10);
                assert_eq!(g_up.height(), height);

                let mut g_down = grid.clone();
                g_down.scroll_down_region(lines, top, bottom);
                // Should not panic; dimensions intact
                assert_eq!(g_down.width(), 10);
                assert_eq!(g_down.height(), height);
            }
        }
    }
}

#[test]
fn stress_put_char_with_width_at_every_position() {
    let width: u16 = 20;
    let height: u16 = 10;
    let attrs = Attrs::default();

    // Test width-1 chars at every position
    let mut grid = Grid::new(width, height);
    for row in 0..height {
        for col in 0..width {
            grid.put_char_with_width(row, col, 'X', attrs, 1).unwrap();
        }
    }
    // Verify all cells have width 1 and char 'X'
    for row in 0..height {
        for col in 0..width {
            let cell = grid.get_cell(row, col).unwrap();
            assert_eq!(cell.ch, 'X');
            assert_eq!(cell.width, 1);
        }
    }

    // Test width-2 chars at every position (except last column where continuation
    // would overflow; put_char_with_width handles this by not placing continuation)
    let mut grid = Grid::new(width, height);
    for row in 0..height {
        for col in 0..width {
            grid.put_char_with_width(row, col, '\u{6F22}', attrs, 2)
                .unwrap();
        }
    }
    // Verify grid is consistent: no cell should have width 2 with its continuation
    // extending beyond the grid, and all cells should be accessible without panic
    for row in 0..height {
        for col in 0..width {
            let cell = grid.get_cell(row, col).unwrap();
            // Cell must be one of: width 0 (continuation), width 1 (normal/reset),
            // or width 2 (wide char owning cell)
            assert!(
                cell.width <= 2,
                "row={row} col={col} unexpected width={}",
                cell.width
            );
        }
    }

    // Overwrite wide chars with narrow chars to verify cleanup of continuation cells
    let mut grid = Grid::new(width, height);
    for row in 0..height {
        // Place wide chars at even columns
        let mut col = 0u16;
        while col + 1 < width {
            grid.put_char_with_width(row, col, '\u{6F22}', attrs, 2)
                .unwrap();
            col += 2;
        }
        // Overwrite with narrow chars at every position
        for c in 0..width {
            grid.put_char_with_width(row, c, 'N', attrs, 1).unwrap();
        }
    }
    // All cells should now be narrow 'N' with width 1
    for row in 0..height {
        for col in 0..width {
            let cell = grid.get_cell(row, col).unwrap();
            assert_eq!(cell.ch, 'N', "row={row} col={col}");
            assert_eq!(cell.width, 1, "row={row} col={col}");
        }
    }
}

#[test]
fn stress_reflow_random_widths() {
    let mut grid = Grid::new(40, 20);
    let attrs = Attrs::default();
    // Fill grid with content
    for row in 0..20u16 {
        for col in 0..40u16 {
            let ch = (b'A' + ((row * 40 + col) % 26) as u8) as char;
            grid.put_char(row, col, ch, attrs).unwrap();
        }
    }

    let mut scrollback = Scrollback::new(1000);
    let mut cursor_row = 10u16;
    let mut cursor_col = 20u16;

    // Resize to 20+ different widths
    let widths: Vec<u16> = (1..=20).map(|i| i * 10).collect();
    for &new_width in &widths {
        let (cr, cc) =
            grid.resize_with_reflow(new_width, 20, cursor_row, cursor_col, &mut scrollback);
        // Cursor must stay within grid bounds
        assert!(
            cr < 20,
            "cursor row {cr} out of bounds after reflow to width {new_width}"
        );
        assert!(
            cc < new_width,
            "cursor col {cc} out of bounds after reflow to width {new_width}"
        );
        assert_eq!(grid.width(), new_width);
        assert_eq!(grid.height(), 20);
        cursor_row = cr;
        cursor_col = cc;
    }
}

#[test]
fn stress_wrapped_line_merge_during_reflow() {
    let width: u16 = 20;
    let height: u16 = 30;
    let mut grid = Grid::new(width, height);
    let attrs = Attrs::default();

    // Write 500 chars sequentially, simulating auto-wrap behavior.
    // This creates wrapped lines as content overflows each row.
    let mut row = 0u16;
    let mut col = 0u16;
    let mut char_index = 0u32;
    let total_chars = 500u32;
    while char_index < total_chars && row < height {
        let ch = (b'A' + (char_index % 26) as u8) as char;
        grid.put_char(row, col, ch, attrs).unwrap();
        col += 1;
        char_index += 1;
        if col >= width {
            col = 0;
            if row + 1 < height {
                // Mark next row as wrapped continuation
                grid.set_row_wrapped(row + 1, true);
            }
            row += 1;
        }
    }
    // char_index is the number of chars actually placed
    let chars_placed = char_index;
    // rows_used = ceil(chars_placed / width) but capped by height
    let rows_used = (chars_placed as u16).div_ceil(width).min(height);

    // Resize to width 80 (wider than 20) - should merge wrapped rows
    let mut scrollback = Scrollback::new(1000);
    let (cr, cc) = grid.resize_with_reflow(80, height, row.min(height - 1), col, &mut scrollback);

    // Verify cursor is in bounds
    assert!(cr < height);
    assert!(cc < 80);

    // After expanding to width 80, the 500 chars should fit in fewer rows.
    // With width 80: ceil(chars_placed / 80) rows needed for content.
    let expected_content_rows = (chars_placed as usize).div_ceil(80);

    // Verify all content is preserved by reading back characters
    let mut recovered = String::new();
    for r in 0..height.min(rows_used) {
        if r as usize >= expected_content_rows {
            break;
        }
        if let Ok(row_str) = grid.row_string(r) {
            recovered.push_str(row_str.trim_end());
        }
    }

    // Build the expected string
    let expected: String = (0..chars_placed)
        .map(|i| (b'A' + (i % 26) as u8) as char)
        .collect();
    assert_eq!(
        recovered,
        expected,
        "content should be preserved after reflow (recovered {} chars, expected {})",
        recovered.len(),
        expected.len()
    );
}

#[test]
fn stress_dirty_rows_consistency_after_operations() {
    let width: u16 = 20;
    let height: u16 = 15;
    let mut grid = Grid::new(width, height);
    let attrs = Attrs::default();

    // Clear initial dirty state
    grid.take_dirty_rows();
    assert!(!grid.has_dirty_rows());
    assert_eq!(grid.dirty_rows().len(), height as usize);

    // Perform a mix of operations and verify dirty_rows length stays consistent
    for i in 0..200u32 {
        match i % 5 {
            0 => {
                // put_char at various positions
                let row = (i as u16) % height;
                let col = (i as u16) % width;
                grid.put_char(row, col, 'X', attrs).unwrap();
            }
            1 => {
                // scroll_up
                grid.scroll_up(1);
            }
            2 => {
                // scroll_down_region
                grid.scroll_down_region(1, 2, height - 1);
            }
            3 => {
                // erase_chars
                let row = (i as u16) % height;
                grid.erase_chars(row, 0, width);
            }
            4 => {
                // clear
                grid.clear();
            }
            _ => unreachable!(),
        }

        // After every operation, dirty_rows must have exactly `height` entries
        assert_eq!(
            grid.dirty_rows().len(),
            height as usize,
            "dirty_rows length mismatch after operation {i}"
        );

        // No out-of-bounds access should be possible
        for idx in 0..height as usize {
            let _ = grid.dirty_rows()[idx];
        }

        // Periodically reset dirty state
        if i % 10 == 0 {
            grid.take_dirty_rows();
            assert_eq!(grid.dirty_rows().len(), height as usize);
        }
    }
}
