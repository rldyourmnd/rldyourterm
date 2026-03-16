// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

pub(crate) fn xterm_modifier_param(shift: bool, alt: bool, ctrl: bool) -> u8 {
    1 + u8::from(shift) + (u8::from(alt) << 1) + (u8::from(ctrl) << 2)
}

pub(crate) fn cursor_key(
    final_byte: u8,
    mod_param: u8,
    has_mod: bool,
    app_cursor: bool,
) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, final_byte as char).into_bytes()
    } else if app_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

pub(crate) fn tilde_modified(n: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[{n};{mod_param}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

pub(crate) fn fkey_ss3_modified(letter: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, letter as char).into_bytes()
    } else {
        vec![0x1b, b'O', letter]
    }
}

pub(crate) fn encode_ctrl_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) - b'a' + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_modifier_param_combinations() {
        assert_eq!(xterm_modifier_param(false, false, false), 1);
        assert_eq!(xterm_modifier_param(true, false, false), 2);
        assert_eq!(xterm_modifier_param(false, true, false), 3);
        assert_eq!(xterm_modifier_param(true, true, false), 4);
        assert_eq!(xterm_modifier_param(false, false, true), 5);
        assert_eq!(xterm_modifier_param(true, false, true), 6);
        assert_eq!(xterm_modifier_param(false, true, true), 7);
        assert_eq!(xterm_modifier_param(true, true, true), 8);
    }

    #[test]
    fn cursor_key_normal_and_application_mode() {
        assert_eq!(cursor_key(b'A', 1, false, false), b"\x1b[A");
        assert_eq!(cursor_key(b'A', 1, false, true), b"\x1bOA");
        assert_eq!(cursor_key(b'A', 5, true, false), b"\x1b[1;5A");
        assert_eq!(cursor_key(b'A', 5, true, true), b"\x1b[1;5A");
    }

    #[test]
    fn tilde_modified_plain_and_with_modifier() {
        assert_eq!(tilde_modified(3, 1, false), b"\x1b[3~");
        assert_eq!(tilde_modified(3, 5, true), b"\x1b[3;5~");
    }

    #[test]
    fn fkey_ss3_plain_and_with_modifier() {
        assert_eq!(fkey_ss3_modified(b'P', 1, false), b"\x1bOP");
        assert_eq!(fkey_ss3_modified(b'P', 5, true), b"\x1b[1;5P");
    }

    #[test]
    fn encode_ctrl_letter_mappings() {
        assert_eq!(encode_ctrl_letter('a'), Some(1));
        assert_eq!(encode_ctrl_letter('c'), Some(3));
        assert_eq!(encode_ctrl_letter('z'), Some(26));
        assert_eq!(encode_ctrl_letter('A'), Some(1));
        assert_eq!(encode_ctrl_letter('1'), None);
    }
}
