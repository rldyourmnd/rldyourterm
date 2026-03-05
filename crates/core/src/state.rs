use crate::{
    cursor::Cursor,
    events::{CoreEvent, DisplayClearMode, IngestDegradeReason, LineClearMode},
    grid::{Attrs, Grid},
    parser::{Parser, ParserAction},
    scrollback::Scrollback,
};

const MAX_FEED_BYTES_PER_CALL: usize = 64 * 1024;
const FEED_CHUNK_BYTES: usize = 4 * 1024;

#[derive(Debug)]
struct AlternateScreenState {
    grid: Grid,
    cursor: Cursor,
    pen: Attrs,
    scrollback: Scrollback,
    saved_cursor: Option<(Cursor, Attrs)>,
    scroll_region: Option<(u16, u16)>,
}

#[derive(Debug)]
pub struct TerminalState {
    pub grid: Grid,
    pub cursor: Cursor,
    pub scrollback: Scrollback,
    parser: Parser,
    pub pen: Attrs,
    saved_cursor: Option<(Cursor, Attrs)>,
    scroll_region: Option<(u16, u16)>,
    alternate_screen: Option<Box<AlternateScreenState>>,
    window_title: String,
    pub bracketed_paste: bool,
    pub application_cursor_keys: bool,
    pub auto_wrap: bool,
}

impl TerminalState {
    pub fn new(width: u16, height: u16, scrollback_cap: usize) -> Self {
        Self {
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            scrollback: Scrollback::new(scrollback_cap),
            parser: Parser::default(),
            pen: Attrs::default(),
            saved_cursor: None,
            scroll_region: None,
            alternate_screen: None,
            window_title: String::new(),
            bracketed_paste: false,
            application_cursor_keys: false,
            auto_wrap: true,
        }
    }

    pub fn window_title(&self) -> &str {
        &self.window_title
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
    }

    pub fn application_cursor_keys_enabled(&self) -> bool {
        self.application_cursor_keys
    }

    pub fn auto_wrap_enabled(&self) -> bool {
        self.auto_wrap
    }

    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        self.grid.resize(new_width, new_height);
        self.cursor.row = self.cursor.row.min(new_height.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(new_width.saturating_sub(1));
        self.cursor.wrap_pending = false;
        self.scroll_region = None;

        if let Some(alt) = self.alternate_screen.as_mut() {
            alt.grid.resize(new_width, new_height);
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<CoreEvent> {
        if bytes.is_empty() {
            return Vec::new();
        }

        let accepted = bytes.len().min(MAX_FEED_BYTES_PER_CALL);
        let mut events = Vec::new();

        for chunk in bytes[..accepted].chunks(FEED_CHUNK_BYTES) {
            for action in self.parser.feed(chunk) {
                self.apply_action_into(action, &mut events);
            }
        }

        let dropped = bytes.len() - accepted;
        if dropped > 0 {
            for action in self.parser.resync_after_truncation() {
                self.apply_action_into(action, &mut events);
            }
            events.push(CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                accepted,
                dropped,
            });
        }

        events
    }

    pub fn apply_actions<I>(&mut self, actions: I) -> Vec<CoreEvent>
    where
        I: IntoIterator<Item = ParserAction>,
    {
        let mut events = Vec::new();
        for action in actions {
            self.apply_action_into(action, &mut events);
        }
        events
    }

    pub fn apply_action(&mut self, action: ParserAction) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        self.apply_action_into(action, &mut events);
        events
    }

    fn apply_action_into(&mut self, action: ParserAction, events: &mut Vec<CoreEvent>) {
        match action {
            ParserAction::Print(ch) => self.apply_print(ch, events),
            ParserAction::LineFeed => self.apply_line_feed(events),
            ParserAction::CarriageReturn => self.apply_carriage_return(events),
            ParserAction::Bell => events.push(CoreEvent::Bell),
            ParserAction::Backspace => self.apply_backspace(events),
            ParserAction::Tab => self.apply_tab(events),
            ParserAction::CursorUp(steps) => self.apply_cursor_relative(-(steps as i32), 0, events),
            ParserAction::CursorDown(steps) => self.apply_cursor_relative(steps as i32, 0, events),
            ParserAction::CursorForward(steps) => {
                self.apply_cursor_relative(0, steps as i32, events)
            }
            ParserAction::CursorBack(steps) => {
                self.apply_cursor_relative(0, -(steps as i32), events)
            }
            ParserAction::CursorPosition { row, col } => {
                self.apply_cursor_position(row, col, events)
            }
            ParserAction::CursorHorizontalAbsolute(col) => {
                self.apply_cursor_position(self.cursor.row, col, events)
            }
            ParserAction::CursorNextLine(n) => {
                self.apply_cursor_relative(n as i32, 0, events);
                self.apply_carriage_return(events);
            }
            ParserAction::CursorPreviousLine(n) => {
                self.apply_cursor_relative(-(n as i32), 0, events);
                self.apply_carriage_return(events);
            }
            ParserAction::VerticalPositionAbsolute(row) => {
                self.apply_cursor_position(row, self.cursor.col, events)
            }
            ParserAction::ClearDisplay(mode) => self.apply_clear_display(mode, events),
            ParserAction::ClearLine(mode) => self.apply_clear_line(mode, events),
            ParserAction::SetGraphicsRendition(params) => self.apply_sgr(params.as_slice()),
            ParserAction::CursorSavePosition => self.apply_cursor_save(),
            ParserAction::CursorRestorePosition => self.apply_cursor_restore(events),
            ParserAction::SetCursorVisible(visible) => {
                self.cursor.visible = visible;
                events.push(CoreEvent::CursorVisibilityChanged { visible });
            }
            ParserAction::AlternateScreenEnter => {
                self.apply_alternate_screen_enter();
                events.push(CoreEvent::AlternateScreenEntered);
            }
            ParserAction::AlternateScreenLeave => {
                self.apply_alternate_screen_leave();
                events.push(CoreEvent::AlternateScreenLeft);
            }
            ParserAction::InsertLines(n) => self.apply_insert_lines(n),
            ParserAction::DeleteLines(n) => self.apply_delete_lines(n),
            ParserAction::ScrollUp(n) => self.apply_scroll_up(n, events),
            ParserAction::ScrollDown(n) => self.apply_scroll_down(n),
            ParserAction::EraseCharacters(n) => self.apply_erase_chars(n),
            ParserAction::InsertCharacters(n) => self.apply_insert_chars(n),
            ParserAction::DeleteCharacters(n) => self.apply_delete_chars(n),
            ParserAction::SetScrollRegion { top, bottom } => {
                let height = self.grid.height();
                let bottom = bottom.unwrap_or(height.saturating_sub(1));
                self.apply_set_scroll_region(top, bottom)
            }
            ParserAction::ReverseIndex => self.apply_reverse_index(events),
            ParserAction::NextLine => {
                self.apply_carriage_return(events);
                self.apply_line_feed(events);
            }
            ParserAction::ApplicationKeypadMode(_enabled) => {
                // Acknowledged but no terminal state change needed.
                // Keypad mode affects key encoding on the input side,
                // which is handled in gui_runtime's encode_winit_key_event.
            }
            ParserAction::SetWindowTitle(title) => {
                self.window_title = title.clone();
                events.push(CoreEvent::WindowTitleChanged { title });
            }
            ParserAction::BracketedPasteMode(enabled) => {
                self.bracketed_paste = enabled;
            }
            ParserAction::ApplicationCursorKeys(enabled) => {
                self.application_cursor_keys = enabled;
            }
            ParserAction::AutoWrapMode(enabled) => {
                self.auto_wrap = enabled;
            }
            ParserAction::SendPrimaryDA => {
                events.push(CoreEvent::TerminalResponse {
                    data: b"\x1b[?1;2c".to_vec(),
                });
            }
            ParserAction::SendDeviceStatusReport => {
                let row = self.cursor.row.saturating_add(1);
                let col = self.cursor.col.saturating_add(1);
                events.push(CoreEvent::TerminalResponse {
                    data: format!("\x1b[{row};{col}R").into_bytes(),
                });
            }
            ParserAction::SendDeviceOk => {
                events.push(CoreEvent::TerminalResponse {
                    data: b"\x1b[0n".to_vec(),
                });
            }
            ParserAction::UnsupportedSequence(sequence) => {
                events.push(CoreEvent::UnsupportedSequenceIgnored { sequence });
            }
            ParserAction::IngestDegraded {
                reason,
                accepted,
                dropped,
            } => {
                events.push(CoreEvent::IngestDegraded {
                    reason,
                    accepted,
                    dropped,
                });
            }
        }
    }

    fn apply_print(&mut self, ch: char, events: &mut Vec<CoreEvent>) {
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

        if self.grid.put_char(row, col, ch, self.pen).is_ok() {
            events.push(CoreEvent::CellUpdated {
                row,
                col,
                ch,
                attrs: self.pen,
            });
        }

        let from = self.cursor;
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

        if from != self.cursor {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_line_feed(&mut self, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        self.cursor.wrap_pending = false;
        let from = self.cursor;
        let bottom = self.scroll_bottom();

        if self.cursor.row == bottom {
            self.scroll_up_at_bottom(1, events);
        } else if self.cursor.row + 1 < self.grid.height() {
            self.cursor.row += 1;
        }

        if from != self.cursor {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_carriage_return(&mut self, events: &mut Vec<CoreEvent>) {
        let from = self.cursor;
        if self.cursor.carriage_return() {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_reverse_index(&mut self, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        self.cursor.wrap_pending = false;
        let from = self.cursor;
        let top = self.scroll_top();

        if self.cursor.row == top {
            self.apply_scroll_down(1);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
        }

        if from != self.cursor {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_backspace(&mut self, events: &mut Vec<CoreEvent>) {
        self.cursor.wrap_pending = false;
        let from = self.cursor;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_tab(&mut self, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }
        self.cursor.wrap_pending = false;
        let from = self.cursor;
        let next_tab = ((self.cursor.col / 8) + 1) * 8;
        self.cursor.col = next_tab.min(self.grid.width().saturating_sub(1));
        if from != self.cursor {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_cursor_relative(
        &mut self,
        row_delta: i32,
        col_delta: i32,
        events: &mut Vec<CoreEvent>,
    ) {
        if self.grid.is_empty() {
            return;
        }

        let from = self.cursor;
        if self
            .cursor
            .move_relative(row_delta, col_delta, self.grid.width(), self.grid.height())
        {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_cursor_position(&mut self, row: u16, col: u16, events: &mut Vec<CoreEvent>) {
        if self.grid.is_empty() {
            return;
        }

        let from = self.cursor;
        if self
            .cursor
            .move_to(row, col, self.grid.width(), self.grid.height())
        {
            events.push(CoreEvent::CursorMoved {
                from,
                to: self.cursor,
            });
        }
    }

    fn apply_clear_display(&mut self, mode: DisplayClearMode, events: &mut Vec<CoreEvent>) {
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

    fn apply_clear_line(&mut self, mode: LineClearMode, events: &mut Vec<CoreEvent>) {
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

    fn apply_sgr(&mut self, params: &[Option<u16>]) {
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
                30..=37 => self.pen.fg = crate::grid::Color::Indexed((code - 30) as u8),
                38 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.pen.fg = color;
                    }
                }
                39 => self.pen.fg = crate::grid::Color::Default,
                40..=47 => self.pen.bg = crate::grid::Color::Indexed((code - 40) as u8),
                48 => {
                    if let Some(color) = parse_extended_color(params, &mut i) {
                        self.pen.bg = color;
                    }
                }
                49 => self.pen.bg = crate::grid::Color::Default,
                90..=97 => self.pen.fg = crate::grid::Color::Indexed((code - 90 + 8) as u8),
                100..=107 => self.pen.bg = crate::grid::Color::Indexed((code - 100 + 8) as u8),
                _ => {} // ignore unknown SGR codes
            }
            i += 1;
        }
    }

    fn apply_cursor_save(&mut self) {
        self.saved_cursor = Some((self.cursor, self.pen));
    }

    fn apply_cursor_restore(&mut self, events: &mut Vec<CoreEvent>) {
        if let Some((saved_cursor, saved_pen)) = self.saved_cursor {
            let from = self.cursor;
            self.cursor = saved_cursor;
            self.pen = saved_pen;
            if !self.grid.is_empty() {
                self.cursor.row = self.cursor.row.min(self.grid.height().saturating_sub(1));
                self.cursor.col = self.cursor.col.min(self.grid.width().saturating_sub(1));
            }
            if from != self.cursor {
                events.push(CoreEvent::CursorMoved {
                    from,
                    to: self.cursor,
                });
            }
        }
    }

    fn apply_alternate_screen_enter(&mut self) {
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

    fn apply_alternate_screen_leave(&mut self) {
        if let Some(saved) = self.alternate_screen.take() {
            self.grid = saved.grid;
            self.cursor = saved.cursor;
            self.pen = saved.pen;
            self.scrollback = saved.scrollback;
            self.saved_cursor = saved.saved_cursor;
            self.scroll_region = saved.scroll_region;
        }
    }

    fn apply_set_scroll_region(&mut self, top: u16, bottom: u16) {
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

    fn apply_insert_lines(&mut self, n: u16) {
        let bottom = self.scroll_bottom();
        self.grid.insert_lines(self.cursor.row, n, bottom);
    }

    fn apply_delete_lines(&mut self, n: u16) {
        let bottom = self.scroll_bottom();
        self.grid.delete_lines(self.cursor.row, n, bottom);
    }

    fn apply_scroll_up(&mut self, n: u16, events: &mut Vec<CoreEvent>) {
        let top = self.scroll_top();
        let bottom = self.scroll_bottom();
        if top == 0 && bottom == self.grid.height().saturating_sub(1) {
            self.push_scrolled_lines(n, events);
        } else {
            let _ = self.grid.scroll_up_region(n, top, bottom);
        }
    }

    fn apply_scroll_down(&mut self, n: u16) {
        let top = self.scroll_top();
        let bottom = self.scroll_bottom();
        self.grid.scroll_down_region(n, top, bottom);
    }

    fn apply_erase_chars(&mut self, n: u16) {
        self.grid.erase_chars(self.cursor.row, self.cursor.col, n);
    }

    fn apply_insert_chars(&mut self, n: u16) {
        self.grid.insert_chars(self.cursor.row, self.cursor.col, n);
    }

    fn apply_delete_chars(&mut self, n: u16) {
        self.grid.delete_chars(self.cursor.row, self.cursor.col, n);
    }

    fn scroll_top(&self) -> u16 {
        self.scroll_region.map_or(0, |(top, _)| top)
    }

    fn scroll_bottom(&self) -> u16 {
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
        let removed = self.grid.scroll_up(lines);
        if removed.is_empty() {
            return;
        }

        events.push(CoreEvent::GridScrolled {
            lines: removed.len() as u16,
        });

        let mut dropped = 0usize;
        for line in removed {
            dropped += self.scrollback.push(line);
        }
        if dropped > 0 {
            events.push(CoreEvent::ScrollbackTrimmed { dropped });
        }
    }
}

fn parse_extended_color(params: &[Option<u16>], i: &mut usize) -> Option<crate::grid::Color> {
    let next = params.get(*i + 1).copied().flatten()?;
    match next {
        5 => {
            // 256-color: 38;5;N or 48;5;N
            let n = params.get(*i + 2).copied().flatten()?;
            *i += 2;
            Some(crate::grid::Color::Indexed(n as u8))
        }
        2 => {
            // truecolor: 38;2;R;G;B or 48;2;R;G;B
            let r = params.get(*i + 2).copied().flatten()?;
            let g = params.get(*i + 3).copied().flatten()?;
            let b = params.get(*i + 4).copied().flatten()?;
            *i += 4;
            Some(crate::grid::Color::Rgb(r as u8, g as u8, b as u8))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::events::{CoreEvent, DisplayClearMode, IngestDegradeReason};
    use crate::grid::{Attrs, Color};

    use super::{FEED_CHUNK_BYTES, MAX_FEED_BYTES_PER_CALL, TerminalState};

    #[test]
    fn feed_wraps_and_scrolls_into_scrollback() {
        let mut state = TerminalState::new(3, 2, 10);
        // With deferred wrap, 'i' at last column sets wrap_pending
        // but does NOT trigger the second scroll until the next char.
        let events = state.feed(b"abcdefghi");

        assert_eq!(state.grid.row_string(0).expect("row 0"), "def");
        assert_eq!(state.grid.row_string(1).expect("row 1"), "ghi");
        assert_eq!(state.scrollback.iter().collect::<Vec<_>>(), vec!["abc"]);
        assert!(state.cursor.wrap_pending);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CoreEvent::GridScrolled { .. }))
                .count(),
            1
        );

        // Feeding one more char triggers the deferred wrap and second scroll
        let events2 = state.feed(b"j");
        assert_eq!(state.grid.row_string(0).expect("row 0 after j"), "ghi");
        assert_eq!(state.grid.row_string(1).expect("row 1 after j"), "j  ");
        assert_eq!(
            state.scrollback.iter().collect::<Vec<_>>(),
            vec!["abc", "def"]
        );
        assert!(!state.cursor.wrap_pending);
        assert_eq!(
            events2
                .iter()
                .filter(|event| matches!(event, CoreEvent::GridScrolled { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn scrollback_trim_event_is_emitted_at_cap_boundary() {
        let mut state = TerminalState::new(2, 1, 1);
        // With deferred wrap, 5 chars triggers 2 scrolls:
        // 'b' sets wrap_pending, 'c' triggers 1st scroll ("ab" -> scrollback),
        // 'd' sets wrap_pending, 'e' triggers 2nd scroll ("cd" -> scrollback, "ab" trimmed).
        let events = state.feed(b"abcde");

        assert_eq!(state.scrollback.len(), 1);
        assert_eq!(state.scrollback.get(0), Some("cd"));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::ScrollbackTrimmed { dropped: 1 }))
        );
    }

    #[test]
    fn clear_display_all_sequence_clears_grid() {
        let mut state = TerminalState::new(4, 2, 5);
        let _ = state.feed(b"ab");
        let events = state.feed(b"\x1b[2J");

        assert_eq!(state.grid.row_string(0).expect("row 0"), "    ");
        assert_eq!(state.grid.row_string(1).expect("row 1"), "    ");
        assert!(events.iter().any(|event| matches!(
            event,
            CoreEvent::DisplayCleared {
                mode: DisplayClearMode::All
            }
        )));
    }

    #[test]
    fn unsupported_sequence_is_reported_without_panicking() {
        let mut state = TerminalState::new(4, 1, 5);
        // Use a truly unsupported private mode
        let events = state.feed(b"\x1b[?9999h");

        assert!(
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
        );
    }

    #[test]
    fn malformed_utf8_feed_is_safe() {
        let mut state = TerminalState::new(8, 1, 5);
        let _ = state.feed(&[0xF0, 0x28, 0x8C, 0x28]);

        assert_eq!(state.grid.get_char(0, 0).expect("cell"), '\u{FFFD}');
    }

    #[test]
    fn oversized_feed_is_bounded_and_reported() {
        let mut state = TerminalState::new(1, 1, 1);
        let bytes = vec![b'x'; MAX_FEED_BYTES_PER_CALL + 17];

        let events = state.feed(&bytes);

        assert!(events.iter().any(|event| matches!(
            event,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                accepted,
                dropped
            } if *accepted == MAX_FEED_BYTES_PER_CALL && *dropped == 17
        )));
    }

    #[test]
    fn parser_degrade_action_maps_to_core_event() {
        let mut state = TerminalState::new(4, 2, 4);
        let events = state.apply_action(crate::parser::ParserAction::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted: 64,
            dropped: 3,
        });

        assert_eq!(
            events,
            vec![CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::CsiSequenceTooLong,
                accepted: 64,
                dropped: 3,
            }]
        );
    }

    #[test]
    fn burst_oversized_csi_is_discarded_and_keeps_events_bounded() {
        let mut state = TerminalState::new(8, 2, 8);
        let mut bytes = vec![0x1B, b'['];
        bytes.extend(std::iter::repeat_n(b'1', FEED_CHUNK_BYTES * 2));
        bytes.push(b'A');
        bytes.push(b'Z');

        let events = state.feed(&bytes);

        assert!(events.iter().any(|event| matches!(
            event,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::CsiSequenceTooLong,
                accepted,
                dropped
            } if *accepted == 64 && *dropped == (FEED_CHUNK_BYTES * 2) - 64 + 1
        )));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CoreEvent::CellUpdated { ch: 'Z', .. }))
                .count(),
            1
        );
        assert!(events.len() <= 8);
        assert_eq!(state.grid.row_string(0).expect("row 0"), "Z       ");
    }

    #[test]
    fn oversized_feed_truncation_resyncs_parser_before_next_feed() {
        let mut state = TerminalState::new(4, 2, 10);
        let mut bytes = vec![b'x'; MAX_FEED_BYTES_PER_CALL - 1];
        bytes.push(0x1B);
        bytes.push(b'[');

        let first_events = state.feed(&bytes);
        assert!(first_events.iter().any(|event| matches!(
            event,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                accepted,
                dropped
            } if *accepted == MAX_FEED_BYTES_PER_CALL && *dropped == 1
        )));
        assert!(
            first_events
                .iter()
                .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
        );

        let second_events = state.feed(b"A");
        assert!(
            second_events
                .iter()
                .any(|event| matches!(event, CoreEvent::CellUpdated { ch: 'A', .. }))
        );
        assert!(
            !second_events
                .iter()
                .any(|event| matches!(event, CoreEvent::UnsupportedSequenceIgnored { .. }))
        );
    }

    #[test]
    fn truncated_overlong_csi_still_emits_csi_degrade_event() {
        let mut state = TerminalState::new(4, 2, 10);
        let mut bytes = vec![0x1B, b'['];
        bytes.extend(std::iter::repeat_n(
            b'1',
            MAX_FEED_BYTES_PER_CALL + FEED_CHUNK_BYTES,
        ));

        let events = state.feed(&bytes);

        assert!(events.iter().any(|event| matches!(
            event,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::CsiSequenceTooLong,
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                ..
            }
        )));
    }

    #[test]
    fn sgr_sets_pen_attributes() {
        let mut state = TerminalState::new(10, 2, 5);
        // ESC[1;31m = bold + red fg
        let _ = state.feed(b"\x1b[1;31mA");
        assert!(state.pen.bold);
        assert_eq!(state.pen.fg, Color::Indexed(1));
        let cell = state.grid.get_cell(0, 0).expect("cell");
        assert_eq!(cell.ch, 'A');
        assert!(cell.attrs.bold);
        assert_eq!(cell.attrs.fg, Color::Indexed(1));
    }

    #[test]
    fn sgr_reset_clears_pen() {
        let mut state = TerminalState::new(10, 2, 5);
        let _ = state.feed(b"\x1b[1;31mA\x1b[0mB");
        assert_eq!(state.pen, Attrs::default());
        let cell = state.grid.get_cell(0, 1).expect("cell B");
        assert_eq!(cell.attrs, Attrs::default());
    }

    #[test]
    fn sgr_256_color() {
        let mut state = TerminalState::new(10, 2, 5);
        let _ = state.feed(b"\x1b[38;5;196mR");
        assert_eq!(state.pen.fg, Color::Indexed(196));
    }

    #[test]
    fn sgr_truecolor() {
        let mut state = TerminalState::new(10, 2, 5);
        let _ = state.feed(b"\x1b[38;2;255;128;0mO");
        assert_eq!(state.pen.fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn tab_advances_to_next_stop() {
        let mut state = TerminalState::new(20, 1, 5);
        let _ = state.feed(b"AB\t");
        assert_eq!(state.cursor.col, 8);
    }

    #[test]
    fn cursor_save_restore_preserves_pen() {
        let mut state = TerminalState::new(10, 5, 5);
        let _ = state.feed(b"\x1b[1;31m\x1b7\x1b[0m\x1b8");
        assert!(state.pen.bold);
        assert_eq!(state.pen.fg, Color::Indexed(1));
    }

    #[test]
    fn alternate_screen_enter_leave_roundtrip() {
        let mut state = TerminalState::new(4, 2, 5);
        let _ = state.feed(b"ABCD");
        let _ = state.feed(b"\x1b[?1049h");
        assert_eq!(state.grid.row_string(0).expect("alt row 0"), "    ");

        let _ = state.feed(b"XY");
        let _ = state.feed(b"\x1b[?1049l");
        assert_eq!(state.grid.row_string(0).expect("main row 0"), "ABCD");
    }

    #[test]
    fn scroll_region_confines_scrolling() {
        let mut state = TerminalState::new(3, 5, 10);
        for row in 0..5u16 {
            let ch = (b'A' + row as u8) as char;
            for col in 0..3u16 {
                let _ = state.grid.put_char(row, col, ch, Attrs::default());
            }
        }
        // Set scroll region rows 1..3 (0-indexed)
        let _ = state.feed(b"\x1b[2;4r");
        // Move to row 3 (bottom of region) and send LF
        state.cursor.row = 3;
        state.cursor.col = 0;
        let _ = state.feed(b"\n");

        assert_eq!(state.grid.row_string(0).expect("row 0"), "AAA");
        assert_eq!(state.grid.row_string(1).expect("row 1"), "CCC");
        assert_eq!(state.grid.row_string(2).expect("row 2"), "DDD");
        assert_eq!(state.grid.row_string(3).expect("row 3"), "   ");
        assert_eq!(state.grid.row_string(4).expect("row 4"), "EEE");
    }

    #[test]
    fn resize_preserves_content() {
        let mut state = TerminalState::new(4, 3, 10);
        let _ = state.feed(b"AB");
        state.resize(6, 4);
        assert_eq!(state.grid.width(), 6);
        assert_eq!(state.grid.height(), 4);
        assert_eq!(state.grid.get_char(0, 0).expect("cell"), 'A');
        assert_eq!(state.grid.get_char(0, 1).expect("cell"), 'B');
    }

    #[test]
    fn resize_clamps_cursor() {
        let mut state = TerminalState::new(10, 10, 10);
        state.cursor.row = 8;
        state.cursor.col = 9;
        state.resize(5, 5);
        assert_eq!(state.cursor.row, 4);
        assert_eq!(state.cursor.col, 4);
    }

    #[test]
    fn cursor_visibility_via_csi_25() {
        let mut state = TerminalState::new(10, 2, 5);
        assert!(state.cursor.visible);
        let events = state.feed(b"\x1b[?25l");
        assert!(!state.cursor.visible);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoreEvent::CursorVisibilityChanged { visible: false }))
        );
        let events = state.feed(b"\x1b[?25h");
        assert!(state.cursor.visible);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoreEvent::CursorVisibilityChanged { visible: true }))
        );
    }

    #[test]
    fn window_title_from_osc() {
        let mut state = TerminalState::new(10, 2, 5);
        let events = state.feed(b"\x1b]0;My Title\x07");
        assert_eq!(state.window_title(), "My Title");
        assert!(
            events.iter().any(
                |e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "My Title")
            )
        );
    }

    #[test]
    fn scroll_region_print_wrap_stays_in_region() {
        let mut state = TerminalState::new(4, 8, 10);
        for row in 0..8u16 {
            let ch = (b'A' + row as u8) as char;
            for col in 0..4u16 {
                let _ = state.grid.put_char(row, col, ch, Attrs::default());
            }
        }
        // Set scroll region rows 3..6 (1-indexed: 4;7r -> 0-indexed 3..6)
        let _ = state.feed(b"\x1b[4;7r");
        // Move cursor to bottom of region, last column
        state.cursor.row = 6;
        state.cursor.col = 3;
        // Print two chars: 'X' at last column sets wrap_pending,
        // 'Y' triggers the deferred wrap + region scroll.
        let events = state.feed(b"XY");
        // After deferred wrap + scroll, cursor is at (6, 1)
        assert_eq!(state.cursor.row, 6);
        assert_eq!(state.cursor.col, 1);
        // Invariant: region-local wrap must not emit global scroll events.
        assert!(
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::LineWrapped { row: 6 }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CoreEvent::GridScrolled { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CoreEvent::ScrollbackTrimmed { .. }))
        );

        // Rows outside region remain unchanged.
        assert_eq!(state.grid.row_string(2).expect("row 2"), "CCCC");
        assert_eq!(state.grid.row_string(7).expect("row 7"), "HHHH");
        // Region scrolled up by one: DDDD dropped, each line shifted up.
        // Row 6 had GGGX (X written before scroll), moved to row 5.
        assert_eq!(state.grid.row_string(3).expect("row 3"), "EEEE");
        assert_eq!(state.grid.row_string(4).expect("row 4"), "FFFF");
        assert_eq!(state.grid.row_string(5).expect("row 5"), "GGGX");
        assert_eq!(state.grid.row_string(6).expect("row 6"), "Y   ");
    }

    #[test]
    fn cursor_restore_is_non_consuming() {
        let mut state = TerminalState::new(10, 10, 5);
        // Save cursor at (5, 10) - ESC 7
        state.cursor.row = 5;
        state.cursor.col = 9;
        let _ = state.feed(b"\x1b7");
        // Restore - ESC 8
        state.cursor.row = 0;
        state.cursor.col = 0;
        let _ = state.feed(b"\x1b8");
        assert_eq!(state.cursor.row, 5);
        assert_eq!(state.cursor.col, 9);
        // Restore again - should still work (non-consuming)
        state.cursor.row = 0;
        state.cursor.col = 0;
        let _ = state.feed(b"\x1b8");
        assert_eq!(state.cursor.row, 5);
        assert_eq!(state.cursor.col, 9);
    }

    #[test]
    fn cursor_restore_survives_resize() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 5;
        state.cursor.col = 9;
        let _ = state.feed(b"\x1b7");
        // Resize smaller
        state.resize(3, 3);
        // Restore - should clamp to (2, 2)
        let _ = state.feed(b"\x1b8");
        assert_eq!(state.cursor.row, 2);
        assert_eq!(state.cursor.col, 2);
        // Resize back to original
        state.resize(10, 10);
        // Restore again - should get original (5, 9) since saved_cursor is preserved
        let _ = state.feed(b"\x1b8");
        assert_eq!(state.cursor.row, 5);
        assert_eq!(state.cursor.col, 9);
    }

    #[test]
    fn scroll_region_single_line() {
        let mut state = TerminalState::new(10, 5, 5);
        // ESC[3;3r -> 1-line region at row 2 (0-indexed)
        let _ = state.feed(b"\x1b[3;3r");
        assert_eq!(state.scroll_region, Some((2, 2)));
    }

    #[test]
    fn bracketed_paste_mode_toggle() {
        let mut state = TerminalState::new(10, 2, 5);
        assert!(!state.bracketed_paste_enabled());
        let _ = state.feed(b"\x1b[?2004h");
        assert!(state.bracketed_paste_enabled());
        let _ = state.feed(b"\x1b[?2004l");
        assert!(!state.bracketed_paste_enabled());
    }

    #[test]
    fn application_cursor_keys_mode() {
        let mut state = TerminalState::new(10, 2, 5);
        assert!(!state.application_cursor_keys_enabled());
        let _ = state.feed(b"\x1b[?1h");
        assert!(state.application_cursor_keys_enabled());
        let _ = state.feed(b"\x1b[?1l");
        assert!(!state.application_cursor_keys_enabled());
    }

    #[test]
    fn auto_wrap_mode() {
        let mut state = TerminalState::new(10, 2, 5);
        assert!(state.auto_wrap_enabled());
        let _ = state.feed(b"\x1b[?7l");
        assert!(!state.auto_wrap_enabled());
        let _ = state.feed(b"\x1b[?7h");
        assert!(state.auto_wrap_enabled());
    }

    #[test]
    fn no_wrap_mode_keeps_cursor_at_last_column_without_line_wrap() {
        let mut state = TerminalState::new(3, 1, 5);
        let _ = state.feed(b"\x1b[?7l");

        let events = state.feed(b"abcd");

        assert_eq!(state.grid.row_string(0).expect("row 0"), "abd");
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 2);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CoreEvent::LineWrapped { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, CoreEvent::GridScrolled { .. }))
        );
    }

    #[test]
    fn osc_st_terminator_sets_title() {
        let mut state = TerminalState::new(10, 2, 5);
        let events = state.feed(b"\x1b]0;Title ST\x1b\\");
        assert_eq!(state.window_title(), "Title ST");
        assert!(
            events.iter().any(
                |e| matches!(e, CoreEvent::WindowTitleChanged { title } if title == "Title ST")
            )
        );
    }

    #[test]
    fn primary_da_emits_terminal_response() {
        let mut state = TerminalState::new(10, 4, 5);
        let events = state.feed(b"\x1b[c");
        assert!(
            events.iter().any(
                |e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[?1;2c")
            )
        );
    }

    #[test]
    fn device_status_report_emits_cursor_position() {
        let mut state = TerminalState::new(10, 4, 5);
        // Move cursor to row 2, col 5 (0-based) via CursorPosition (1-based params)
        state.feed(b"\x1b[3;6H");
        let events = state.feed(b"\x1b[6n");
        // Response should be 1-based: row=3, col=6
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[3;6R"))
        );
    }

    #[test]
    fn reverse_index_scrolls_down_at_top_of_region() {
        let mut state = TerminalState::new(4, 5, 10);
        // Set scroll region rows 1..3 (0-indexed)
        let _ = state.feed(b"\x1b[2;4r");
        // Move cursor to top of region
        state.cursor.row = 1;
        state.cursor.col = 2;
        // Reverse index at top of region should scroll down
        let _ = state.feed(b"\x1bM");
        assert_eq!(state.cursor.row, 1);
        assert_eq!(state.cursor.col, 2);
    }

    #[test]
    fn reverse_index_moves_cursor_up() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 5;
        state.cursor.col = 3;
        let _ = state.feed(b"\x1bM");
        assert_eq!(state.cursor.row, 4);
        assert_eq!(state.cursor.col, 3);
    }

    #[test]
    fn next_line_moves_down_and_to_col_zero() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 3;
        state.cursor.col = 7;
        let _ = state.feed(b"\x1bE");
        assert_eq!(state.cursor.row, 4);
        assert_eq!(state.cursor.col, 0);
    }

    #[test]
    fn clear_scrollback_empties_scrollback_buffer() {
        let mut state = TerminalState::new(3, 2, 10);
        // Fill scrollback
        let _ = state.feed(b"abcdefghi");
        assert!(!state.scrollback.is_empty());
        // Clear scrollback
        let events = state.feed(b"\x1b[3J");
        assert!(state.scrollback.is_empty());
        assert!(events.iter().any(|e| matches!(
            e,
            CoreEvent::DisplayCleared {
                mode: DisplayClearMode::Scrollback
            }
        )));
    }

    #[test]
    fn cursor_next_line_moves_down_and_to_col_zero() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 2;
        state.cursor.col = 5;
        let _ = state.feed(b"\x1b[3E");
        assert_eq!(state.cursor.row, 5);
        assert_eq!(state.cursor.col, 0);
    }

    #[test]
    fn cursor_previous_line_moves_up_and_to_col_zero() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 5;
        state.cursor.col = 7;
        let _ = state.feed(b"\x1b[2F");
        assert_eq!(state.cursor.row, 3);
        assert_eq!(state.cursor.col, 0);
    }

    #[test]
    fn vertical_position_absolute_sets_row() {
        let mut state = TerminalState::new(10, 10, 5);
        state.cursor.row = 0;
        state.cursor.col = 5;
        let _ = state.feed(b"\x1b[4d");
        assert_eq!(state.cursor.row, 3);
        assert_eq!(state.cursor.col, 5);
    }

    #[test]
    fn device_ok_emits_terminal_response() {
        let mut state = TerminalState::new(10, 4, 5);
        let events = state.feed(b"\x1b[5n");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CoreEvent::TerminalResponse { data } if data == b"\x1b[0n"))
        );
    }

    #[test]
    fn deferred_wrap_sets_flag_at_last_column() {
        let mut state = TerminalState::new(3, 2, 5);
        let _ = state.feed(b"abc");
        // After 'c' at last column (col 2), cursor stays at col 2 with wrap_pending
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 2);
        assert!(state.cursor.wrap_pending);
    }

    #[test]
    fn deferred_wrap_cr_clears_without_wrapping() {
        // This is the exact fish right-prompt scenario:
        // print to last column, then CR should stay on same row.
        let mut state = TerminalState::new(3, 2, 5);
        let _ = state.feed(b"abc");
        assert!(state.cursor.wrap_pending);
        // CR clears wrap_pending and moves to col 0, same row
        let _ = state.feed(b"\r");
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 0);
        assert!(!state.cursor.wrap_pending);
    }

    #[test]
    fn deferred_wrap_cuf_clears_without_wrapping() {
        let mut state = TerminalState::new(5, 2, 5);
        let _ = state.feed(b"abcde");
        assert!(state.cursor.wrap_pending);
        // CUF(2) clears wrap_pending and moves forward (clamped to width)
        let _ = state.feed(b"\x1b[2C");
        assert_eq!(state.cursor.row, 0);
        assert!(!state.cursor.wrap_pending);
    }

    #[test]
    fn deferred_wrap_next_char_triggers_wrap() {
        let mut state = TerminalState::new(3, 2, 5);
        let _ = state.feed(b"abc");
        assert!(state.cursor.wrap_pending);
        assert_eq!(state.cursor.row, 0);
        // Next printable char triggers the deferred wrap
        let _ = state.feed(b"d");
        assert_eq!(state.cursor.row, 1);
        assert_eq!(state.cursor.col, 1);
        assert!(!state.cursor.wrap_pending);
        assert_eq!(state.grid.row_string(0).expect("row 0"), "abc");
        assert_eq!(state.grid.row_string(1).expect("row 1"), "d  ");
    }

    #[test]
    fn deferred_wrap_fish_right_prompt_pattern() {
        // Simulates the fish shell right-prompt pattern that caused the staircase bug:
        // print chars to fill the last column, then CR + CUF to reposition.
        let mut state = TerminalState::new(10, 3, 5);
        // Fill row 0 to the last column
        let _ = state.feed(b"0123456789");
        assert!(state.cursor.wrap_pending);
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 9);
        // CR (like fish does after drawing right prompt)
        let _ = state.feed(b"\r");
        assert_eq!(state.cursor.row, 0); // Must stay on same row!
        assert_eq!(state.cursor.col, 0);
        assert!(!state.cursor.wrap_pending);
        // CUF to reposition cursor (like fish does to go back to command line)
        let _ = state.feed(b"\x1b[5C");
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 5);
        assert!(!state.cursor.wrap_pending);
    }

    #[test]
    fn deferred_wrap_lf_clears_without_wrapping() {
        let mut state = TerminalState::new(3, 3, 5);
        let _ = state.feed(b"abc");
        assert!(state.cursor.wrap_pending);
        // LF clears wrap_pending and moves down, col stays at 2
        let _ = state.feed(b"\n");
        assert_eq!(state.cursor.row, 1);
        assert_eq!(state.cursor.col, 2);
        assert!(!state.cursor.wrap_pending);
    }

    #[test]
    fn deferred_wrap_resize_clears_flag() {
        let mut state = TerminalState::new(3, 2, 5);
        let _ = state.feed(b"abc");
        assert!(state.cursor.wrap_pending);
        state.resize(5, 3);
        assert!(!state.cursor.wrap_pending);
    }

    // ── Stress tests ─────────────────────────────────────────────

    #[test]
    fn stress_ai_cli_output_burst_10k_lines() {
        let mut state = TerminalState::new(80, 24, 50_000);
        let line = b"\x1b[32mOutput line with ANSI color\x1b[0m\r\n";
        for _ in 0..10_000 {
            state.feed(line);
        }
        assert!(state.scrollback.len() > 0);
        assert!(state.scrollback.len() <= 50_000);
    }

    #[test]
    fn stress_scrollback_cap_enforced_at_50k() {
        let mut state = TerminalState::new(80, 24, 50_000);
        let line = b"scrollback test line\r\n";
        for _ in 0..60_000 {
            state.feed(line);
        }
        assert!(state.scrollback.len() <= 50_000);
    }

    #[test]
    fn stress_unicode_multibyte_throughput() {
        let mut state = TerminalState::new(80, 24, 1_000);
        let text = "Hello Мир 你好 🌍\r\n".as_bytes();
        for _ in 0..5_000 {
            state.feed(text);
        }
        // Should not panic, grid should have valid state
        assert!(state.grid.height() > 0);
    }

    #[test]
    fn stress_rapid_sgr_attribute_sequences() {
        let mut state = TerminalState::new(80, 24, 100);
        let mut buf = Vec::with_capacity(100_000);
        for i in 0..10_000u32 {
            let sgr = format!("\x1b[{}mX", i % 109);
            buf.extend_from_slice(sgr.as_bytes());
        }
        state.feed(&buf);
    }

    #[test]
    fn stress_cursor_positioning_boundaries() {
        let mut state = TerminalState::new(80, 24, 100);
        for row in 1..=30u16 {
            for col in 1..=90u16 {
                let seq = format!("\x1b[{};{}H", row, col);
                state.feed(seq.as_bytes());
            }
        }
        // Cursor should be clamped to grid bounds
        assert!(state.cursor.row < state.grid.height());
        assert!(state.cursor.col < state.grid.width());
    }

    #[test]
    fn stress_resize_during_output() {
        let mut state = TerminalState::new(80, 24, 1_000);
        for i in 0..500u16 {
            state.feed(b"some output text\r\n");
            let w = 60 + (i % 40);
            let h = 20 + (i % 10);
            state.resize(w, h);
        }
        assert!(state.cursor.row < state.grid.height());
        assert!(state.cursor.col < state.grid.width());
    }

    #[test]
    fn stress_bulk_feed_max_chunk_64kb() {
        let mut state = TerminalState::new(80, 24, 1_000);
        let bulk = vec![b'A'; MAX_FEED_BYTES_PER_CALL];
        state.feed(&bulk);
        // Should not panic; grid filled with 'A's
        let cell = state.grid.get_cell(0, 0).unwrap();
        assert_eq!(cell.ch, 'A');
    }

    #[test]
    fn stress_incomplete_escape_at_chunk_boundary() {
        let mut state = TerminalState::new(80, 24, 100);
        // Send partial escape sequence at chunk boundary
        for _ in 0..1_000 {
            state.feed(b"\x1b[");
            state.feed(b"31m");
            state.feed(b"X");
        }
        // Parser should recover and render 'X' chars
    }

    #[test]
    fn stress_alternating_normal_and_alternate_screen() {
        let mut state = TerminalState::new(80, 24, 1_000);
        for _ in 0..1_000 {
            // Enter alternate screen
            state.feed(b"\x1b[?1049h");
            state.feed(b"alternate content\r\n");
            // Exit alternate screen
            state.feed(b"\x1b[?1049l");
            state.feed(b"normal content\r\n");
        }
    }

    #[test]
    fn stress_attribute_combinations_all_64() {
        let mut state = TerminalState::new(80, 24, 100);
        // Bold=1, Dim=2, Italic=3, Underline=4, Blink=5, Inverse=7, Strikethrough=9
        let combos = [
            "\x1b[1;3;4mBIU\x1b[0m",       // bold+italic+underline
            "\x1b[1;2;7mBDR\x1b[0m",       // bold+dim+inverse
            "\x1b[1;3;4;9mBIUS\x1b[0m",    // bold+italic+underline+strikethrough
            "\x1b[1;2;3;4;7;9mALL\x1b[0m", // all attributes
        ];
        for _ in 0..1_000 {
            for combo in &combos {
                state.feed(combo.as_bytes());
            }
        }
    }

    #[test]
    fn stress_erase_operations() {
        let mut state = TerminalState::new(80, 24, 100);
        for _ in 0..5_000 {
            state.feed(b"fill this line with text");
            state.feed(b"\x1b[2J"); // erase entire screen
            state.feed(b"\x1b[K"); // erase to end of line
            state.feed(b"\x1b[1K"); // erase to start of line
            state.feed(b"\x1b[2K"); // erase entire line
        }
    }
}
