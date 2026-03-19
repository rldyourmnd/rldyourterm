// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_services::terminal::MouseFormat;
use winit::event::MouseButton;

pub const PRIMARY_MOUSE_BUTTON_CODE: u8 = 0;

pub fn mouse_button_code_from_winit(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

pub fn active_mouse_button_code(buttons_mask: u8) -> u8 {
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

pub fn encode_mouse_event(
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
        assert_eq!(active_mouse_button_code(0b111), 0);
        assert_eq!(active_mouse_button_code(0b010), 1);
        assert_eq!(active_mouse_button_code(0b100), 2);
    }

    #[test]
    fn normal_format_clamps_coordinates_to_protocol_maximum() {
        let encoded = encode_mouse_event(MouseFormat::Normal, 0, 300, 400, true);
        let cx = 222_u8.saturating_add(33);
        let cy = 222_u8.saturating_add(33);
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, cx, cy]);
    }

    #[test]
    fn normal_format_preserves_in_range_coordinates() {
        let encoded = encode_mouse_event(MouseFormat::Normal, 0, 100, 50, true);
        assert_eq!(encoded, vec![0x1b, b'[', b'M', 32, 133, 83]);
    }

    #[test]
    fn button_mapping_follows_winit_buttons() {
        assert_eq!(mouse_button_code_from_winit(MouseButton::Left), Some(0));
        assert_eq!(mouse_button_code_from_winit(MouseButton::Middle), Some(1));
        assert_eq!(mouse_button_code_from_winit(MouseButton::Right), Some(2));
        assert_eq!(mouse_button_code_from_winit(MouseButton::Back), None);
    }
}
