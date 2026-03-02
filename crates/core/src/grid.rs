use crate::error::CoreError;

pub const BLANK_CHAR: char = ' ';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Self { ch: BLANK_CHAR }
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

    pub fn put_char(&mut self, row: u16, col: u16, ch: char) -> Result<(), CoreError> {
        let idx = self.index(row, col)?;
        self.cells[idx].ch = ch;
        self.mark_row_dirty(row);
        Ok(())
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.ch = BLANK_CHAR;
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
            // Row bounds are constrained by lines <= height, so this cannot fail.
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
                cell.ch = BLANK_CHAR;
            }
        }

        self.mark_all_dirty();
        removed
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
            cell.ch = BLANK_CHAR;
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
    use super::Grid;

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
            grid.put_char(row, col, ch).expect("valid put");
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
        let err = grid.put_char(10, 1, 'x').expect_err("must fail");
        let msg = err.to_string();
        assert!(msg.contains("invalid grid position"));
    }
}
