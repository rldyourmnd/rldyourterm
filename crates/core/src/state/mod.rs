// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

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
    grid::{Attrs, Cell, Color, DEFAULT_BG, DEFAULT_FG, Grid, Palette, TerminalTheme},
    parser::{Parser, ParserAction},
    scrollback::Scrollback,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseMode {
    #[default]
    Off,
    /// Mode 9: X10 compatibility mouse - report button press only.
    X10,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SavedCursorState {
    pub(super) cursor: Cursor,
    pub(super) pen: Attrs,
    pub(super) origin_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScreenModeState {
    pub(super) bracketed_paste: bool,
    pub(super) application_keypad_mode: bool,
    pub(super) application_cursor_keys: bool,
    pub(super) auto_wrap: bool,
    pub(super) reverse_video: bool,
    pub(super) origin_mode: bool,
    pub(super) grapheme_cluster_mode: bool,
    pub(super) mouse_mode: MouseMode,
    pub(super) mouse_format: MouseFormat,
    pub(super) alternate_scroll: bool,
    pub(super) cursor_blink: bool,
    pub(super) cursor_shape: u8,
    pub(super) meta_sends_escape: bool,
    pub(super) alt_sends_escape: bool,
    pub(super) focus_reporting: bool,
    pub(super) synchronized_output: bool,
    pub(super) kitty_keyboard_stack: Vec<u16>,
}

#[derive(Debug)]
pub(super) struct AlternateScreenState {
    pub(super) grid: Grid,
    pub(super) cursor: Cursor,
    pub(super) pen: Attrs,
    pub(super) scrollback: Scrollback,
    pub(super) saved_cursor: Option<SavedCursorState>,
    pub(super) scroll_region: Option<(u16, u16)>,
    pub(super) screen_modes: ScreenModeState,
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
    pub(super) saved_cursor: Option<SavedCursorState>,
    pub(super) scroll_region: Option<(u16, u16)>,
    pub(super) alternate_screen: Option<Box<AlternateScreenState>>,
    pub(super) window_title: String,
    pub(super) cwd: String,
    pub(super) pending_clipboard: Option<(char, String)>,
    pub(super) pending_bell: bool,
    pub(super) palette: Palette,
    pub(super) default_fg: (u8, u8, u8),
    pub(super) default_bg: (u8, u8, u8),
    pub(super) bracketed_paste: bool,
    pub(super) application_keypad_mode: bool,
    pub(super) application_cursor_keys: bool,
    pub(super) auto_wrap: bool,
    pub(super) reverse_video: bool,
    pub(super) origin_mode: bool,
    pub(super) grapheme_cluster_mode: bool,
    pub(super) mouse_mode: MouseMode,
    pub(super) mouse_format: MouseFormat,
    pub(super) alternate_scroll: bool,
    pub(super) cursor_blink: bool,
    pub(super) cursor_shape: u8,
    pub(super) meta_sends_escape: bool,
    pub(super) alt_sends_escape: bool,
    pub(super) focus_reporting: bool,
    pub(super) synchronized_output: bool,
    pub(super) kitty_keyboard_stack: Vec<u16>,
    pub(super) last_printed_char: Option<char>,
    pub(super) tab_stops: Vec<bool>,
    pub(super) current_hyperlink: Option<String>,
    pub(super) viewport_pixels: Option<(u32, u32)>,
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
            pending_bell: false,
            palette: Palette::default(),
            default_fg: DEFAULT_FG,
            default_bg: DEFAULT_BG,
            bracketed_paste: false,
            application_keypad_mode: false,
            application_cursor_keys: false,
            auto_wrap: true,
            reverse_video: false,
            origin_mode: false,
            grapheme_cluster_mode: false,
            mouse_mode: MouseMode::Off,
            mouse_format: MouseFormat::Normal,
            alternate_scroll: false,
            cursor_blink: false,
            cursor_shape: 0,
            meta_sends_escape: true,
            alt_sends_escape: false,
            focus_reporting: false,
            synchronized_output: false,
            kitty_keyboard_stack: Vec::new(),
            last_printed_char: None,
            tab_stops: Self::default_tab_stops(width),
            current_hyperlink: None,
            viewport_pixels: None,
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

    /// Returns and clears the pending bell flag set by BEL (0x07).
    pub fn take_pending_bell(&mut self) -> bool {
        std::mem::replace(&mut self.pending_bell, false)
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

    pub fn alternate_screen_active(&self) -> bool {
        self.alternate_screen.is_some()
    }

    pub fn reverse_video_enabled(&self) -> bool {
        self.reverse_video
    }

    pub fn alternate_scroll_enabled(&self) -> bool {
        self.alternate_scroll
    }

    pub fn meta_sends_escape_enabled(&self) -> bool {
        self.meta_sends_escape
    }

    pub fn alt_sends_escape_enabled(&self) -> bool {
        self.alt_sends_escape
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

    pub fn application_cursor_keys_enabled(&self) -> bool {
        self.application_cursor_keys
    }

    pub fn application_keypad_mode_enabled(&self) -> bool {
        self.application_keypad_mode
    }

    pub fn kitty_keyboard_flags(&self) -> u16 {
        self.kitty_keyboard_stack.last().copied().unwrap_or(0)
    }

    pub fn palette_color(&self, index: u8) -> u32 {
        self.palette.get(index)
    }

    pub fn resolve_color(&self, color: Color, default: (u8, u8, u8)) -> u32 {
        self.palette.resolve_color(color, default)
    }

    pub fn resolve_cell_colors(&self, attrs: &Attrs) -> (u32, u32) {
        let mut fg = self.resolve_color(attrs.fg, self.default_fg);
        let mut bg = self.resolve_color(attrs.bg, self.default_bg);

        if attrs.dim() {
            let r = ((fg >> 16) & 0xff) as u8 / 2;
            let g = ((fg >> 8) & 0xff) as u8 / 2;
            let b = (fg & 0xff) as u8 / 2;
            fg = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }

        if self.reverse_video ^ attrs.inverse() {
            std::mem::swap(&mut fg, &mut bg);
        }

        if attrs.hidden() {
            fg = bg;
        }

        (fg, bg)
    }

    pub fn apply_theme(&mut self, theme: &TerminalTheme) {
        self.default_fg = theme.default_fg;
        self.default_bg = theme.default_bg;
        self.palette.set_base_colors(theme.palette);
        self.grid.mark_all_dirty();
        if let Some(alternate_screen) = self.alternate_screen.as_mut() {
            alternate_screen.grid.mark_all_dirty();
        }
    }

    pub(super) fn set_palette_color(&mut self, index: u8, rgb: (u8, u8, u8)) {
        self.palette.set_rgb(index, rgb);
    }

    pub(super) fn reset_palette_color(&mut self, index: u8) {
        self.palette.reset_color(index);
    }

    pub(super) fn reset_palette(&mut self) {
        self.palette.reset_all();
    }

    pub(super) fn capture_screen_modes(&self) -> ScreenModeState {
        ScreenModeState {
            bracketed_paste: self.bracketed_paste,
            application_keypad_mode: self.application_keypad_mode,
            application_cursor_keys: self.application_cursor_keys,
            auto_wrap: self.auto_wrap,
            reverse_video: self.reverse_video,
            origin_mode: self.origin_mode,
            grapheme_cluster_mode: self.grapheme_cluster_mode,
            mouse_mode: self.mouse_mode,
            mouse_format: self.mouse_format,
            alternate_scroll: self.alternate_scroll,
            cursor_blink: self.cursor_blink,
            cursor_shape: self.cursor_shape,
            meta_sends_escape: self.meta_sends_escape,
            alt_sends_escape: self.alt_sends_escape,
            focus_reporting: self.focus_reporting,
            synchronized_output: self.synchronized_output,
            kitty_keyboard_stack: self.kitty_keyboard_stack.clone(),
        }
    }

    pub(super) fn restore_screen_modes(&mut self, modes: ScreenModeState) {
        self.bracketed_paste = modes.bracketed_paste;
        self.application_keypad_mode = modes.application_keypad_mode;
        self.application_cursor_keys = modes.application_cursor_keys;
        self.auto_wrap = modes.auto_wrap;
        self.reverse_video = modes.reverse_video;
        self.origin_mode = modes.origin_mode;
        self.grapheme_cluster_mode = modes.grapheme_cluster_mode;
        self.mouse_mode = modes.mouse_mode;
        self.mouse_format = modes.mouse_format;
        self.alternate_scroll = modes.alternate_scroll;
        self.cursor_blink = modes.cursor_blink;
        self.cursor_shape = modes.cursor_shape;
        self.meta_sends_escape = modes.meta_sends_escape;
        self.alt_sends_escape = modes.alt_sends_escape;
        self.focus_reporting = modes.focus_reporting;
        self.synchronized_output = modes.synchronized_output;
        self.kitty_keyboard_stack = modes.kitty_keyboard_stack;
    }

    pub(super) fn blank_cell(&self) -> Cell {
        Cell::blank_with_bg(self.pen.bg)
    }

    #[cfg(test)]
    pub(crate) fn auto_wrap_enabled(&self) -> bool {
        self.auto_wrap
    }

    /// Returns the currently active hyperlink URI, if any (set via OSC 8).
    #[cfg(test)]
    pub(crate) fn current_hyperlink(&self) -> Option<&str> {
        self.current_hyperlink.as_deref()
    }

    /// Checks whether a DEC private mode is recognized and its current state.
    /// Returns `Some(true)` if set, `Some(false)` if reset, `None` if unrecognized.
    pub(super) fn is_private_mode_set(&self, mode: u16) -> Option<bool> {
        match mode {
            1 => Some(self.application_cursor_keys),
            5 => Some(self.reverse_video),
            6 => Some(self.origin_mode),
            7 => Some(self.auto_wrap),
            9 => Some(self.mouse_mode == MouseMode::X10),
            2027 => Some(self.grapheme_cluster_mode),
            12 => Some(self.cursor_blink),
            25 => Some(self.cursor.visible),
            47 | 1047 => Some(self.alternate_screen.is_some()),
            1000 => Some(self.mouse_mode == MouseMode::Basic),
            1002 => Some(self.mouse_mode == MouseMode::ButtonTrack),
            1003 => Some(self.mouse_mode == MouseMode::AnyEvent),
            1004 => Some(self.focus_reporting),
            1006 => Some(self.mouse_format == MouseFormat::Sgr),
            1007 => Some(self.alternate_scroll),
            1036 => Some(!self.meta_sends_escape),
            1039 => Some(self.alt_sends_escape),
            1048 => Some(self.saved_cursor.is_some()),
            1049 => Some(self.alternate_screen.is_some()),
            2004 => Some(self.bracketed_paste),
            2026 => Some(self.synchronized_output),
            _ => None,
        }
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

    pub fn set_viewport_pixels(&mut self, width: u32, height: u32) {
        self.viewport_pixels = Some((width, height));
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

    /// Feed terminal bytes and expose all core events (bell, title change,
    /// hyperlink, terminal responses, etc.). Use this when embedding the
    /// terminal engine in a custom UI that needs full event visibility.
    pub fn feed_all_events_into(&mut self, bytes: &[u8], events: &mut Vec<CoreEvent>) {
        self.feed_into(bytes, events);
    }
}
