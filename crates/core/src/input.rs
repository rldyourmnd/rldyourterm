// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKey {
    Character(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    Delete,
    Insert,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuntimeKeyModifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    pub super_key: bool,
}

impl RuntimeKeyModifiers {
    fn xterm_modifier_param(self) -> u8 {
        xterm_modifier_param(self.shift, self.alt, self.control)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeKeyEvent {
    pub key: RuntimeKey,
    pub modifiers: RuntimeKeyModifiers,
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalModeFlags {
    pub application_cursor_keys: bool,
    pub kitty_keyboard_flags: u16,
    pub meta_sends_escape: bool,
    pub alt_sends_escape: bool,
}

impl Default for TerminalModeFlags {
    fn default() -> Self {
        Self {
            application_cursor_keys: false,
            kitty_keyboard_flags: 0,
            meta_sends_escape: true,
            alt_sends_escape: false,
        }
    }
}

impl TerminalModeFlags {
    pub fn alt_modifier_sends_escape(self) -> bool {
        // The runtime input model exposes Alt but not a distinct Meta modifier bit.
        // Treat Alt as the shared Meta/Alt-like modifier and let the terminal modes
        // decide whether that modifier should prefix ESC.
        self.meta_sends_escape || self.alt_sends_escape
    }
}

pub fn encode_runtime_key_event(
    key_event: RuntimeKeyEvent,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    if modes.kitty_keyboard_flags > 0 {
        return encode_kitty_key_event(key_event, modes);
    }
    encode_legacy_key_event(key_event, modes)
}

fn encode_legacy_key_event(
    key_event: RuntimeKeyEvent,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    let modifiers = key_event.modifiers;
    let mod_param = modifiers.xterm_modifier_param();
    let has_mod = mod_param > 1;
    let app_cursor = modes.application_cursor_keys;

    match key_event.key {
        RuntimeKey::Enter => Some(vec![b'\r']),
        RuntimeKey::Backspace
            if modifiers.alt && !modifiers.control && modes.alt_modifier_sends_escape() =>
        {
            Some(b"\x1b\x7f".to_vec())
        }
        RuntimeKey::Backspace => Some(vec![0x7f]),
        RuntimeKey::Tab if modifiers.shift && !modifiers.alt && !modifiers.control => {
            Some(b"\x1b[Z".to_vec())
        }
        RuntimeKey::Tab if !modifiers.alt && !modifiers.control => Some(vec![b'\t']),
        RuntimeKey::Escape => Some(vec![0x1b]),
        RuntimeKey::Up => Some(cursor_key(b'A', mod_param, has_mod, app_cursor)),
        RuntimeKey::Down => Some(cursor_key(b'B', mod_param, has_mod, app_cursor)),
        RuntimeKey::Right => Some(cursor_key(b'C', mod_param, has_mod, app_cursor)),
        RuntimeKey::Left => Some(cursor_key(b'D', mod_param, has_mod, app_cursor)),
        RuntimeKey::Home => Some(cursor_key(b'H', mod_param, has_mod, app_cursor)),
        RuntimeKey::End => Some(cursor_key(b'F', mod_param, has_mod, app_cursor)),
        RuntimeKey::Delete => Some(tilde_modified(3, mod_param, has_mod)),
        RuntimeKey::Insert => Some(tilde_modified(2, mod_param, has_mod)),
        RuntimeKey::PageUp => Some(tilde_modified(5, mod_param, has_mod)),
        RuntimeKey::PageDown => Some(tilde_modified(6, mod_param, has_mod)),
        RuntimeKey::F(1) => Some(fkey_ss3_modified(b'P', mod_param, has_mod)),
        RuntimeKey::F(2) => Some(fkey_ss3_modified(b'Q', mod_param, has_mod)),
        RuntimeKey::F(3) => Some(fkey_ss3_modified(b'R', mod_param, has_mod)),
        RuntimeKey::F(4) => Some(fkey_ss3_modified(b'S', mod_param, has_mod)),
        RuntimeKey::F(5) => Some(tilde_modified(15, mod_param, has_mod)),
        RuntimeKey::F(6) => Some(tilde_modified(17, mod_param, has_mod)),
        RuntimeKey::F(7) => Some(tilde_modified(18, mod_param, has_mod)),
        RuntimeKey::F(8) => Some(tilde_modified(19, mod_param, has_mod)),
        RuntimeKey::F(9) => Some(tilde_modified(20, mod_param, has_mod)),
        RuntimeKey::F(10) => Some(tilde_modified(21, mod_param, has_mod)),
        RuntimeKey::F(11) => Some(tilde_modified(23, mod_param, has_mod)),
        RuntimeKey::F(12) => Some(tilde_modified(24, mod_param, has_mod)),
        RuntimeKey::F(13) => Some(tilde_modified(25, mod_param, has_mod)),
        RuntimeKey::F(14) => Some(tilde_modified(26, mod_param, has_mod)),
        RuntimeKey::F(15) => Some(tilde_modified(28, mod_param, has_mod)),
        RuntimeKey::F(16) => Some(tilde_modified(29, mod_param, has_mod)),
        RuntimeKey::F(17) => Some(tilde_modified(31, mod_param, has_mod)),
        RuntimeKey::F(18) => Some(tilde_modified(32, mod_param, has_mod)),
        RuntimeKey::F(19) => Some(tilde_modified(33, mod_param, has_mod)),
        RuntimeKey::F(20) => Some(tilde_modified(34, mod_param, has_mod)),
        RuntimeKey::Character(ch) if modifiers.control => {
            encode_ctrl_letter(ch).map(|code| vec![code])
        }
        RuntimeKey::Character(ch)
            if modifiers.alt
                && !modifiers.control
                && !modifiers.super_key
                && modes.alt_modifier_sends_escape() =>
        {
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(ch.to_string().as_bytes());
            Some(bytes)
        }
        RuntimeKey::Character(ch) if !modifiers.control && !modifiers.super_key => {
            Some(ch.to_string().into_bytes())
        }
        RuntimeKey::Character(_) => None,
        RuntimeKey::Tab => None,
        RuntimeKey::F(_) => None,
    }
}

fn encode_kitty_key_event(key_event: RuntimeKeyEvent, modes: TerminalModeFlags) -> Option<Vec<u8>> {
    let modifiers = key_event.modifiers;
    let kitty_mod = kitty_modifier_param(modifiers);
    let has_mod = kitty_mod > 1;

    match key_event.key {
        RuntimeKey::Enter => Some(kitty_csi_u(13, kitty_mod, has_mod)),
        RuntimeKey::Tab => Some(kitty_csi_u(9, kitty_mod, has_mod)),
        RuntimeKey::Backspace => Some(kitty_csi_u(127, kitty_mod, has_mod)),
        RuntimeKey::Escape => Some(kitty_csi_u(27, kitty_mod, has_mod)),
        RuntimeKey::Character(ch) => Some(kitty_csi_u(u32::from(ch), kitty_mod, has_mod)),
        RuntimeKey::Up
        | RuntimeKey::Down
        | RuntimeKey::Right
        | RuntimeKey::Left
        | RuntimeKey::Home
        | RuntimeKey::End
        | RuntimeKey::Delete
        | RuntimeKey::Insert
        | RuntimeKey::PageUp
        | RuntimeKey::PageDown
        | RuntimeKey::F(_) => encode_legacy_key_event(key_event, modes),
    }
}

fn xterm_modifier_param(shift: bool, alt: bool, ctrl: bool) -> u8 {
    1 + u8::from(shift) + (u8::from(alt) << 1) + (u8::from(ctrl) << 2)
}

fn cursor_key(final_byte: u8, mod_param: u8, has_mod: bool, app_cursor: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, final_byte as char).into_bytes()
    } else if app_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

fn tilde_modified(n: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[{n};{mod_param}~").into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

fn fkey_ss3_modified(letter: u8, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[1;{}{}", mod_param, letter as char).into_bytes()
    } else {
        vec![0x1b, b'O', letter]
    }
}

fn encode_ctrl_letter(ch: char) -> Option<u8> {
    let lower = ch.to_ascii_lowercase();
    if lower.is_ascii_lowercase() {
        Some((lower as u8) - b'a' + 1)
    } else {
        match ch {
            '@' => Some(0x00),
            '[' => Some(0x1B),
            '\\' => Some(0x1C),
            ']' => Some(0x1D),
            '^' => Some(0x1E),
            '_' => Some(0x1F),
            _ => None,
        }
    }
}

fn kitty_modifier_param(modifiers: RuntimeKeyModifiers) -> u8 {
    1 + u8::from(modifiers.shift)
        + (u8::from(modifiers.alt) << 1)
        + (u8::from(modifiers.control) << 2)
        + (u8::from(modifiers.super_key) << 3)
}

fn kitty_csi_u(codepoint: u32, mod_param: u8, has_mod: bool) -> Vec<u8> {
    if has_mod {
        format!("\x1b[{codepoint};{mod_param}u").into_bytes()
    } else {
        format!("\x1b[{codepoint}u").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeKey, RuntimeKeyEvent, RuntimeKeyModifiers, TerminalModeFlags, cursor_key,
        encode_ctrl_letter, encode_runtime_key_event, fkey_ss3_modified, tilde_modified,
        xterm_modifier_param,
    };

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

    #[test]
    fn encode_ctrl_non_letter_special_chars() {
        assert_eq!(encode_ctrl_letter('@'), Some(0x00));
        assert_eq!(encode_ctrl_letter('['), Some(0x1B));
        assert_eq!(encode_ctrl_letter('\\'), Some(0x1C));
        assert_eq!(encode_ctrl_letter(']'), Some(0x1D));
        assert_eq!(encode_ctrl_letter('^'), Some(0x1E));
        assert_eq!(encode_ctrl_letter('_'), Some(0x1F));
    }

    #[test]
    fn application_cursor_keys_sends_ss3_for_arrows() {
        let modes = TerminalModeFlags {
            application_cursor_keys: true,
            ..Default::default()
        };
        let no_mods = RuntimeKeyModifiers {
            shift: false,
            alt: false,
            control: false,
            super_key: false,
        };
        let ev = RuntimeKeyEvent {
            key: RuntimeKey::Up,
            modifiers: no_mods,
        };
        assert_eq!(
            encode_runtime_key_event(ev, modes),
            Some(b"\x1bOA".to_vec())
        );

        let ev = RuntimeKeyEvent {
            key: RuntimeKey::Down,
            modifiers: no_mods,
        };
        assert_eq!(
            encode_runtime_key_event(ev, modes),
            Some(b"\x1bOB".to_vec())
        );

        let ev = RuntimeKeyEvent {
            key: RuntimeKey::Right,
            modifiers: no_mods,
        };
        assert_eq!(
            encode_runtime_key_event(ev, modes),
            Some(b"\x1bOC".to_vec())
        );

        let ev = RuntimeKeyEvent {
            key: RuntimeKey::Left,
            modifiers: no_mods,
        };
        assert_eq!(
            encode_runtime_key_event(ev, modes),
            Some(b"\x1bOD".to_vec())
        );
    }

    #[test]
    fn application_cursor_keys_uses_csi_when_modifier_present() {
        let modes = TerminalModeFlags {
            application_cursor_keys: true,
            ..Default::default()
        };
        let ctrl = RuntimeKeyModifiers {
            shift: false,
            alt: false,
            control: true,
            super_key: false,
        };
        let ev = RuntimeKeyEvent {
            key: RuntimeKey::Up,
            modifiers: ctrl,
        };
        assert_eq!(
            encode_runtime_key_event(ev, modes),
            Some(b"\x1b[1;5A".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_regular_keys_as_csi_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Character('a'),
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_enter_as_csi_13_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Enter,
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[13u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_tab_as_csi_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Tab,
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[9u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_backspace_as_csi_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Backspace,
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[127u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_escape_as_csi_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Escape,
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[27u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_ctrl_a_with_modifier() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Character('a'),
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: true,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97;5u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_shift_tab_with_modifier() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Tab,
            modifiers: RuntimeKeyModifiers {
                shift: true,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[9;2u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_arrows_use_legacy_encoding() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Up,
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn kitty_mode_disabled_uses_legacy() {
        let modes = TerminalModeFlags::default();
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Character('a'),
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        };
        assert_eq!(encode_runtime_key_event(key_event, modes), Some(vec![b'a']));
    }

    #[test]
    fn legacy_alt_backspace_respects_escape_modes() {
        let disabled = TerminalModeFlags {
            meta_sends_escape: false,
            alt_sends_escape: false,
            ..Default::default()
        };
        let enabled = TerminalModeFlags {
            meta_sends_escape: false,
            alt_sends_escape: true,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Backspace,
            modifiers: RuntimeKeyModifiers {
                alt: true,
                ..Default::default()
            },
        };

        assert_eq!(
            encode_runtime_key_event(key_event, disabled),
            Some(vec![0x7f])
        );
        assert_eq!(
            encode_runtime_key_event(key_event, enabled),
            Some(b"\x1b\x7f".to_vec())
        );
    }

    #[test]
    fn kitty_mode_super_modifier_encoded() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = RuntimeKeyEvent {
            key: RuntimeKey::Character('a'),
            modifiers: RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: true,
            },
        };
        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97;9u".to_vec())
        );
    }
}
