use crate::{events::CoreEvent, parser::ParserAction};

use super::TerminalState;

impl TerminalState {
    #[cfg(test)]
    pub(crate) fn apply_action(&mut self, action: ParserAction) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        self.apply_action_into(action, &mut events);
        events
    }

    pub(super) fn apply_action_into(&mut self, action: ParserAction, events: &mut Vec<CoreEvent>) {
        match action {
            ParserAction::Print(ch) => self.apply_print(ch, events),
            ParserAction::PrintText(ref text) => {
                for ch in text.chars() {
                    self.apply_print(ch, events);
                }
            }
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
            ParserAction::ApplicationKeypadMode(enabled) => {
                self.application_keypad_mode = enabled;
            }
            ParserAction::SetWindowTitle(title) => {
                if self.window_title != title {
                    self.window_title = title.clone();
                    events.push(CoreEvent::WindowTitleChanged { title });
                }
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
}
