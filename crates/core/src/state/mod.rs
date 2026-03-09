mod actions;
mod dispatch;

#[cfg(test)]
mod tests_feed;
#[cfg(test)]
mod tests_modes;
#[cfg(test)]
mod tests_stress;

use crate::{
    cursor::Cursor,
    events::{CoreEvent, IngestDegradeReason},
    grid::{Attrs, Grid},
    parser::{Parser, ParserAction},
    scrollback::Scrollback,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseMode {
    #[default]
    Off,
    /// Mode 1000: report button press/release.
    Basic,
    /// Mode 1002: report press/release and motion while button held.
    ButtonTrack,
    /// Mode 1003: report all motion events.
    AnyEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseFormat {
    #[default]
    Normal,
    /// Mode 1006: SGR extended mouse format (no coordinate limit).
    Sgr,
}

pub const MAX_FEED_BYTES_PER_CALL: usize = 64 * 1024;
pub(super) const FEED_CHUNK_BYTES: usize = 4 * 1024;
const PARSER_ACTIONS_SCRATCH_INITIAL_CAPACITY: usize = FEED_CHUNK_BYTES / 2;
const FEED_EVENTS_SCRATCH_INITIAL_CAPACITY: usize = 8;

#[derive(Debug)]
pub(super) struct AlternateScreenState {
    pub(super) grid: Grid,
    pub(super) cursor: Cursor,
    pub(super) pen: Attrs,
    pub(super) scrollback: Scrollback,
    pub(super) saved_cursor: Option<(Cursor, Attrs)>,
    pub(super) scroll_region: Option<(u16, u16)>,
}

#[derive(Debug)]
pub struct TerminalState {
    pub grid: Grid,
    pub cursor: Cursor,
    pub scrollback: Scrollback,
    pub(super) parser: Parser,
    pub(super) parser_actions_scratch: Vec<ParserAction>,
    pub(super) feed_events_scratch: Vec<CoreEvent>,
    pub(super) pen: Attrs,
    pub(super) saved_cursor: Option<(Cursor, Attrs)>,
    pub(super) scroll_region: Option<(u16, u16)>,
    pub(super) alternate_screen: Option<Box<AlternateScreenState>>,
    pub(super) window_title: String,
    pub(super) cwd: String,
    pub(super) pending_clipboard: Option<(char, String)>,
    pub(super) bracketed_paste: bool,
    pub(super) application_keypad_mode: bool,
    pub(super) application_cursor_keys: bool,
    pub(super) auto_wrap: bool,
    pub(super) mouse_mode: MouseMode,
    pub(super) mouse_format: MouseFormat,
    pub(super) cursor_blink: bool,
    pub(super) cursor_shape: u8,
    pub(super) focus_reporting: bool,
    pub(super) synchronized_output: bool,
    pub(super) last_printed_char: Option<char>,
    pub(super) tab_stops: Vec<bool>,
}

impl TerminalState {
    pub fn new(width: u16, height: u16, scrollback_cap: usize) -> Self {
        Self {
            grid: Grid::new(width, height),
            cursor: Cursor::new(),
            scrollback: Scrollback::new(scrollback_cap),
            parser: Parser::default(),
            parser_actions_scratch: Vec::with_capacity(PARSER_ACTIONS_SCRATCH_INITIAL_CAPACITY),
            feed_events_scratch: Vec::with_capacity(FEED_EVENTS_SCRATCH_INITIAL_CAPACITY),
            pen: Attrs::default(),
            saved_cursor: None,
            scroll_region: None,
            alternate_screen: None,
            window_title: String::new(),
            cwd: String::new(),
            pending_clipboard: None,
            bracketed_paste: false,
            application_keypad_mode: false,
            application_cursor_keys: false,
            auto_wrap: true,
            mouse_mode: MouseMode::Off,
            mouse_format: MouseFormat::Normal,
            cursor_blink: false,
            cursor_shape: 0,
            focus_reporting: false,
            synchronized_output: false,
            last_printed_char: None,
            tab_stops: Self::default_tab_stops(width),
        }
    }

    fn default_tab_stops(width: u16) -> Vec<bool> {
        let w = width as usize;
        let mut stops = vec![false; w];
        for col in (8..w).step_by(8) {
            stops[col] = true;
        }
        stops
    }

    pub fn window_title(&self) -> &str {
        &self.window_title
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Returns and clears the pending clipboard set request from OSC 52.
    /// The tuple contains (selection_char, base64_data).
    pub fn take_pending_clipboard(&mut self) -> Option<(char, String)> {
        self.pending_clipboard.take()
    }

    pub fn bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
    }

    pub fn mouse_mode(&self) -> MouseMode {
        self.mouse_mode
    }

    pub fn mouse_format(&self) -> MouseFormat {
        self.mouse_format
    }

    pub fn cursor_shape(&self) -> u8 {
        self.cursor_shape
    }

    pub fn focus_reporting_enabled(&self) -> bool {
        self.focus_reporting
    }

    pub fn synchronized_output_enabled(&self) -> bool {
        self.synchronized_output
    }

    #[cfg(test)]
    pub(crate) fn application_cursor_keys_enabled(&self) -> bool {
        self.application_cursor_keys
    }

    #[cfg(test)]
    pub(crate) fn application_keypad_mode_enabled(&self) -> bool {
        self.application_keypad_mode
    }

    #[cfg(test)]
    pub(crate) fn auto_wrap_enabled(&self) -> bool {
        self.auto_wrap
    }

    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        // Primary screen: reflow to preserve logical lines across width changes.
        let (new_row, new_col) = self.grid.resize_with_reflow(
            new_width,
            new_height,
            self.cursor.row,
            self.cursor.col,
            &mut self.scrollback,
        );
        self.cursor.row = new_row;
        self.cursor.col = new_col;
        self.cursor.wrap_pending = false;
        self.scroll_region = None;

        let old_len = self.tab_stops.len();
        let new_len = new_width as usize;
        self.tab_stops.resize(new_len, false);
        for col in (old_len..new_len).filter(|c| c % 8 == 0) {
            self.tab_stops[col] = true;
        }

        // Alternate screen: simple resize (no reflow per xterm/VTE convention).
        if let Some(alt) = self.alternate_screen.as_mut() {
            alt.grid.resize(new_width, new_height);
            alt.cursor.row = alt.cursor.row.min(new_height.saturating_sub(1));
            alt.cursor.col = alt.cursor.col.min(new_width.saturating_sub(1));
            alt.cursor.wrap_pending = false;
        }
    }

    #[cfg(test)]
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<CoreEvent> {
        let mut events = Vec::new();
        self.feed_into(bytes, &mut events);
        events
    }

    /// Feed terminal bytes while reusing a caller-provided event buffer.
    /// This avoids per-call `Vec<CoreEvent>` allocations on hot ingest paths.
    pub(crate) fn feed_into(&mut self, bytes: &[u8], events: &mut Vec<CoreEvent>) {
        events.clear();
        if bytes.is_empty() {
            return;
        }

        let accepted = bytes.len().min(MAX_FEED_BYTES_PER_CALL);

        for chunk in bytes[..accepted].chunks(FEED_CHUNK_BYTES) {
            let mut parser_actions = std::mem::take(&mut self.parser_actions_scratch);
            self.parser.feed_into(chunk, &mut parser_actions);
            for action in parser_actions.drain(..) {
                self.apply_action_into(action, events);
            }
            self.parser_actions_scratch = parser_actions;
        }

        let dropped = bytes.len() - accepted;
        if dropped > 0 {
            let mut parser_actions = std::mem::take(&mut self.parser_actions_scratch);
            self.parser
                .resync_after_truncation_into(&mut parser_actions);
            for action in parser_actions.drain(..) {
                self.apply_action_into(action, events);
            }
            self.parser_actions_scratch = parser_actions;
            events.push(CoreEvent::IngestDegraded {
                reason: IngestDegradeReason::InputFeedTooLarge,
                accepted,
                dropped,
            });
        }
    }

    /// Feed terminal bytes and expose only terminal-response payloads.
    /// Runtime callers use this to avoid depending on the broader core event model.
    pub fn feed_terminal_responses_into(&mut self, bytes: &[u8], responses: &mut Vec<Vec<u8>>) {
        responses.clear();
        if bytes.is_empty() {
            return;
        }

        let mut events = std::mem::take(&mut self.feed_events_scratch);
        self.feed_into(bytes, &mut events);
        for event in events.drain(..) {
            if let CoreEvent::TerminalResponse { data } = event {
                responses.push(data);
            }
        }
        self.feed_events_scratch = events;
    }
}
