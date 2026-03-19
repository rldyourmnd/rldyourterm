// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::fmt::Write as _;

use crate::{
    events::CoreEvent,
    grid::{Attrs, Color, DEFAULT_BG, DEFAULT_FG, UnderlineStyle},
    parser::{ParserAction, StatusStringRequest},
};

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
            ParserAction::Bell => {
                self.pending_bell = true;
                events.push(CoreEvent::Bell);
            }
            ParserAction::Backspace => self.apply_backspace(events),
            ParserAction::Tab => self.apply_tab(events),
            ParserAction::CursorBackwardTab(count) => self.apply_cursor_backward_tab(count, events),
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
            ParserAction::SetGraphicsRendition(params) => self.apply_sgr(&params),
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
            ParserAction::SetOriginMode(enabled) => {
                self.origin_mode = enabled;
                if enabled {
                    // DECOM set: cursor moves to home position within scroll region
                    let top = self.scroll_region.map_or(0, |(t, _)| t);
                    self.cursor.row = top;
                    self.cursor.col = 0;
                }
            }
            ParserAction::SetGraphemeClusterMode(enabled) => {
                self.grapheme_cluster_mode = enabled;
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
            ParserAction::SetReverseVideo(enabled) => {
                self.reverse_video = enabled;
            }
            ParserAction::SendPrimaryDA => {
                events.push(CoreEvent::TerminalResponse {
                    data: b"\x1b[?1;2c".to_vec(),
                });
            }
            ParserAction::SendSecondaryDA => {
                events.push(CoreEvent::TerminalResponse {
                    data: b"\x1b[>0;0;0c".to_vec(),
                });
            }
            ParserAction::SendXtversion => {
                events.push(CoreEvent::TerminalResponse {
                    data: b"\x1bP>|rldyourterm 0.1.0\x1b\\".to_vec(),
                });
            }
            ParserAction::SendWindowSizeChars => {
                let rows = self.grid.height();
                let cols = self.grid.width();
                events.push(CoreEvent::TerminalResponse {
                    data: format!("\x1b[8;{rows};{cols}t").into_bytes(),
                });
            }
            ParserAction::SendWindowSizePixels => {
                if let Some((width, height)) = self.viewport_pixels {
                    events.push(CoreEvent::TerminalResponse {
                        data: format!("\x1b[4;{height};{width}t").into_bytes(),
                    });
                }
            }
            ParserAction::RequestModeReport(mode) => {
                let setting = match self.is_private_mode_set(mode) {
                    Some(true) => 1u8,
                    Some(false) => 2u8,
                    None => 0u8,
                };
                events.push(CoreEvent::TerminalResponse {
                    data: format!("\x1b[?{mode};{setting}$y").into_bytes(),
                });
            }
            ParserAction::SendDeviceStatusReport => {
                let row = if self.origin_mode {
                    self.cursor
                        .row
                        .saturating_sub(self.scroll_top())
                        .saturating_add(1)
                } else {
                    self.cursor.row.saturating_add(1)
                };
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
            ParserAction::RepeatLastChar(n) => self.apply_repeat_last_char(n, events),
            ParserAction::HorizontalTabSet => self.apply_horizontal_tab_set(),
            ParserAction::TabClear(mode) => self.apply_tab_clear(mode),
            ParserAction::SetMouseMode(mode) => {
                self.mouse_mode = mode;
            }
            ParserAction::SetMouseFormat(format) => {
                self.mouse_format = format;
            }
            ParserAction::SetAlternateScroll(enabled) => {
                self.alternate_scroll = enabled;
            }
            ParserAction::AlternateScreenEnterSimple => {
                self.apply_alternate_screen_enter_simple();
                events.push(CoreEvent::AlternateScreenEntered);
            }
            ParserAction::AlternateScreenLeaveSimple => {
                self.apply_alternate_screen_leave();
                events.push(CoreEvent::AlternateScreenLeft);
            }
            ParserAction::CursorSavePositionDec => self.apply_cursor_save(),
            ParserAction::CursorRestorePositionDec => self.apply_cursor_restore(events),
            ParserAction::SetCursorBlink(enabled) => {
                self.cursor_blink = enabled;
            }
            ParserAction::SetMetaSendsEscape(enabled) => {
                self.meta_sends_escape = enabled;
            }
            ParserAction::SetAltSendsEscape(enabled) => {
                self.alt_sends_escape = enabled;
            }
            ParserAction::SetFocusReporting(enabled) => {
                self.focus_reporting = enabled;
            }
            ParserAction::SetSynchronizedOutput(enabled) => {
                self.synchronized_output = enabled;
            }
            ParserAction::SetCursorShape(shape) => {
                self.cursor_shape = shape;
            }
            ParserAction::SetCurrentWorkingDirectory(path) => {
                if self.cwd != path {
                    self.cwd = path.clone();
                    events.push(CoreEvent::CurrentWorkingDirectoryChanged { path });
                }
            }
            ParserAction::ClipboardSet {
                selection,
                base64_data,
            } => {
                self.pending_clipboard = Some((selection, base64_data));
            }
            ParserAction::HyperlinkStart { uri } => {
                self.current_hyperlink = Some(uri);
            }
            ParserAction::HyperlinkEnd => {
                self.current_hyperlink = None;
            }
            ParserAction::SetPaletteColor { index, rgb } => {
                self.set_palette_color(index, rgb);
            }
            ParserAction::QueryPaletteColor(index) => {
                events.push(CoreEvent::TerminalResponse {
                    data: osc_palette_response(index, self.palette_color(index)),
                });
            }
            ParserAction::ResetPaletteColor(index) => {
                self.reset_palette_color(index);
            }
            ParserAction::ResetPalette => {
                self.reset_palette();
            }
            ParserAction::QueryForegroundColor => {
                events.push(CoreEvent::TerminalResponse {
                    data: osc_dynamic_color_response(10, DEFAULT_FG),
                });
            }
            ParserAction::QueryBackgroundColor => {
                events.push(CoreEvent::TerminalResponse {
                    data: osc_dynamic_color_response(11, DEFAULT_BG),
                });
            }
            ParserAction::RequestStatusString(request) => {
                events.push(CoreEvent::TerminalResponse {
                    data: decrqss_response(self, request),
                });
            }
            ParserAction::ShellMarker(kind) => {
                events.push(CoreEvent::ShellMarkerReceived { kind });
            }
            ParserAction::PushKittyKeyboardMode(flags) => {
                if self.kitty_keyboard_stack.len() < 16 {
                    self.kitty_keyboard_stack.push(flags);
                }
            }
            ParserAction::PopKittyKeyboardMode(count) => {
                for _ in 0..count {
                    if self.kitty_keyboard_stack.pop().is_none() {
                        break;
                    }
                }
            }
            ParserAction::QueryKittyKeyboardMode => {
                let flags = self.kitty_keyboard_stack.last().copied().unwrap_or(0);
                events.push(CoreEvent::TerminalResponse {
                    data: format!("\x1b[?{flags}u").into_bytes(),
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

fn osc_dynamic_color_response(code: u16, rgb: (u8, u8, u8)) -> Vec<u8> {
    format_osc_rgb_response(code, rgb.0, rgb.1, rgb.2)
}

fn decrqss_response(state: &TerminalState, request: StatusStringRequest) -> Vec<u8> {
    let Some(payload) = decrqss_payload(state, request) else {
        return b"\x1bP0$r\x1b\\".to_vec();
    };

    format!("\x1bP1$r{payload}\x1b\\").into_bytes()
}

fn decrqss_payload(state: &TerminalState, request: StatusStringRequest) -> Option<String> {
    match request {
        StatusStringRequest::Sgr => Some(format_sgr_status_string(&state.pen)),
        StatusStringRequest::CursorStyle => Some(format!("{} q", state.cursor_shape)),
        StatusStringRequest::ScrollRegion => Some(format!(
            "{};{}r",
            state.scroll_top().saturating_add(1),
            state.scroll_bottom().saturating_add(1)
        )),
        StatusStringRequest::Unsupported => None,
    }
}

fn format_sgr_status_string(attrs: &Attrs) -> String {
    let mut params = Vec::new();

    if attrs.bold() {
        params.push(String::from("1"));
    }
    if attrs.dim() {
        params.push(String::from("2"));
    }
    if attrs.italic() {
        params.push(String::from("3"));
    }
    match attrs.underline_style() {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => params.push(String::from("4")),
        UnderlineStyle::Double => params.push(String::from("4:2")),
        UnderlineStyle::Curly => params.push(String::from("4:3")),
        UnderlineStyle::Dotted => params.push(String::from("4:4")),
        UnderlineStyle::Dashed => params.push(String::from("4:5")),
    }
    if attrs.blink() {
        params.push(String::from("5"));
    }
    if attrs.inverse() {
        params.push(String::from("7"));
    }
    if attrs.hidden() {
        params.push(String::from("8"));
    }
    if attrs.strikethrough() {
        params.push(String::from("9"));
    }
    if attrs.overline() {
        params.push(String::from("53"));
    }

    push_sgr_color(&mut params, attrs.fg, false);
    push_sgr_color(&mut params, attrs.bg, true);
    push_sgr_underline_color(&mut params, attrs.underline_color);

    if params.is_empty() {
        params.push(String::from("0"));
    }

    format!("{}m", params.join(";"))
}

fn push_sgr_color(params: &mut Vec<String>, color: Color, background: bool) {
    match color {
        Color::Default => {}
        Color::Indexed(index @ 0..=7) => {
            let base = if background { 40 } else { 30 };
            params.push((base + u16::from(index)).to_string());
        }
        Color::Indexed(index @ 8..=15) => {
            let base = if background { 100 } else { 90 };
            params.push((base + u16::from(index - 8)).to_string());
        }
        Color::Indexed(index) => {
            let base = if background { 48 } else { 38 };
            params.push(format!("{base};5;{index}"));
        }
        Color::Rgb(red, green, blue) => {
            let base = if background { 48 } else { 38 };
            params.push(format!("{base};2;{red};{green};{blue}"));
        }
    }
}

fn push_sgr_underline_color(params: &mut Vec<String>, color: Color) {
    match color {
        Color::Default => {}
        Color::Indexed(index) => params.push(format!("58;5;{index}")),
        Color::Rgb(red, green, blue) => {
            params.push(format!("58;2;{red};{green};{blue}"));
        }
    }
}

fn osc_palette_response(index: u8, packed_rgb: u32) -> Vec<u8> {
    let (red, green, blue) = unpack_rgb(packed_rgb);
    let mut response = String::from("\x1b]4;");
    let _ = write!(
        response,
        "{index};rgb:{:04x}/{:04x}/{:04x}",
        widen_channel(red),
        widen_channel(green),
        widen_channel(blue)
    );
    response.push_str("\x1b\\");
    response.into_bytes()
}

fn format_osc_rgb_response(code: u16, red: u8, green: u8, blue: u8) -> Vec<u8> {
    format!(
        "\x1b]{code};rgb:{:04x}/{:04x}/{:04x}\x1b\\",
        widen_channel(red),
        widen_channel(green),
        widen_channel(blue)
    )
    .into_bytes()
}

const fn unpack_rgb(color: u32) -> (u8, u8, u8) {
    ((color >> 16) as u8, (color >> 8) as u8, color as u8)
}

const fn widen_channel(channel: u8) -> u16 {
    (channel as u16) * 0x0101
}
