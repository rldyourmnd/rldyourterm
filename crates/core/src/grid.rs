use crate::error::CoreError;

pub const BLANK_CHAR: char = ' ';
pub const CELL_WIDTH: usize = 8;
pub const CELL_HEIGHT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub attrs: Attrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: BLANK_CHAR,
            attrs: Attrs::default(),
        }
    }
}

#[rustfmt::skip]
pub static ANSI_PALETTE: [u32; 256] = {
    let mut palette = [0u32; 256];

    // Standard 16 colors (0-15)
    palette[0]  = 0x00_000000; // black
    palette[1]  = 0x00_aa0000; // red
    palette[2]  = 0x00_00aa00; // green
    palette[3]  = 0x00_aa5500; // yellow
    palette[4]  = 0x00_0000aa; // blue
    palette[5]  = 0x00_aa00aa; // magenta
    palette[6]  = 0x00_00aaaa; // cyan
    palette[7]  = 0x00_aaaaaa; // white
    palette[8]  = 0x00_555555; // bright black
    palette[9]  = 0x00_ff5555; // bright red
    palette[10] = 0x00_55ff55; // bright green
    palette[11] = 0x00_ffff55; // bright yellow
    palette[12] = 0x00_5555ff; // bright blue
    palette[13] = 0x00_ff55ff; // bright magenta
    palette[14] = 0x00_55ffff; // bright cyan
    palette[15] = 0x00_ffffff; // bright white

    // 216 color cube (16-231): 6x6x6 with levels [0, 95, 135, 175, 215, 255]
    let levels: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let mut i = 16usize;
    let mut ri = 0usize;
    while ri < 6 {
        let mut gi = 0usize;
        while gi < 6 {
            let mut bi = 0usize;
            while bi < 6 {
                palette[i] = ((levels[ri] as u32) << 16)
                    | ((levels[gi] as u32) << 8)
                    | (levels[bi] as u32);
                i += 1;
                bi += 1;
            }
            gi += 1;
        }
        ri += 1;
    }

    // 24-step grayscale ramp (232-255): 8, 18, 28, ..., 238
    let mut g = 0usize;
    while g < 24 {
        let level = (8 + 10 * g) as u32;
        palette[232 + g] = (level << 16) | (level << 8) | level;
        g += 1;
    }

    palette
};

/// Converts a terminal `Color` to a packed RGB `u32` (`0x00RRGGBB`).
/// `default` is used when the color is `Color::Default`.
#[must_use]
pub fn color_to_u32(color: Color, default: (u8, u8, u8)) -> u32 {
    match color {
        Color::Default => {
            ((default.0 as u32) << 16) | ((default.1 as u32) << 8) | (default.2 as u32)
        }
        Color::Indexed(idx) => ANSI_PALETTE[idx as usize],
        Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
    dirty_rows: Vec<bool>,
}

impl Grid {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
            dirty_rows: vec![true; height as usize],
        }
    }

    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn get_char(&self, row: u16, col: u16) -> Result<char, CoreError> {
        let idx = self.index(row, col)?;
        Ok(self.cells[idx].ch)
    }

    pub fn get_cell(&self, row: u16, col: u16) -> Result<&Cell, CoreError> {
        let idx = self.index(row, col)?;
        Ok(&self.cells[idx])
    }

    pub fn row_cells(&self, row: u16) -> Result<&[Cell], CoreError> {
        if row >= self.height {
            return Err(CoreError::InvalidGridPosition {
                row,
                col: 0,
                width: self.width,
                height: self.height,
            });
        }
        if self.width == 0 {
            return Ok(&[]);
        }
        let w = self.width as usize;
        let start = row as usize * w;
        Ok(&self.cells[start..start + w])
    }

    pub fn put_char(
        &mut self,
        row: u16,
        col: u16,
        ch: char,
        attrs: Attrs,
    ) -> Result<(), CoreError> {
        let idx = self.index(row, col)?;
        self.cells[idx] = Cell { ch, attrs };
        self.mark_row_dirty(row);
        Ok(())
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
        self.mark_all_dirty();
    }

    pub fn clear_row(&mut self, row: u16) -> Result<(), CoreError> {
        self.clear_row_range(row, 0, self.width)
    }

    pub fn clear_row_from(&mut self, row: u16, start_col: u16) -> Result<(), CoreError> {
        self.clear_row_range(row, start_col, self.width)
    }

    pub fn clear_row_to_inclusive(&mut self, row: u16, end_col: u16) -> Result<(), CoreError> {
        let end_exclusive = end_col.saturating_add(1).min(self.width);
        self.clear_row_range(row, 0, end_exclusive)
    }

    pub fn row_string(&self, row: u16) -> Result<String, CoreError> {
        if row >= self.height {
            return Err(CoreError::InvalidGridPosition {
                row,
                col: 0,
                width: self.width,
                height: self.height,
            });
        }
        if self.width == 0 {
            return Ok(String::new());
        }

        let width = self.width as usize;
        let start = row as usize * width;
        let end = start + width;
        Ok(self.cells[start..end].iter().map(|cell| cell.ch).collect())
    }

    pub fn scroll_up(&mut self, lines: u16) -> Vec<String> {
        if lines == 0 || self.height == 0 {
            return Vec::new();
        }

        let lines = lines.min(self.height);
        let mut removed = Vec::with_capacity(lines as usize);
        for row in 0..lines {
            removed.push(self.row_string(row).unwrap_or_default());
        }

        if self.width == 0 || lines == self.height {
            self.clear();
            return removed;
        }

        let width = self.width as usize;
        let lines = lines as usize;
        let height = self.height as usize;

        for dst_row in 0..(height - lines) {
            let src_row = dst_row + lines;
            let src_start = src_row * width;
            let dst_start = dst_row * width;
            self.cells
                .copy_within(src_start..(src_start + width), dst_start);
        }

        for row in (height - lines)..height {
            let start = row * width;
            for cell in &mut self.cells[start..(start + width)] {
                *cell = Cell::default();
            }
        }

        self.mark_all_dirty();
        removed
    }

    pub fn scroll_up_region(
        &mut self,
        lines: u16,
        region_top: u16,
        region_bottom: u16,
    ) -> Vec<String> {
        if lines == 0 || self.height == 0 || region_top > region_bottom {
            return Vec::new();
        }
        let region_top = region_top.min(self.height.saturating_sub(1));
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let region_height = region_bottom - region_top + 1;
        let lines = lines.min(region_height);

        let mut removed = Vec::with_capacity(lines as usize);
        for row in region_top..(region_top + lines) {
            removed.push(self.row_string(row).unwrap_or_default());
        }

        if self.width == 0 {
            return removed;
        }

        let width = self.width as usize;
        let lines_usize = lines as usize;
        let top = region_top as usize;
        let bottom = region_bottom as usize;

        for dst_row in top..=(bottom - lines_usize) {
            let src_row = dst_row + lines_usize;
            let src_start = src_row * width;
            let dst_start = dst_row * width;
            self.cells
                .copy_within(src_start..(src_start + width), dst_start);
        }

        for row in (bottom + 1 - lines_usize)..=bottom {
            let start = row * width;
            for cell in &mut self.cells[start..(start + width)] {
                *cell = Cell::default();
            }
        }

        self.mark_all_dirty();
        removed
    }

    pub fn scroll_down_region(&mut self, lines: u16, region_top: u16, region_bottom: u16) {
        if lines == 0 || self.height == 0 || region_top > region_bottom {
            return;
        }
        let region_top = region_top.min(self.height.saturating_sub(1));
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let region_height = region_bottom - region_top + 1;
        let lines = lines.min(region_height);

        if self.width == 0 {
            return;
        }

        let width = self.width as usize;
        let lines_usize = lines as usize;
        let top = region_top as usize;
        let bottom = region_bottom as usize;

        for dst_row in (top..=(bottom - lines_usize)).rev() {
            let src_start = dst_row * width;
            let dst_start = (dst_row + lines_usize) * width;
            self.cells
                .copy_within(src_start..(src_start + width), dst_start);
        }

        for row in top..(top + lines_usize) {
            let start = row * width;
            for cell in &mut self.cells[start..(start + width)] {
                *cell = Cell::default();
            }
        }

        self.mark_all_dirty();
    }

    pub fn insert_lines(&mut self, at_row: u16, count: u16, region_bottom: u16) {
        if count == 0 || self.width == 0 || at_row > region_bottom {
            return;
        }
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let at_row = at_row.min(region_bottom);
        self.scroll_down_region(count, at_row, region_bottom);
    }

    pub fn delete_lines(&mut self, at_row: u16, count: u16, region_bottom: u16) {
        if count == 0 || self.width == 0 || at_row > region_bottom {
            return;
        }
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let at_row = at_row.min(region_bottom);
        let _ = self.scroll_up_region(count, at_row, region_bottom);
    }

    pub fn insert_chars(&mut self, row: u16, at_col: u16, count: u16) {
        if count == 0 || self.width == 0 || row >= self.height || at_col >= self.width {
            return;
        }
        let w = self.width as usize;
        let row_start = row as usize * w;
        let col = at_col as usize;
        let cnt = (count as usize).min(w - col);

        if cnt > 0 {
            let src_start = row_start + col;
            let src_end = row_start + w - cnt;
            self.cells.copy_within(src_start..src_end, src_start + cnt);
        }

        for i in col..(col + cnt) {
            self.cells[row_start + i] = Cell::default();
        }
        self.mark_row_dirty(row);
    }

    pub fn delete_chars(&mut self, row: u16, at_col: u16, count: u16) {
        if count == 0 || self.width == 0 || row >= self.height || at_col >= self.width {
            return;
        }
        let w = self.width as usize;
        let row_start = row as usize * w;
        let col = at_col as usize;
        let cnt = (count as usize).min(w - col);

        let src_start = row_start + col + cnt;
        let dst_start = row_start + col;
        if src_start < row_start + w {
            self.cells.copy_within(src_start..row_start + w, dst_start);
        }

        for i in (w - cnt)..w {
            self.cells[row_start + i] = Cell::default();
        }
        self.mark_row_dirty(row);
    }

    pub fn erase_chars(&mut self, row: u16, at_col: u16, count: u16) {
        if count == 0 || self.width == 0 || row >= self.height || at_col >= self.width {
            return;
        }
        let w = self.width as usize;
        let row_start = row as usize * w;
        let col = at_col as usize;
        let end = (col + count as usize).min(w);

        for i in col..end {
            self.cells[row_start + i] = Cell::default();
        }
        self.mark_row_dirty(row);
    }

    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        let new_size = new_width as usize * new_height as usize;
        let mut new_cells = vec![Cell::default(); new_size];

        let copy_rows = (self.height as usize).min(new_height as usize);
        let copy_cols = (self.width as usize).min(new_width as usize);

        for row in 0..copy_rows {
            let src_start = row * self.width as usize;
            let dst_start = row * new_width as usize;
            new_cells[dst_start..dst_start + copy_cols]
                .copy_from_slice(&self.cells[src_start..src_start + copy_cols]);
        }

        self.cells = new_cells;
        self.width = new_width;
        self.height = new_height;
        self.dirty_rows = vec![true; new_height as usize];
    }

    pub fn has_dirty_rows(&self) -> bool {
        self.dirty_rows.iter().any(|&d| d)
    }

    pub fn dirty_rows(&self) -> &[bool] {
        &self.dirty_rows
    }

    pub fn take_dirty_rows(&mut self) -> Vec<u16> {
        let mut rows = Vec::new();
        for (idx, dirty) in self.dirty_rows.iter_mut().enumerate() {
            if *dirty {
                rows.push(idx as u16);
                *dirty = false;
            }
        }
        rows
    }

    pub fn mark_all_dirty(&mut self) {
        for dirty in &mut self.dirty_rows {
            *dirty = true;
        }
    }

    fn clear_row_range(
        &mut self,
        row: u16,
        start_col: u16,
        end_col_exclusive: u16,
    ) -> Result<(), CoreError> {
        if row >= self.height {
            return Err(CoreError::InvalidGridPosition {
                row,
                col: start_col,
                width: self.width,
                height: self.height,
            });
        }
        if self.width == 0 {
            return Ok(());
        }

        let start_col = start_col.min(self.width);
        let end_col_exclusive = end_col_exclusive.min(self.width);
        if start_col >= end_col_exclusive {
            return Ok(());
        }

        let width = self.width as usize;
        let row_start = row as usize * width;
        let start = row_start + start_col as usize;
        let end = row_start + end_col_exclusive as usize;
        for cell in &mut self.cells[start..end] {
            *cell = Cell::default();
        }
        self.mark_row_dirty(row);
        Ok(())
    }

    fn index(&self, row: u16, col: u16) -> Result<usize, CoreError> {
        if self.width == 0 || self.height == 0 {
            return Err(CoreError::InvalidGridSize {
                width: self.width,
                height: self.height,
            });
        }
        if row >= self.height || col >= self.width {
            return Err(CoreError::InvalidGridPosition {
                row,
                col,
                width: self.width,
                height: self.height,
            });
        }
        Ok(row as usize * self.width as usize + col as usize)
    }

    fn mark_row_dirty(&mut self, row: u16) {
        if let Some(slot) = self.dirty_rows.get_mut(row as usize) {
            *slot = true;
        }
    }
}

#[cfg(test)]
mod tests {
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
        let attrs = Attrs {
            fg: Color::Indexed(1),
            bold: true,
            ..Attrs::default()
        };
        grid.put_char(0, 0, 'A', attrs).expect("valid put");
        let cell = grid.get_cell(0, 0).expect("valid get");
        assert_eq!(cell.ch, 'A');
        assert_eq!(cell.attrs.fg, Color::Indexed(1));
        assert!(cell.attrs.bold);
    }

    #[test]
    fn clear_resets_attrs_to_default() {
        let mut grid = Grid::new(2, 2);
        let attrs = Attrs {
            fg: Color::Rgb(255, 0, 0),
            ..Attrs::default()
        };
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
    fn resize_resets_dirty_to_new_height() {
        let mut grid = Grid::new(3, 3);
        grid.take_dirty_rows();
        grid.resize(5, 6);
        assert_eq!(grid.dirty_rows().len(), 6);
        assert!(grid.dirty_rows().iter().all(|&d| d));
    }
}
