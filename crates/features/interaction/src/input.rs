// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rldyourterm_core::{
    RuntimeKey, RuntimeKeyEvent, RuntimeKeyModifiers, TerminalModeFlags, encode_runtime_key_event,
};
use winit::event::KeyEvent as WinitKeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

fn runtime_key_modifiers_from_crossterm(modifiers: KeyModifiers) -> RuntimeKeyModifiers {
    RuntimeKeyModifiers {
        shift: modifiers.contains(KeyModifiers::SHIFT),
        alt: modifiers.contains(KeyModifiers::ALT),
        control: modifiers.contains(KeyModifiers::CONTROL),
        super_key: modifiers.contains(KeyModifiers::SUPER),
    }
}

fn runtime_key_modifiers_from_winit(modifiers: ModifiersState) -> RuntimeKeyModifiers {
    RuntimeKeyModifiers {
        shift: modifiers.shift_key(),
        alt: modifiers.alt_key(),
        control: modifiers.control_key(),
        super_key: modifiers.super_key(),
    }
}

pub fn runtime_key_event_from_crossterm(key_event: KeyEvent) -> Option<RuntimeKeyEvent> {
    let mut modifiers = runtime_key_modifiers_from_crossterm(key_event.modifiers);
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
        modifiers: runtime_key_modifiers_from_winit(modifiers),
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
            modifiers: runtime_key_modifiers_from_winit(modifiers),
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
    let modifiers = runtime_key_modifiers_from_winit(modifiers);
    if modes.kitty_keyboard_flags == 0
        && let Some(bytes) = encode_winit_text_bytes(event.text.as_deref(), modifiers, modes)
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

fn encode_winit_text_bytes(
    text: Option<&str>,
    modifiers: RuntimeKeyModifiers,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    let text = text.filter(|text| !text.is_empty())?;
    if modifiers.alt
        && !modifiers.control
        && !modifiers.super_key
        && modes.alt_modifier_sends_escape()
    {
        let mut bytes = vec![0x1b];
        bytes.extend_from_slice(text.as_bytes());
        Some(bytes)
    } else if !modifiers.control && !modifiers.super_key {
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
        encode_crossterm_key_event, encode_winit_text_bytes, is_runtime_palette_shortcut_winit,
        runtime_key_from_crossterm, runtime_key_from_winit_borrowed,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rldyourterm_core::{RuntimeKey, RuntimeKeyModifiers, TerminalModeFlags};
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
            TerminalModeFlags::default(),
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
            TerminalModeFlags::default(),
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
            TerminalModeFlags::default(),
        );
        assert_eq!(bytes, None);
    }

    #[test]
    fn encode_winit_text_bytes_allows_alt_text_without_escape_when_disabled() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers {
                shift: false,
                alt: true,
                control: false,
                super_key: false,
            },
            TerminalModeFlags {
                meta_sends_escape: false,
                alt_sends_escape: false,
                ..Default::default()
            },
        );
        assert_eq!(bytes, Some(vec![b'x']));
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
}
