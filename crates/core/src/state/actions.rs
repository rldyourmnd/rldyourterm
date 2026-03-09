use crate::{
    events::{CoreEvent, DisplayClearMode, LineClearMode},
    grid::{Attrs, Color, Grid},
    scrollback::Scrollback,
};

use super::{AlternateScreenState, TerminalState};

impl TerminalState {
    pub(super) fn apply_print(&mut self, ch: char, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        let width = self.grid.width();

        // VT100 deferred wrap: if wrap_pending is set from a previous print
        // at the last column, execute the actual wrap now before printing.
        if self.cursor.wrap_pending {
            self.cursor.wrap_pending = false;
            let row = self.cursor.row;
            events.push(CoreEvent::LineWrapped { row });
            self.cursor.col = 0;
            if row >= self.scroll_bottom() {
                self.scroll_up_at_bottom(1, events);
                self.cursor.row = self.scroll_bottom();
            } else {
                self.cursor.row = row + 1;
            }
        }

        let height = self.grid.height();
        let row = self.cursor.row.min(height.saturating_sub(1));
        let col = self.cursor.col.min(width.saturating_sub(1));
        self.cursor.row = row;
        self.cursor.col = col;

        let _ = self.grid.put_char(row, col, ch, self.pen);

        if col + 1 >= width {
            if self.auto_wrap {
                // Deferred wrap: stay at last column, set pending flag.
                // The wrap will execute on the next printable character.
                self.cursor.wrap_pending = true;
            }
            // In both auto_wrap and no-wrap mode, cursor stays at last column
        } else {
            self.cursor.col = col + 1;
        }
    }

    pub(super) fn apply_line_feed(&mut self, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        self.cursor.wrap_pending = false;
        let bottom = self.scroll_bottom();

        if self.cursor.row == bottom {
            self.scroll_up_at_bottom(1, events);
        } else if self.cursor.row + 1 < self.grid.height() {
            self.cursor.row += 1;
        }
    }

    pub(super) fn apply_carriage_return(&mut self, _events: &mut Vec<CoreEvent>) {
        self.cursor.carriage_return();
    }

    pub(super) fn apply_reverse_index(&mut self, _events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        self.cursor.wrap_pending = false;
        let top = self.scroll_top();

        if self.cursor.row == top {
            self.apply_scroll_down(1);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
        }
    }

    pub(super) fn apply_backspace(&mut self, _events: &mut Vec<CoreEvent>) {
        self.cursor.wrap_pending = false;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    pub(super) fn apply_tab(&mut self, _events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }
        self.cursor.wrap_pending = false;
        let next_tab = (self.cursor.col / 8).saturating_add(1).saturating_mul(8);
        self.cursor.col = next_tab.min(self.grid.width().saturating_sub(1));
    }

    pub(super) fn apply_cursor_relative(
        &mut self,
        row_delta: i32,
        col_delta: i32,
        _events: &mut Vec<CoreEvent>,
    ) {
        if self.grid.is_empty() {
            return;
        }
        self.cursor
            .move_relative(row_delta, col_delta, self.grid.width(), self.grid.height());
    }

    pub(super) fn apply_cursor_position(
        &mut self,
        row: u16,
        col: u16,
        _events: &mut Vec<CoreEvent>,
    ) {
        if self.grid.is_empty() {
            return;
        }

        self.cursor
            .move_to(row, col, self.grid.width(), self.grid.height());
    }

    pub(super) fn apply_clear_display(
        &mut self,
        mode: DisplayClearMode,
        events: &mut Vec<CoreEvent>,
    ) {
        if self.grid.is_empty() {
            events.push(CoreEvent::DisplayCleared { mode });
            return;
        }

        let width = self.grid.width();
        let height = self.grid.height();
        let row = self.cursor.row.min(height.saturating_sub(1));
        let col = self.cursor.col.min(width.saturating_sub(1));

        match mode {
            DisplayClearMode::Below => {
                let _ = self.grid.clear_row_from(row, col);
                for target_row in (row + 1)..height {
                    let _ = self.grid.clear_row(target_row);
                }
            }
            DisplayClearMode::Above => {
                for target_row in 0..row {
                    let _ = self.grid.clear_row(target_row);
                }
                let _ = self.grid.clear_row_to_inclusive(row, col);
            }
            DisplayClearMode::All => self.grid.clear(),
            DisplayClearMode::Scrollback => {
                self.scrollback.clear();
            }
        }

        events.push(CoreEvent::DisplayCleared { mode });
    }

    pub(super) fn apply_clear_line(&mut self, mode: LineClearMode, events: &mut Vec<CoreEvent>) {
        let row = if self.grid.is_empty() {
            self.cursor.row
        } else {
            self.cursor.row.min(self.grid.height().saturating_sub(1))
        };

        if !self.grid.is_empty() {
            let col = self.cursor.col.min(self.grid.width().saturating_sub(1));
            match mode {
                LineClearMode::Right => {
                    let _ = self.grid.clear_row_from(row, col);
                }
                LineClearMode::Left => {
                    let _ = self.grid.clear_row_to_inclusive(row, col);
                }
                LineClearMode::All => {
                    let _ = self.grid.clear_row(row);
                }
            }
        }

        events.push(CoreEvent::LineCleared { row, mode });
    }

    pub(super) fn apply_sgr(&mut self, params: &[Option<u16>]) {
        if params.is_empty() {
            self.pen = Attrs::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            let code = params[i].unwrap_or(0);
            match code {
                0 => self.pen = Attrs::default(),
                1 => self.pen.bold = true,
                2 => self.pen.dim = true,
                3 => self.pen.italic = true,
                4 => self.pen.underline = true,
                7 => self.pen.inverse = true,
                9 => self.pen.strikethrough = true,
                22 => {
                    self.pen.bold = false;
                    self.pen.dim = false;
                }
                23 => self.pen.italic = false,
                24 => self.pen.underline = false,
                27 => self.pen.inverse = false,
                29 => self.pen.strikethrough = false,
                30..=37 => self.pen.fg = Color::Indexed((code - 30) as u8),
                38 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.pen.fg = color;
                    }
                }
                39 => self.pen.fg = Color::Default,
                40..=47 => self.pen.bg = Color::Indexed((code - 40) as u8),
                48 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.pen.bg = color;
                    }
                }
                49 => self.pen.bg = Color::Default,
                90..=97 => self.pen.fg = Color::Indexed((code - 90 + 8) as u8),
                100..=107 => self.pen.bg = Color::Indexed((code - 100 + 8) as u8),
                _ => {} // ignore unknown SGR codes
            }
            i += 1;
        }
    }

    pub(super) fn apply_cursor_save(&mut self) {
        self.saved_cursor = Some((self.cursor, self.pen));
    }

    pub(super) fn apply_cursor_restore(&mut self, _events: &mut Vec<CoreEvent>) {
        if let Some((saved_cursor, saved_pen)) = self.saved_cursor {
            self.cursor = saved_cursor;
            self.pen = saved_pen;
            if !self.grid.is_empty() {
                self.cursor.row = self.cursor.row.min(self.grid.height().saturating_sub(1));
                self.cursor.col = self.cursor.col.min(self.grid.width().saturating_sub(1));
            }
        }
    }

    pub(super) fn apply_alternate_screen_enter(&mut self) {
        if self.alternate_screen.is_some() {
            return;
        }

        let w = self.grid.width();
        let h = self.grid.height();
        let saved = AlternateScreenState {
            grid: std::mem::replace(&mut self.grid, Grid::new(w, h)),
            cursor: std::mem::take(&mut self.cursor),
            pen: std::mem::take(&mut self.pen),
            scrollback: std::mem::replace(
                &mut self.scrollback,
                Scrollback::new(0), // alt screen has no scrollback
            ),
            saved_cursor: self.saved_cursor.take(),
            scroll_region: self.scroll_region.take(),
        };

        self.alternate_screen = Some(Box::new(saved));
    }

    pub(super) fn apply_alternate_screen_leave(&mut self) {
        if let Some(saved) = self.alternate_screen.take() {
            self.grid = saved.grid;
            self.cursor = saved.cursor;
            self.pen = saved.pen;
            self.scrollback = saved.scrollback;
            self.saved_cursor = saved.saved_cursor;
            self.scroll_region = saved.scroll_region;
            if !self.grid.is_empty() {
                self.cursor.row = self.cursor.row.min(self.grid.height().saturating_sub(1));
                self.cursor.col = self.cursor.col.min(self.grid.width().saturating_sub(1));
            }
        }
    }

    pub(super) fn apply_set_scroll_region(&mut self, top: u16, bottom: u16) {
        if self.grid.is_empty() {
            return;
        }
        let height = self.grid.height();
        let top = top.min(height.saturating_sub(1));
        let bottom = bottom.min(height.saturating_sub(1));

        if top == 0 && bottom == height.saturating_sub(1) {
            self.scroll_region = None;
        } else if top <= bottom {
            self.scroll_region = Some((top, bottom));
        }
        // CSI r also homes the cursor
        let from = self.cursor;
        self.cursor.row = 0;
        self.cursor.col = 0;
        if from != self.cursor {
            // no event needed for implicit home from CSI r
        }
    }

    pub(super) fn apply_insert_lines(&mut self, n: u16) {
        let bottom = self.scroll_bottom();
        self.grid.insert_lines(self.cursor.row, n, bottom);
    }

    pub(super) fn apply_delete_lines(&mut self, n: u16) {
        let bottom = self.scroll_bottom();
        self.grid.delete_lines(self.cursor.row, n, bottom);
    }

    pub(super) fn apply_scroll_up(&mut self, n: u16, events: &mut Vec<CoreEvent>) {
        let top = self.scroll_top();
        let bottom = self.scroll_bottom();
        if top == 0 && bottom == self.grid.height().saturating_sub(1) {
            self.push_scrolled_lines(n, events);
        } else {
            let _ = self.grid.scroll_up_region(n, top, bottom);
        }
    }

    pub(super) fn apply_scroll_down(&mut self, n: u16) {
        let top = self.scroll_top();
        let bottom = self.scroll_bottom();
        self.grid.scroll_down_region(n, top, bottom);
    }

    pub(super) fn apply_erase_chars(&mut self, n: u16) {
        self.grid.erase_chars(self.cursor.row, self.cursor.col, n);
    }

    pub(super) fn apply_insert_chars(&mut self, n: u16) {
        self.grid.insert_chars(self.cursor.row, self.cursor.col, n);
    }

    pub(super) fn apply_delete_chars(&mut self, n: u16) {
        self.grid.delete_chars(self.cursor.row, self.cursor.col, n);
    }

    fn scroll_top(&self) -> u16 {
        self.scroll_region.map_or(0, |(top, _)| top)
    }

    pub(super) fn scroll_bottom(&self) -> u16 {
        self.scroll_region
            .map_or(self.grid.height().saturating_sub(1), |(_, bottom)| bottom)
    }

    fn scroll_up_at_bottom(&mut self, lines: u16, events: &mut Vec<CoreEvent>) {
        let top = self.scroll_top();
        let bottom = self.scroll_bottom();

        if top == 0 && bottom == self.grid.height().saturating_sub(1) {
            self.push_scrolled_lines(lines, events);
        } else {
            let _ = self.grid.scroll_up_region(lines, top, bottom);
        }
    }

    fn push_scrolled_lines(&mut self, lines: u16, events: &mut Vec<CoreEvent>) {
        if lines == 0 || self.grid.is_empty() {
            return;
        }

        let effective_lines = lines.min(self.grid.height());

        // Push rows directly from cell data into scrollback, avoiding the
        // intermediate Vec<String> allocation that scroll_up would create.
        let mut dropped = 0usize;
        for row in 0..effective_lines {
            if let Ok(cells) = self.grid.row_cells(row) {
                dropped += self.scrollback.push_from_cells(cells);
            }
        }

        // Shift remaining rows up and clear vacated bottom rows.
        self.grid.scroll_up_discard(effective_lines);

        events.push(CoreEvent::GridScrolled {
            lines: effective_lines,
        });
        if dropped > 0 {
            events.push(CoreEvent::ScrollbackTrimmed { dropped });
        }
    }
}

fn parse_extended_color(params: &[Option<u16>], i: &mut usize) -> Option<Color> {
    let next = params.get(*i + 1).copied().flatten()?;
    match next {
        5 => {
            // 256-color: 38;5;N or 48;5;N
            let n = params.get(*i + 2).copied().flatten()?;
            *i += 2;
            Some(Color::Indexed(n as u8))
        }
        2 => {
            // truecolor: 38;2;R;G;B or 48;2;R;G;B
            let r = params.get(*i + 2).copied().flatten()?;
            let g = params.get(*i + 3).copied().flatten()?;
            let b = params.get(*i + 4).copied().flatten()?;
            *i += 4;
            Some(Color::Rgb(r as u8, g as u8, b as u8))
        }
        _ => None,
    }
}
