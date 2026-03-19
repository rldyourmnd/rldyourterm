// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;
use rldyourterm_interaction::{
    PRIMARY_MOUSE_BUTTON_CODE, active_mouse_button_code, encode_mouse_event,
    mouse_button_code_from_winit,
};

fn encode_alternate_scroll_payload(lines: i32, application_cursor_keys: bool) -> Option<Vec<u8>> {
    let key = if lines < 0 {
        RuntimeKey::Up
    } else if lines > 0 {
        RuntimeKey::Down
    } else {
        return None;
    };

    encode_runtime_key_event(
        RuntimeKeyEvent::new(key, RuntimeKeyModifiers::default()),
        TerminalModeFlags {
            application_cursor_keys,
            ..Default::default()
        },
    )
}

impl GuiRuntimeApp {
    pub(super) fn handle_cursor_moved(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        event_loop: &ActiveEventLoop,
    ) {
        let grid_cols = self.terminal.grid.width();
        let grid_rows = self.terminal.grid.height();
        if !self.interaction.state.update_pointer_cell(
            position,
            CELL_WIDTH,
            CELL_HEIGHT,
            grid_cols.into(),
            grid_rows.into(),
        ) {
            return;
        }

        let pointer = self.interaction.state.pointer();
        let col = pointer.cell_col();
        let row = pointer.cell_row();

        // Guard on MouseMode::Off prevents selection state corruption when a TUI app
        // activates mouse reporting after a selection was started.
        if self.interaction.state.has_selection()
            && pointer.primary_button_held()
            && self.terminal.mouse_mode() == MouseMode::Off
            && self.interaction.state.update_selection_to_pointer()
        {
            self.terminal.grid.mark_all_dirty();
            self.queue_redraw();
        }

        match self.terminal.mouse_mode() {
            MouseMode::AnyEvent => {
                let button_code = if pointer.buttons_mask() == 0 {
                    35
                } else {
                    active_mouse_button_code(pointer.buttons_mask()) + 32
                };
                let encoded =
                    encode_mouse_event(self.terminal.mouse_format(), button_code, col, row, true);
                let _ =
                    self.write_pty_payload(&encoded, event_loop, "failed to write mouse motion");
            }
            MouseMode::ButtonTrack if pointer.buttons_mask() != 0 => {
                let button_code = active_mouse_button_code(pointer.buttons_mask()) + 32;
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
        let Some(button_code) = mouse_button_code_from_winit(button) else {
            return;
        };

        let is_press = state == ElementState::Pressed;
        self.interaction
            .state
            .set_pointer_button_state(button_code, is_press);

        let mouse_mode = self.terminal.mouse_mode();
        if button_code == PRIMARY_MOUSE_BUTTON_CODE
            && mouse_mode == MouseMode::Off
            && self.interaction.state.viewport_offset() == 0
        {
            if is_press {
                self.interaction.state.begin_selection_at_pointer();
                self.terminal.grid.mark_all_dirty();
                self.queue_redraw();
            } else if self.interaction.state.has_selection() {
                self.copy_selection_to_clipboard();
                self.clear_selection();
            }
            return;
        }

        if mouse_mode == MouseMode::Off {
            return;
        }

        if mouse_mode == MouseMode::X10 && !is_press {
            return;
        }

        let pointer = self.interaction.state.pointer();
        let encoded = encode_mouse_event(
            self.terminal.mouse_format(),
            button_code,
            pointer.cell_col(),
            pointer.cell_row(),
            is_press,
        );
        let _ = self.write_pty_payload(&encoded, event_loop, "failed to write mouse button event");
    }

    fn copy_selection_to_clipboard(&mut self) {
        let selection = self.interaction.state.selection();
        let Some((lo, hi)) = selection.ordered_flat_range(self.terminal.grid.width().into()) else {
            return;
        };

        if selection.is_single_cell() {
            self.clear_selection();
            return;
        }

        let cols = self.terminal.grid.width() as u32;
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
                cell.append_text_to(&mut text);
            }
        }

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

        if self.terminal.mouse_mode() == MouseMode::Off {
            if self.terminal.alternate_scroll_enabled() && self.terminal.alternate_screen_active() {
                if let Some(encoded) = encode_alternate_scroll_payload(
                    lines,
                    self.terminal.application_cursor_keys_enabled(),
                ) {
                    for _ in 0..lines.unsigned_abs().min(10) {
                        let _ = self.write_pty_payload(
                            &encoded,
                            event_loop,
                            "failed to write alternate scroll key event",
                        );
                    }
                }
                return;
            }

            let max_offset = self.terminal.scrollback.len();
            self.interaction
                .state
                .scroll_viewport_by_lines(lines, max_offset);
            self.terminal.grid.mark_all_dirty();
            self.queue_redraw();
            return;
        }

        let button_code: u8 = if lines < 0 { 64 } else { 65 };
        let count = lines.unsigned_abs();
        let pointer = self.interaction.state.pointer();
        for _ in 0..count.min(10) {
            let encoded = encode_mouse_event(
                self.terminal.mouse_format(),
                button_code,
                pointer.cell_col(),
                pointer.cell_row(),
                true,
            );
            let _ =
                self.write_pty_payload(&encoded, event_loop, "failed to write mouse scroll event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_alternate_scroll_payload;

    #[test]
    fn alternate_scroll_payload_uses_csi_cursor_keys_by_default() {
        assert_eq!(
            encode_alternate_scroll_payload(-1, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode_alternate_scroll_payload(1, false),
            Some(b"\x1b[B".to_vec())
        );
    }

    #[test]
    fn alternate_scroll_payload_uses_application_cursor_mode_when_enabled() {
        assert_eq!(
            encode_alternate_scroll_payload(-1, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode_alternate_scroll_payload(1, true),
            Some(b"\x1bOB".to_vec())
        );
    }
}
