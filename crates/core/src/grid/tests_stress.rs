// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

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
        assert!(grid.scroll_count() > 0);
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
