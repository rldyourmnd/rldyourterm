// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use winit::event::KeyEvent as WinitKeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::key_encoding::{
    cursor_key, encode_ctrl_letter, fkey_ss3_modified, tilde_modified, xterm_modifier_param,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeKeyModifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
    pub super_key: bool,
}

impl RuntimeKeyModifiers {
    fn from_crossterm(modifiers: KeyModifiers) -> Self {
        Self {
            shift: modifiers.contains(KeyModifiers::SHIFT),
            alt: modifiers.contains(KeyModifiers::ALT),
            control: modifiers.contains(KeyModifiers::CONTROL),
            super_key: modifiers.contains(KeyModifiers::SUPER),
        }
    }

    fn from_winit(modifiers: ModifiersState) -> Self {
        Self {
            shift: modifiers.shift_key(),
            alt: modifiers.alt_key(),
            control: modifiers.control_key(),
            super_key: modifiers.super_key(),
        }
    }

    fn xterm_modifier_param(self) -> u8 {
        xterm_modifier_param(self.shift, self.alt, self.control)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeKeyEvent {
    pub key: RuntimeKey,
    pub modifiers: RuntimeKeyModifiers,
}

pub fn runtime_key_event_from_crossterm(key_event: KeyEvent) -> Option<RuntimeKeyEvent> {
    let mut modifiers = RuntimeKeyModifiers::from_crossterm(key_event.modifiers);
    let key = match key_event.code {
        KeyCode::Char(ch) => RuntimeKey::Character(ch),
        KeyCode::Enter => RuntimeKey::Enter,
        KeyCode::Backspace => RuntimeKey::Backspace,
        KeyCode::BackTab => {
            modifiers.shift = true;
            RuntimeKey::Tab
        }
        KeyCode::Tab => RuntimeKey::Tab,
        KeyCode::Esc => RuntimeKey::Escape,
        KeyCode::Up => RuntimeKey::Up,
        KeyCode::Down => RuntimeKey::Down,
        KeyCode::Right => RuntimeKey::Right,
        KeyCode::Left => RuntimeKey::Left,
        KeyCode::Home => RuntimeKey::Home,
        KeyCode::End => RuntimeKey::End,
        KeyCode::Delete => RuntimeKey::Delete,
        KeyCode::Insert => RuntimeKey::Insert,
        KeyCode::PageUp => RuntimeKey::PageUp,
        KeyCode::PageDown => RuntimeKey::PageDown,
        // Legacy xterm/function-key mapping in this runtime is defined only for F1-F20.
        // Reject higher crossterm function keys up front instead of accepting and
        // silently dropping them during encoding.
        KeyCode::F(index @ 1..=20) => RuntimeKey::F(index),
        _ => return None,
    };

    Some(RuntimeKeyEvent { key, modifiers })
}

pub fn runtime_key_event_from_winit(
    key: &Key,
    modifiers: ModifiersState,
) -> Option<RuntimeKeyEvent> {
    Some(RuntimeKeyEvent {
        key: runtime_key_from_winit(key)?,
        modifiers: RuntimeKeyModifiers::from_winit(modifiers),
    })
}

#[cfg(test)]
pub(crate) fn runtime_key_from_crossterm(key_event: KeyEvent) -> Option<RuntimeKey> {
    runtime_key_event_from_crossterm(key_event).map(|event| event.key)
}

pub fn runtime_key_from_winit_borrowed(key: Key<&str>) -> Option<RuntimeKey> {
    runtime_key_from_winit_ref(key)
}

pub fn is_runtime_palette_shortcut(key_event: RuntimeKeyEvent) -> bool {
    key_event.modifiers.shift
        && (key_event.modifiers.control || key_event.modifiers.super_key)
        && matches!(
            key_event.key,
            RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'p')
        )
}

pub fn is_runtime_palette_shortcut_crossterm(key_event: KeyEvent) -> bool {
    runtime_key_event_from_crossterm(key_event).is_some_and(is_runtime_palette_shortcut)
}

pub fn is_runtime_palette_shortcut_winit(key: Key<&str>, modifiers: ModifiersState) -> bool {
    runtime_key_from_winit_borrowed(key)
        .map(|key| RuntimeKeyEvent {
            key,
            modifiers: RuntimeKeyModifiers::from_winit(modifiers),
        })
        .is_some_and(is_runtime_palette_shortcut)
}

pub fn is_local_shutdown_key(key_event: RuntimeKeyEvent) -> bool {
    key_event.modifiers.control
        && matches!(
            key_event.key,
            RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'q')
        )
}

pub fn is_local_shutdown_key_crossterm(key_event: KeyEvent) -> bool {
    runtime_key_event_from_crossterm(key_event).is_some_and(is_local_shutdown_key)
}

pub fn is_local_shutdown_key_winit(event: &WinitKeyEvent, modifiers: ModifiersState) -> bool {
    runtime_key_event_from_winit(&event.logical_key, modifiers).is_some_and(is_local_shutdown_key)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalModeFlags {
    pub application_cursor_keys: bool,
    pub kitty_keyboard_flags: u16,
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
        RuntimeKey::Backspace if modifiers.alt && !modifiers.control => Some(b"\x1b\x7f".to_vec()),
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
            if modifiers.alt && !modifiers.control && !modifiers.super_key =>
        {
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(ch.to_string().as_bytes());
            Some(bytes)
        }
        RuntimeKey::Character(ch) if !modifiers.super_key => Some(ch.to_string().into_bytes()),
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

pub fn encode_crossterm_key_event(
    key_event: KeyEvent,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    let key_event = runtime_key_event_from_crossterm(key_event)?;
    encode_runtime_key_event(key_event, modes)
}

pub fn encode_winit_key_event(
    event: &WinitKeyEvent,
    modifiers: ModifiersState,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    let modifiers = RuntimeKeyModifiers::from_winit(modifiers);
    if modes.kitty_keyboard_flags == 0
        && let Some(bytes) = encode_winit_text_bytes(event.text.as_deref(), modifiers)
    {
        return Some(bytes);
    }
    runtime_key_from_winit(&event.logical_key)
        .map(|key| RuntimeKeyEvent { key, modifiers })
        .or_else(|| {
            event
                .text
                .as_deref()
                .and_then(|text| text.chars().next())
                .map(|ch| RuntimeKeyEvent {
                    key: RuntimeKey::Character(ch),
                    modifiers,
                })
        })
        .and_then(|event| encode_runtime_key_event(event, modes))
}

fn encode_winit_text_bytes(text: Option<&str>, modifiers: RuntimeKeyModifiers) -> Option<Vec<u8>> {
    let text = text.filter(|text| !text.is_empty())?;
    if modifiers.alt && !modifiers.control && !modifiers.super_key {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(text.as_bytes());
        Some(bytes)
    } else if !modifiers.control && !modifiers.alt && !modifiers.super_key {
        Some(text.as_bytes().to_vec())
    } else {
        None
    }
}

fn runtime_key_from_winit(key: &Key) -> Option<RuntimeKey> {
    runtime_key_from_winit_ref(key.as_ref())
}

fn runtime_key_from_winit_ref(key: Key<&str>) -> Option<RuntimeKey> {
    match key {
        Key::Named(NamedKey::Enter) => Some(RuntimeKey::Enter),
        Key::Named(NamedKey::Tab) => Some(RuntimeKey::Tab),
        Key::Named(NamedKey::Escape) => Some(RuntimeKey::Escape),
        Key::Named(NamedKey::Backspace) => Some(RuntimeKey::Backspace),
        Key::Named(NamedKey::ArrowUp) => Some(RuntimeKey::Up),
        Key::Named(NamedKey::ArrowDown) => Some(RuntimeKey::Down),
        Key::Named(NamedKey::ArrowRight) => Some(RuntimeKey::Right),
        Key::Named(NamedKey::ArrowLeft) => Some(RuntimeKey::Left),
        Key::Named(NamedKey::Home) => Some(RuntimeKey::Home),
        Key::Named(NamedKey::End) => Some(RuntimeKey::End),
        Key::Named(NamedKey::Delete) => Some(RuntimeKey::Delete),
        Key::Named(NamedKey::Insert) => Some(RuntimeKey::Insert),
        Key::Named(NamedKey::PageUp) => Some(RuntimeKey::PageUp),
        Key::Named(NamedKey::PageDown) => Some(RuntimeKey::PageDown),
        Key::Named(NamedKey::F1) => Some(RuntimeKey::F(1)),
        Key::Named(NamedKey::F2) => Some(RuntimeKey::F(2)),
        Key::Named(NamedKey::F3) => Some(RuntimeKey::F(3)),
        Key::Named(NamedKey::F4) => Some(RuntimeKey::F(4)),
        Key::Named(NamedKey::F5) => Some(RuntimeKey::F(5)),
        Key::Named(NamedKey::F6) => Some(RuntimeKey::F(6)),
        Key::Named(NamedKey::F7) => Some(RuntimeKey::F(7)),
        Key::Named(NamedKey::F8) => Some(RuntimeKey::F(8)),
        Key::Named(NamedKey::F9) => Some(RuntimeKey::F(9)),
        Key::Named(NamedKey::F10) => Some(RuntimeKey::F(10)),
        Key::Named(NamedKey::F11) => Some(RuntimeKey::F(11)),
        Key::Named(NamedKey::F12) => Some(RuntimeKey::F(12)),
        Key::Named(NamedKey::F13) => Some(RuntimeKey::F(13)),
        Key::Named(NamedKey::F14) => Some(RuntimeKey::F(14)),
        Key::Named(NamedKey::F15) => Some(RuntimeKey::F(15)),
        Key::Named(NamedKey::F16) => Some(RuntimeKey::F(16)),
        Key::Named(NamedKey::F17) => Some(RuntimeKey::F(17)),
        Key::Named(NamedKey::F18) => Some(RuntimeKey::F(18)),
        Key::Named(NamedKey::F19) => Some(RuntimeKey::F(19)),
        Key::Named(NamedKey::F20) => Some(RuntimeKey::F(20)),
        Key::Character(text) => {
            let mut chars = text.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Some(RuntimeKey::Character(ch))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeKey, RuntimeKeyEvent, RuntimeKeyModifiers, TerminalModeFlags,
        encode_crossterm_key_event, encode_runtime_key_event, encode_winit_text_bytes,
        is_runtime_palette_shortcut_winit, runtime_key_from_crossterm,
        runtime_key_from_winit_borrowed,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

    fn encode_key(key_event: KeyEvent) -> Option<Vec<u8>> {
        encode_crossterm_key_event(key_event, TerminalModeFlags::default())
    }

    #[test]
    fn encode_winit_text_bytes_prefers_plain_text_without_modifiers() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: false,
                super_key: false,
            },
        );
        assert_eq!(bytes, Some(vec![b'x']));
    }

    #[test]
    fn encode_winit_text_bytes_prefixes_alt_text_with_escape() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers {
                shift: false,
                alt: true,
                control: false,
                super_key: false,
            },
        );
        assert_eq!(bytes, Some(b"\x1bx".to_vec()));
    }

    #[test]
    fn encode_winit_text_bytes_ignores_controlled_text() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers {
                shift: false,
                alt: false,
                control: true,
                super_key: false,
            },
        );
        assert_eq!(bytes, None);
    }

    #[test]
    fn borrowed_winit_key_maps_characters_and_escape() {
        assert_eq!(
            runtime_key_from_winit_borrowed(Key::Character(&SmolStr::new("p"))),
            Some(RuntimeKey::Character('p'))
        );
        assert_eq!(
            runtime_key_from_winit_borrowed(Key::Named(NamedKey::Escape)),
            Some(RuntimeKey::Escape)
        );
    }

    #[test]
    fn crossterm_key_maps_runtime_key() {
        assert_eq!(
            runtime_key_from_crossterm(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(RuntimeKey::Up)
        );
    }

    #[test]
    fn crossterm_f21_is_rejected_when_no_encoding_exists() {
        assert_eq!(
            runtime_key_from_crossterm(KeyEvent::new(KeyCode::F(21), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn winit_palette_shortcut_matches_ctrl_shift_p() {
        let mut modifiers = ModifiersState::default();
        modifiers.set(ModifiersState::SHIFT, true);
        modifiers.set(ModifiersState::CONTROL, true);
        assert!(is_runtime_palette_shortcut_winit(
            Key::Character(&SmolStr::new("p")),
            modifiers
        ));
    }

    #[test]
    fn crossterm_encoded_key_event_still_uses_shared_encoder() {
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(vec![b'\r'])
        );
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
    fn kitty_mode_encodes_tab_as_csi_9_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
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
    fn kitty_mode_encodes_backspace_as_csi_127_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
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
    fn kitty_mode_encodes_escape_as_csi_27_u() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
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
    fn kitty_mode_super_modifier_encoded() {
        let modes = TerminalModeFlags {
            application_cursor_keys: false,
            kitty_keyboard_flags: 1,
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
