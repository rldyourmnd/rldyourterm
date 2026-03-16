// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use super::*;

impl GuiRuntimeApp {
    pub(super) fn handle_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        event_loop: &ActiveEventLoop,
    ) {
        let col = (position.x as usize / CELL_WIDTH) as u16;
        let row = (position.y as usize / CELL_HEIGHT) as u16;

        let grid_cols = self.terminal.grid.width();
        let grid_rows = self.terminal.grid.height();
        let col = col.min(grid_cols.saturating_sub(1));
        let row = row.min(grid_rows.saturating_sub(1));

        let prev_col = self.interaction.mouse_cell_col;
        let prev_row = self.interaction.mouse_cell_row;
        self.interaction.mouse_cell_col = col;
        self.interaction.mouse_cell_row = row;

        if col == prev_col && row == prev_row {
            return;
        }

        // Selection drag: update selection_end while left button held and mouse mode is off.
        // Guard on MouseMode::Off prevents selection state corruption when a TUI app
        // activates mouse reporting after a selection was started.
        if self.interaction.selection_anchor.is_some()
            && self.interaction.mouse_buttons & 1 != 0
            && self.terminal.mouse_mode() == MouseMode::Off
        {
            self.interaction.selection_end = Some((row, col));
            self.terminal.grid.mark_all_dirty();
            self.queue_redraw();
        }

        let mouse_mode = self.terminal.mouse_mode();
        match mouse_mode {
            MouseMode::AnyEvent => {
                let button_code = if self.interaction.mouse_buttons == 0 {
                    35 // no button, motion only
                } else {
                    mouse_button_code(self.interaction.mouse_buttons) + 32
                };
                let encoded =
                    encode_mouse_event(self.terminal.mouse_format(), button_code, col, row, true);
                let _ =
                    self.write_pty_payload(&encoded, event_loop, "failed to write mouse motion");
            }
            MouseMode::ButtonTrack if self.interaction.mouse_buttons != 0 => {
                let button_code = mouse_button_code(self.interaction.mouse_buttons) + 32;
                let encoded =
                    encode_mouse_event(self.terminal.mouse_format(), button_code, col, row, true);
                let _ = self.write_pty_payload(
                    &encoded,
                    event_loop,
                    "failed to write mouse drag motion",
                );
            }
            _ => {}
        }
    }

    pub(super) fn handle_mouse_input(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        event_loop: &ActiveEventLoop,
    ) {
        let button_code = match button {
            winit::event::MouseButton::Left => 0u8,
            winit::event::MouseButton::Middle => 1,
            winit::event::MouseButton::Right => 2,
            _ => return,
        };

        let is_press = state == ElementState::Pressed;
        if is_press {
            self.interaction.mouse_buttons |= 1 << button_code;
        } else {
            self.interaction.mouse_buttons &= !(1 << button_code);
        }

        let mouse_mode = self.terminal.mouse_mode();

        // Selection: left click when mouse mode is off and live view (not scrollback).
        // Scrollback view (viewport_offset > 0) uses screen-relative coordinates that
        // don't map to grid flat-indices, so selection is disabled in that mode.
        if button_code == 0 && mouse_mode == MouseMode::Off && self.interaction.viewport_offset == 0
        {
            if is_press {
                let row = self.interaction.mouse_cell_row;
                let col = self.interaction.mouse_cell_col;
                self.interaction.selection_anchor = Some((row, col));
                self.interaction.selection_end = Some((row, col));
                self.terminal.grid.mark_all_dirty();
                self.queue_redraw();
            } else if self.interaction.selection_anchor.is_some() {
                self.copy_selection_to_clipboard();
                self.clear_selection();
            }
            return;
        }

        if mouse_mode == MouseMode::Off {
            return;
        }

        let encoded = encode_mouse_event(
            self.terminal.mouse_format(),
            button_code,
            self.interaction.mouse_cell_col,
            self.interaction.mouse_cell_row,
            is_press,
        );
        let _ = self.write_pty_payload(&encoded, event_loop, "failed to write mouse button event");
    }

    fn copy_selection_to_clipboard(&mut self) {
        let Some((ar, ac)) = self.interaction.selection_anchor else {
            return;
        };
        let Some((er, ec)) = self.interaction.selection_end else {
            return;
        };

        // Single cell click = no selection, just clear.
        if ar == er && ac == ec {
            self.clear_selection();
            return;
        }

        let cols = self.terminal.grid.width() as u32;
        let start = ar as u32 * cols + ac as u32;
        let end = er as u32 * cols + ec as u32;
        let (lo, hi) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let mut text = String::new();
        let mut prev_row = lo / cols;

        for flat_idx in lo..=hi {
            let row = flat_idx / cols;
            let col = flat_idx % cols;

            if row != prev_row {
                text.push('\n');
                prev_row = row;
            }

            if let Ok(cells) = self.terminal.grid.row_cells(row as u16)
                && let Some(cell) = cells.get(col as usize)
                && cell.width > 0
            {
                text.push(cell.ch);
            }
        }

        // Trim trailing whitespace per line for clean copy.
        let trimmed: String = text
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");

        if !trimmed.is_empty() {
            if let Err(err) = self.clipboard.set_text(&trimmed) {
                debug!(%err, "failed to copy selection to clipboard");
            } else {
                trace!(bytes = trimmed.len(), "selection copied to clipboard");
            }
        }
    }

    pub(super) fn handle_mouse_wheel(
        &mut self,
        delta: winit::event::MouseScrollDelta,
        event_loop: &ActiveEventLoop,
    ) {
        let lines = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => {
                if y > 0.0 {
                    -(y.ceil() as i32)
                } else {
                    (-y).ceil() as i32
                }
            }
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                let cell_height = CELL_HEIGHT as f64;
                if pos.y.abs() < cell_height {
                    return;
                }
                -(pos.y / cell_height) as i32
            }
        };

        if lines == 0 {
            return;
        }

        let mouse_mode = self.terminal.mouse_mode();
        if mouse_mode == MouseMode::Off {
            // Scrollback navigation when mouse reporting is off
            if lines < 0 {
                let page_size = (-lines) as usize;
                let max_offset = self.terminal.scrollback.len();
                self.interaction.viewport_offset =
                    (self.interaction.viewport_offset + page_size).min(max_offset);
            } else {
                let page_size = lines as usize;
                self.interaction.viewport_offset =
                    self.interaction.viewport_offset.saturating_sub(page_size);
            }
            self.terminal.grid.mark_all_dirty();
            self.queue_redraw();
            return;
        }

        // Mouse mode active: encode scroll events
        let button_code: u8 = if lines < 0 { 64 } else { 65 };
        let count = lines.unsigned_abs();
        for _ in 0..count.min(10) {
            let encoded = encode_mouse_event(
                self.terminal.mouse_format(),
                button_code,
                self.interaction.mouse_cell_col,
                self.interaction.mouse_cell_row,
                true,
            );
            let _ =
                self.write_pty_payload(&encoded, event_loop, "failed to write mouse scroll event");
        }
    }
}

fn mouse_button_code(buttons_mask: u8) -> u8 {
    if buttons_mask & 1 != 0 {
        0
    } else if buttons_mask & 2 != 0 {
        1
    } else if buttons_mask & 4 != 0 {
        2
    } else {
        0
    }
}

fn encode_mouse_event(
    format: MouseFormat,
    button_code: u8,
    col: u16,
    row: u16,
    is_press: bool,
) -> Vec<u8> {
    match format {
        MouseFormat::Sgr => {
            let suffix = if is_press { 'M' } else { 'm' };
            format!(
                "\x1b[<{};{};{}{}",
                button_code,
                col.saturating_add(1),
                row.saturating_add(1),
                suffix
            )
            .into_bytes()
        }
        MouseFormat::Normal => {
            let cb = button_code.saturating_add(32);
            let cx = (col.min(222) as u8).saturating_add(33);
            let cy = (row.min(222) as u8).saturating_add(33);
            vec![0x1b, b'[', b'M', cb, cx, cy]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_press_encodes_correctly() {
        let encoded = encode_mouse_event(MouseFormat::Sgr, 0, 5, 10, true);
        assert_eq!(encoded, b"\x1b[<0;6;11M");
    }

    #[test]
    fn sgr_release_encodes_correctly() {
        let encoded = encode_mouse_event(MouseFormat::Sgr, 0, 0, 0, false);
        assert_eq!(encoded, b"\x1b[<0;1;1m");
    }

    #[test]
    fn normal_format_encodes_press() {
        let encoded = encode_mouse_event(MouseFormat::Normal, 0, 0, 0, true);
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, 33, 33]);
    }

    #[test]
    fn scroll_up_button_code_is_64() {
        let encoded = encode_mouse_event(MouseFormat::Sgr, 64, 3, 7, true);
        assert_eq!(encoded, b"\x1b[<64;4;8M");
    }

    #[test]
    fn scroll_down_button_code_is_65() {
        let encoded = encode_mouse_event(MouseFormat::Sgr, 65, 0, 0, true);
        assert_eq!(encoded, b"\x1b[<65;1;1M");
    }

    #[test]
    fn motion_with_button_held_adds_32() {
        let encoded = encode_mouse_event(MouseFormat::Sgr, 32, 10, 20, true);
        assert_eq!(encoded, b"\x1b[<32;11;21M");
    }

    #[test]
    fn mouse_button_code_prefers_left() {
        assert_eq!(mouse_button_code(0b111), 0);
        assert_eq!(mouse_button_code(0b010), 1);
        assert_eq!(mouse_button_code(0b100), 2);
    }

    #[test]
    fn normal_format_clamps_coordinates_to_protocol_maximum() {
        let encoded = encode_mouse_event(MouseFormat::Normal, 0, 300, 400, true);
        let cx = (222_u8).saturating_add(33);
        let cy = (222_u8).saturating_add(33);
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, cx, cy]);
    }

    #[test]
    fn normal_format_preserves_in_range_coordinates() {
        let encoded = encode_mouse_event(MouseFormat::Normal, 0, 100, 50, true);
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, 133, 83]);
    }
}
