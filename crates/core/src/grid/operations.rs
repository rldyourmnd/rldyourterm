use crate::error::CoreError;

use super::{Cell, Grid};

impl Grid {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        Self {
            width,
            height,
            cells: vec![Cell::default(); size],
            dirty_rows: vec![true; height as usize],
            scroll_count: 0,
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
        attrs: super::Attrs,
    ) -> Result<(), CoreError> {
        let idx = self.index(row, col)?;
        self.cells[idx] = Cell { ch, attrs };
        self.mark_row_dirty(row);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.cells.fill(Cell::default());
        self.scroll_count = 0;
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
        let mut s = String::with_capacity(width);
        for cell in &self.cells[start..end] {
            s.push(cell.ch);
        }
        Ok(s)
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
            self.cells[start..(start + width)].fill(Cell::default());
        }

        let max_scroll = self.height.saturating_sub(1) as usize;
        self.scroll_count = self.scroll_count.saturating_add(lines).min(max_scroll);
        self.mark_all_dirty();
        removed
    }

    /// Shifts rows up by `lines` and clears the vacated bottom rows.
    /// Unlike `scroll_up`, does not extract row text - the caller is expected
    /// to push row data into scrollback directly via `row_cells` beforehand.
    pub fn scroll_up_discard(&mut self, lines: u16) {
        if lines == 0 || self.height == 0 {
            return;
        }

        let lines = lines.min(self.height);
        if self.width == 0 || lines == self.height {
            self.clear();
            return;
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
            self.cells[start..(start + width)].fill(Cell::default());
        }

        let max_scroll = self.height.saturating_sub(1) as usize;
        self.scroll_count = self.scroll_count.saturating_add(lines).min(max_scroll);
        self.mark_all_dirty();
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
            self.cells[start..(start + width)].fill(Cell::default());
        }

        // Region scroll invalidates the DMA scroll optimization (which assumes
        // a uniform full-screen shift). Reset scroll_count so GPU renderer
        // falls back to the standard dirty-row upload path.
        self.scroll_count = 0;
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
            self.cells[start..(start + width)].fill(Cell::default());
        }

        self.scroll_count = 0;
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

        self.cells[row_start + col..row_start + col + cnt].fill(Cell::default());
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

        self.cells[row_start + w - cnt..row_start + w].fill(Cell::default());
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

        self.cells[row_start + col..row_start + end].fill(Cell::default());
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
        self.scroll_count = 0;
    }

    pub fn has_dirty_rows(&self) -> bool {
        self.dirty_rows.iter().any(|&d| d)
    }

    /// Returns lines scrolled since last `take_dirty_rows`. Used by GPU renderer
    /// to optimize scroll via DMA buffer copy instead of full re-upload.
    pub fn scroll_count(&self) -> usize {
        self.scroll_count
    }

    pub fn dirty_rows(&self) -> &[bool] {
        &self.dirty_rows
    }

    pub fn take_dirty_rows(&mut self) -> Vec<u16> {
        let mut rows = Vec::with_capacity(self.dirty_rows.len());
        for (idx, dirty) in self.dirty_rows.iter_mut().enumerate() {
            if *dirty {
                rows.push(idx as u16);
                *dirty = false;
            }
        }
        self.scroll_count = 0;
        rows
    }

    /// Clear all dirty flags and reset scroll count without allocating.
    /// Use when the renderer has already consumed `dirty_rows()` by reference.
    pub fn clear_dirty_rows(&mut self) {
        self.dirty_rows.fill(false);
        self.scroll_count = 0;
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty_rows.fill(true);
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
        self.cells[start..end].fill(Cell::default());
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
