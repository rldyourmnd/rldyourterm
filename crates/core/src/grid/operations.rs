// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

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
            wrapped: vec![false; height as usize],
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
        self.put_char_with_width(row, col, ch, attrs, 1)
    }

    pub fn put_char_with_width(
        &mut self,
        row: u16,
        col: u16,
        ch: char,
        attrs: super::Attrs,
        char_width: u8,
    ) -> Result<(), CoreError> {
        let idx = self.index(row, col)?;

        // If overwriting a continuation cell (width=0), clear the owning wide char
        if self.cells[idx].width == 0
            && col > 0
            && let Ok(prev_idx) = self.index(row, col - 1)
            && self.cells[prev_idx].width == 2
        {
            self.cells[prev_idx] = Cell::default();
        }

        // If overwriting the first cell of a wide char, clear its continuation
        if self.cells[idx].width == 2
            && col + 1 < self.width
            && let Ok(next_idx) = self.index(row, col + 1)
            && self.cells[next_idx].width == 0
        {
            self.cells[next_idx] = Cell::default();
        }

        self.cells[idx] = Cell {
            ch,
            attrs,
            width: char_width,
        };

        // Place continuation cell for wide characters
        if char_width == 2
            && col + 1 < self.width
            && let Ok(next_idx) = self.index(row, col + 1)
        {
            // If the continuation overwrites the start of another wide char,
            // clear that wide char's own continuation
            if self.cells[next_idx].width == 2
                && col + 2 < self.width
                && let Ok(nn_idx) = self.index(row, col + 2)
                && self.cells[nn_idx].width == 0
            {
                self.cells[nn_idx] = Cell::default();
            }
            self.cells[next_idx] = Cell {
                ch: ' ',
                attrs,
                width: 0,
            };
        }

        self.mark_row_dirty(row);
        Ok(())
    }

    pub fn is_row_wrapped(&self, row: u16) -> bool {
        self.wrapped.get(row as usize).copied().unwrap_or(false)
    }

    pub fn set_row_wrapped(&mut self, row: u16, val: bool) {
        if let Some(slot) = self.wrapped.get_mut(row as usize) {
            *slot = val;
        }
    }

    pub fn clear(&mut self) {
        self.clear_with_cell(Cell::default());
    }

    pub fn clear_with_cell(&mut self, blank: Cell) {
        self.cells.fill(blank);
        self.wrapped.fill(false);
        self.scroll_count = 0;
        self.mark_all_dirty();
    }

    pub fn clear_row(&mut self, row: u16) -> Result<(), CoreError> {
        self.clear_row_with_cell(row, Cell::default())
    }

    pub fn clear_row_with_cell(&mut self, row: u16, blank: Cell) -> Result<(), CoreError> {
        self.clear_row_range(row, 0, self.width, blank)
    }

    pub fn clear_row_from(&mut self, row: u16, start_col: u16) -> Result<(), CoreError> {
        self.clear_row_from_with_cell(row, start_col, Cell::default())
    }

    pub fn clear_row_from_with_cell(
        &mut self,
        row: u16,
        start_col: u16,
        blank: Cell,
    ) -> Result<(), CoreError> {
        self.clear_row_range(row, start_col, self.width, blank)
    }

    pub fn clear_row_to_inclusive(&mut self, row: u16, end_col: u16) -> Result<(), CoreError> {
        self.clear_row_to_inclusive_with_cell(row, end_col, Cell::default())
    }

    pub fn clear_row_to_inclusive_with_cell(
        &mut self,
        row: u16,
        end_col: u16,
        blank: Cell,
    ) -> Result<(), CoreError> {
        let end_exclusive = end_col.saturating_add(1).min(self.width);
        self.clear_row_range(row, 0, end_exclusive, blank)
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
            if cell.width == 0 {
                continue;
            }
            s.push(cell.ch);
        }
        Ok(s)
    }

    #[cfg(test)]
    pub(crate) fn scroll_up(&mut self, lines: u16) -> Vec<String> {
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
            self.wrapped[dst_row] = self.wrapped[src_row];
        }

        for row in (height - lines)..height {
            let start = row * width;
            self.cells[start..(start + width)].fill(Cell::default());
            self.wrapped[row] = false;
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
        self.scroll_up_discard_with_cell(lines, Cell::default());
    }

    pub fn scroll_up_discard_with_cell(&mut self, lines: u16, blank: Cell) {
        if lines == 0 || self.height == 0 {
            return;
        }

        let lines = lines.min(self.height);
        if self.width == 0 || lines == self.height {
            self.clear_with_cell(blank);
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
            self.wrapped[dst_row] = self.wrapped[src_row];
        }

        for row in (height - lines)..height {
            let start = row * width;
            self.cells[start..(start + width)].fill(blank);
            self.wrapped[row] = false;
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

        if lines < region_height {
            for dst_row in top..=(bottom - lines_usize) {
                let src_row = dst_row + lines_usize;
                let src_start = src_row * width;
                let dst_start = dst_row * width;
                self.cells
                    .copy_within(src_start..(src_start + width), dst_start);
                self.wrapped[dst_row] = self.wrapped[src_row];
            }
        }

        let clear_start = if lines < region_height {
            bottom + 1 - lines_usize
        } else {
            top
        };
        for row in clear_start..=bottom {
            let start = row * width;
            self.cells[start..(start + width)].fill(Cell::default());
            self.wrapped[row] = false;
        }

        // Region scroll invalidates the DMA scroll optimization (which assumes
        // a uniform full-screen shift). Reset scroll_count so GPU renderer
        // falls back to the standard dirty-row upload path.
        self.scroll_count = 0;
        self.mark_all_dirty();
        removed
    }

    pub fn scroll_up_region_discard(&mut self, lines: u16, region_top: u16, region_bottom: u16) {
        self.scroll_up_region_discard_with_cell(lines, region_top, region_bottom, Cell::default());
    }

    pub fn scroll_up_region_discard_with_cell(
        &mut self,
        lines: u16,
        region_top: u16,
        region_bottom: u16,
        blank: Cell,
    ) {
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

        if lines < region_height {
            for dst_row in top..=(bottom - lines_usize) {
                let src_row = dst_row + lines_usize;
                let src_start = src_row * width;
                let dst_start = dst_row * width;
                self.cells
                    .copy_within(src_start..(src_start + width), dst_start);
                self.wrapped[dst_row] = self.wrapped[src_row];
            }
        }

        let clear_start = if lines < region_height {
            bottom + 1 - lines_usize
        } else {
            top
        };
        for row in clear_start..=bottom {
            let start = row * width;
            self.cells[start..(start + width)].fill(blank);
            self.wrapped[row] = false;
        }

        self.scroll_count = 0;
        self.mark_all_dirty();
    }

    pub fn scroll_down_region(&mut self, lines: u16, region_top: u16, region_bottom: u16) {
        self.scroll_down_region_with_cell(lines, region_top, region_bottom, Cell::default());
    }

    pub fn scroll_down_region_with_cell(
        &mut self,
        lines: u16,
        region_top: u16,
        region_bottom: u16,
        blank: Cell,
    ) {
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

        if lines < region_height {
            for dst_row in (top..=(bottom - lines_usize)).rev() {
                let src_start = dst_row * width;
                let dst_start = (dst_row + lines_usize) * width;
                self.cells
                    .copy_within(src_start..(src_start + width), dst_start);
                self.wrapped[dst_row + lines_usize] = self.wrapped[dst_row];
            }
        }

        let clear_end = if lines < region_height {
            top + lines_usize
        } else {
            bottom + 1
        };
        for row in top..clear_end {
            let start = row * width;
            self.cells[start..(start + width)].fill(blank);
            self.wrapped[row] = false;
        }

        self.scroll_count = 0;
        self.mark_all_dirty();
    }

    pub fn insert_lines(&mut self, at_row: u16, count: u16, region_bottom: u16) {
        self.insert_lines_with_cell(at_row, count, region_bottom, Cell::default());
    }

    pub fn insert_lines_with_cell(
        &mut self,
        at_row: u16,
        count: u16,
        region_bottom: u16,
        blank: Cell,
    ) {
        if count == 0 || self.width == 0 || at_row > region_bottom {
            return;
        }
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let at_row = at_row.min(region_bottom);
        self.scroll_down_region_with_cell(count, at_row, region_bottom, blank);
    }

    pub fn delete_lines(&mut self, at_row: u16, count: u16, region_bottom: u16) {
        self.delete_lines_with_cell(at_row, count, region_bottom, Cell::default());
    }

    pub fn delete_lines_with_cell(
        &mut self,
        at_row: u16,
        count: u16,
        region_bottom: u16,
        blank: Cell,
    ) {
        if count == 0 || self.width == 0 || at_row > region_bottom {
            return;
        }
        let region_bottom = region_bottom.min(self.height.saturating_sub(1));
        let at_row = at_row.min(region_bottom);
        self.scroll_up_region_discard_with_cell(count, at_row, region_bottom, blank);
    }

    pub fn insert_chars(&mut self, row: u16, at_col: u16, count: u16) {
        self.insert_chars_with_cell(row, at_col, count, Cell::default());
    }

    pub fn insert_chars_with_cell(&mut self, row: u16, at_col: u16, count: u16, blank: Cell) {
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

        self.cells[row_start + col..row_start + col + cnt].fill(blank);
        self.mark_row_dirty(row);
    }

    pub fn delete_chars(&mut self, row: u16, at_col: u16, count: u16) {
        self.delete_chars_with_cell(row, at_col, count, Cell::default());
    }

    pub fn delete_chars_with_cell(&mut self, row: u16, at_col: u16, count: u16, blank: Cell) {
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

        self.cells[row_start + w - cnt..row_start + w].fill(blank);
        self.mark_row_dirty(row);
    }

    pub fn erase_chars(&mut self, row: u16, at_col: u16, count: u16) {
        self.erase_chars_with_cell(row, at_col, count, Cell::default());
    }

    pub fn erase_chars_with_cell(&mut self, row: u16, at_col: u16, count: u16, blank: Cell) {
        if count == 0 || self.width == 0 || row >= self.height || at_col >= self.width {
            return;
        }
        let w = self.width as usize;
        let row_start = row as usize * w;
        let col = at_col as usize;
        let end = (col + count as usize).min(w);

        self.cells[row_start + col..row_start + end].fill(blank);
        self.mark_row_dirty(row);
    }

    /// Simple resize: copies min(old, new) rows/cols. No reflow.
    /// Used for alternate screen where reflow is not expected (per xterm/VTE).
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
        self.wrapped = vec![false; new_height as usize];
        self.scroll_count = 0;
    }

    /// Resize with reflow: merges soft-wrapped logical lines and re-wraps to new width.
    /// Returns the new (row, col) for the cursor after reflow.
    /// Overflow rows are pushed into `scrollback`.
    pub fn resize_with_reflow(
        &mut self,
        new_width: u16,
        new_height: u16,
        cursor_row: u16,
        cursor_col: u16,
        scrollback: &mut crate::scrollback::Scrollback,
    ) -> (u16, u16) {
        if new_width == 0 || new_height == 0 {
            self.resize(new_width, new_height);
            return (0, 0);
        }
        if new_width == self.width && new_height == self.height {
            return (cursor_row, cursor_col);
        }
        if self.width == 0 || self.height == 0 {
            self.resize(new_width, new_height);
            return (0, 0);
        }

        // Phase 1: Collect logical lines from the current grid.
        // A logical line is a sequence of cells spanning one or more grid rows,
        // where consecutive rows marked as wrapped belong to the same logical line.
        let old_width = self.width as usize;
        let height = self.height as usize;
        let cursor_abs = cursor_row as usize * old_width + cursor_col as usize;

        let mut logical_lines: Vec<Vec<Cell>> = Vec::new();
        let mut cursor_logical_line: usize = 0;
        let mut cursor_offset_in_logical: usize = 0;
        let mut abs_offset: usize = 0;

        let mut row = 0usize;
        while row < height {
            let row_start = row * old_width;
            let mut line_cells: Vec<Cell> = Vec::new();
            line_cells.extend_from_slice(&self.cells[row_start..row_start + old_width]);

            // Merge subsequent wrapped rows into this logical line
            while row + 1 < height && self.wrapped[row + 1] {
                row += 1;
                let next_start = row * old_width;
                line_cells.extend_from_slice(&self.cells[next_start..next_start + old_width]);
            }

            // Track cursor position within logical lines
            let line_end_abs = abs_offset + line_cells.len();
            if cursor_abs >= abs_offset && cursor_abs < line_end_abs {
                cursor_logical_line = logical_lines.len();
                cursor_offset_in_logical = cursor_abs - abs_offset;
            }
            abs_offset = line_end_abs;

            // Trim trailing blank cells to save memory during reflow
            while line_cells
                .last()
                .is_some_and(|c| c.ch == ' ' && c.attrs == super::Attrs::default() && c.width == 1)
            {
                line_cells.pop();
            }

            // Clamp cursor offset to trimmed length so Phase 2 can find it.
            // The cursor may have been on a trailing blank that was trimmed above.
            if cursor_logical_line == logical_lines.len() && !line_cells.is_empty() {
                cursor_offset_in_logical = cursor_offset_in_logical.min(line_cells.len() - 1);
            }

            logical_lines.push(line_cells);
            row += 1;
        }

        // Trim trailing empty logical lines beyond cursor to avoid
        // scrollback pollution from blank rows below the cursor.
        let keep_up_to = cursor_logical_line + 1;
        while logical_lines.len() > keep_up_to && logical_lines.last().is_some_and(Vec::is_empty) {
            logical_lines.pop();
        }

        // Phase 2: Re-wrap each logical line to new_width and fill the new grid.
        let nw = new_width as usize;
        let nh = new_height as usize;
        let new_size = nw * nh;
        let mut new_cells = vec![Cell::default(); new_size];
        let mut new_wrapped = vec![false; nh];
        let mut new_cursor_row: usize = 0;
        let mut new_cursor_col: usize = 0;

        // Collect all re-wrapped rows first (may exceed new_height)
        struct WrappedRow {
            cells: Vec<Cell>,
            wrapped: bool,
        }
        let mut all_rows: Vec<WrappedRow> = Vec::new();

        for (line_idx, line_cells) in logical_lines.iter().enumerate() {
            if line_cells.is_empty() {
                // Preserve empty logical lines as a single blank row
                let cursor_here = line_idx == cursor_logical_line;
                if cursor_here {
                    new_cursor_row = all_rows.len();
                    // Clamp original column to new width
                    new_cursor_col = cursor_offset_in_logical.min(nw.saturating_sub(1));
                }
                all_rows.push(WrappedRow {
                    cells: vec![Cell::default(); nw],
                    wrapped: false,
                });
                continue;
            }

            let mut col: usize = 0;
            let mut current_row_cells = vec![Cell::default(); nw];
            let mut is_first_row_of_line = true;

            for (cell_idx, cell) in line_cells.iter().enumerate() {
                // Track cursor before skipping continuation cells so that a cursor
                // positioned on a continuation slot (col 1 of a wide char) snaps to
                // the owning wide cell's column rather than defaulting to (0, 0).
                if line_idx == cursor_logical_line && cell_idx == cursor_offset_in_logical {
                    new_cursor_row = all_rows.len();
                    // Continuation cells (width=0): snap to owning wide cell's start.
                    // The wide cell was placed at col-2 since it advanced col by its
                    // width of 2. Normal cells use current col directly.
                    new_cursor_col = if cell.width == 0 {
                        col.saturating_sub(2)
                    } else {
                        col
                    };
                }

                // Skip continuation cells (width=0)
                if cell.width == 0 {
                    continue;
                }

                let char_width = cell.width as usize;

                // Wide char that doesn't fit at end of row
                if char_width == 2 && col + 1 >= nw {
                    // Wrap to next row
                    all_rows.push(WrappedRow {
                        cells: current_row_cells,
                        wrapped: !is_first_row_of_line,
                    });
                    current_row_cells = vec![Cell::default(); nw];
                    is_first_row_of_line = false;
                    col = 0;
                }

                // Normal wrap at row boundary
                if col >= nw {
                    all_rows.push(WrappedRow {
                        cells: current_row_cells,
                        wrapped: !is_first_row_of_line,
                    });
                    current_row_cells = vec![Cell::default(); nw];
                    is_first_row_of_line = false;
                    col = 0;
                }

                current_row_cells[col] = *cell;
                if char_width == 2 && col + 1 < nw {
                    current_row_cells[col + 1] = Cell {
                        ch: ' ',
                        attrs: cell.attrs,
                        width: 0,
                    };
                }
                col += char_width;
            }

            // Push the last partial row
            all_rows.push(WrappedRow {
                cells: current_row_cells,
                wrapped: !is_first_row_of_line,
            });
        }

        // Phase 3: If more rows than new_height, push overflow to scrollback.
        let overflow = all_rows.len().saturating_sub(nh);
        for row_data in all_rows.drain(..overflow) {
            scrollback.push_from_cells(&row_data.cells);
        }

        // Adjust cursor row after overflow
        if new_cursor_row < overflow {
            new_cursor_row = 0;
        } else {
            new_cursor_row -= overflow;
        }

        // Phase 4: Place remaining rows into the new grid.
        for (i, row_data) in all_rows.iter().enumerate() {
            if i >= nh {
                break;
            }
            let dst_start = i * nw;
            let copy_len = row_data.cells.len().min(nw);
            new_cells[dst_start..dst_start + copy_len].copy_from_slice(&row_data.cells[..copy_len]);
            new_wrapped[i] = row_data.wrapped;
        }
        self.cells = new_cells;
        self.width = new_width;
        self.height = new_height;
        self.dirty_rows = vec![true; nh];
        self.wrapped = new_wrapped;
        self.scroll_count = 0;

        let final_row = (new_cursor_row as u16).min(new_height.saturating_sub(1));
        let final_col = (new_cursor_col as u16).min(new_width.saturating_sub(1));
        (final_row, final_col)
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
        blank: Cell,
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
        self.cells[start..end].fill(blank);
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
