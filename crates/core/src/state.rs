use crate::{
    cursor::Cursor,
    events::{CoreEvent, DisplayClearMode, IngestDegradeReason, LineClearMode},
    grid::Grid,
    parser::{Parser, ParserAction},
    scrollback::Scrollback,
};

const MAX_FEED_BYTES_PER_CALL: usize = 64 * 1024;
const FEED_CHUNK_BYTES: usize = 4 * 1024;

#[derive(Debug)]
pub struct TerminalState {
    pub grid: Grid,
    pub cursor: Cursor,
    pub scrollback: Scrollback,
    parser: Parser,
}

impl TerminalState {
    pub fn new(width: u16, height: u16, scrollback_cap: usize) -> Self {
        Self {
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            scrollback: Scrollback::new(scrollback_cap),
            parser: Parser::default(),
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
            ParserAction::ClearDisplay(mode) => self.apply_clear_display(mode, events),
            ParserAction::ClearLine(mode) => self.apply_clear_line(mode, events),
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
        let height = self.grid.height();
        let row = self.cursor.row.min(height.saturating_sub(1));
        let col = self.cursor.col.min(width.saturating_sub(1));
        self.cursor.row = row;
        self.cursor.col = col;

        if self.grid.put_char(row, col, ch).is_ok() {
            events.push(CoreEvent::CellUpdated { row, col, ch });
        }

        let from = self.cursor;
        if col + 1 >= width {
            events.push(CoreEvent::LineWrapped { row });
            self.cursor.col = 0;
            if row + 1 >= height {
                self.push_scrolled_lines(1, events);
                self.cursor.row = height.saturating_sub(1);
            } else {
                self.cursor.row = row + 1;
            }
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

        let from = self.cursor;
        if self.cursor.row + 1 >= self.grid.height() {
            self.push_scrolled_lines(1, events);
            self.cursor.row = self.grid.height().saturating_sub(1);
        } else {
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

    fn apply_backspace(&mut self, events: &mut Vec<CoreEvent>) {
        let from = self.cursor;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
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

#[cfg(test)]
mod tests {
    use crate::events::{CoreEvent, DisplayClearMode, IngestDegradeReason};

    use super::{FEED_CHUNK_BYTES, MAX_FEED_BYTES_PER_CALL, TerminalState};

    #[test]
    fn feed_wraps_and_scrolls_into_scrollback() {
        let mut state = TerminalState::new(3, 2, 10);
        let events = state.feed(b"abcdefghi");

        assert_eq!(state.grid.row_string(0).expect("row 0"), "ghi");
        assert_eq!(state.grid.row_string(1).expect("row 1"), "   ");
        assert_eq!(
            state.scrollback.iter().collect::<Vec<_>>(),
            vec!["abc", "def"]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, CoreEvent::GridScrolled { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn scrollback_trim_event_is_emitted_at_cap_boundary() {
        let mut state = TerminalState::new(2, 1, 1);
        let events = state.feed(b"abcd");

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
        let events = state.feed(b"\x1b[?25l");

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
}
