// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rldyourterm_core::{
    RuntimeKey, RuntimeKeyEvent, RuntimeKeyEventKind, RuntimeKeyModifiers, TerminalModeFlags,
    encode_runtime_key_event,
};
use winit::event::{ElementState, KeyEvent as WinitKeyEvent};
use winit::keyboard::{Key, KeyCode as WinitKeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

struct CanonicalizedCharacter {
    key: char,
    shifted_key: Option<char>,
    associated_text: Option<String>,
}

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

fn runtime_key_event_kind_from_crossterm(kind: KeyEventKind) -> RuntimeKeyEventKind {
    match kind {
        KeyEventKind::Press => RuntimeKeyEventKind::Press,
        KeyEventKind::Repeat => RuntimeKeyEventKind::Repeat,
        KeyEventKind::Release => RuntimeKeyEventKind::Release,
    }
}

fn runtime_key_event_kind_from_winit(event: &WinitKeyEvent) -> RuntimeKeyEventKind {
    match event.state {
        ElementState::Released => RuntimeKeyEventKind::Release,
        ElementState::Pressed if event.repeat => RuntimeKeyEventKind::Repeat,
        ElementState::Pressed => RuntimeKeyEventKind::Press,
    }
}

pub fn runtime_key_event_from_crossterm(key_event: KeyEvent) -> Option<RuntimeKeyEvent> {
    let mut modifiers = runtime_key_modifiers_from_crossterm(key_event.modifiers);
    let kind = runtime_key_event_kind_from_crossterm(key_event.kind);

    match key_event.code {
        KeyCode::Char(ch) => {
            let canonical = canonicalize_crossterm_character(ch, modifiers);
            let mut event = RuntimeKeyEvent::new(RuntimeKey::Character(canonical.key), modifiers)
                .with_kind(kind);
            event = event.with_shifted_key(canonical.shifted_key);
            if let Some(text) = canonical.associated_text {
                event = event.with_associated_text(text);
            }
            Some(event)
        }
        KeyCode::Enter => Some(RuntimeKeyEvent::new(RuntimeKey::Enter, modifiers).with_kind(kind)),
        KeyCode::Backspace => {
            Some(RuntimeKeyEvent::new(RuntimeKey::Backspace, modifiers).with_kind(kind))
        }
        KeyCode::BackTab => {
            modifiers.shift = true;
            Some(RuntimeKeyEvent::new(RuntimeKey::Tab, modifiers).with_kind(kind))
        }
        KeyCode::Tab => Some(RuntimeKeyEvent::new(RuntimeKey::Tab, modifiers).with_kind(kind)),
        KeyCode::Esc => Some(RuntimeKeyEvent::new(RuntimeKey::Escape, modifiers).with_kind(kind)),
        KeyCode::Up => Some(RuntimeKeyEvent::new(RuntimeKey::Up, modifiers).with_kind(kind)),
        KeyCode::Down => Some(RuntimeKeyEvent::new(RuntimeKey::Down, modifiers).with_kind(kind)),
        KeyCode::Right => Some(RuntimeKeyEvent::new(RuntimeKey::Right, modifiers).with_kind(kind)),
        KeyCode::Left => Some(RuntimeKeyEvent::new(RuntimeKey::Left, modifiers).with_kind(kind)),
        KeyCode::Home => Some(RuntimeKeyEvent::new(RuntimeKey::Home, modifiers).with_kind(kind)),
        KeyCode::End => Some(RuntimeKeyEvent::new(RuntimeKey::End, modifiers).with_kind(kind)),
        KeyCode::Delete => {
            Some(RuntimeKeyEvent::new(RuntimeKey::Delete, modifiers).with_kind(kind))
        }
        KeyCode::Insert => {
            Some(RuntimeKeyEvent::new(RuntimeKey::Insert, modifiers).with_kind(kind))
        }
        KeyCode::PageUp => {
            Some(RuntimeKeyEvent::new(RuntimeKey::PageUp, modifiers).with_kind(kind))
        }
        KeyCode::PageDown => {
            Some(RuntimeKeyEvent::new(RuntimeKey::PageDown, modifiers).with_kind(kind))
        }
        // Legacy xterm/function-key mapping in this runtime is defined only for F1-F20.
        // Reject higher crossterm function keys up front instead of accepting and
        // silently dropping them during encoding.
        KeyCode::F(index @ 1..=20) => {
            Some(RuntimeKeyEvent::new(RuntimeKey::F(index), modifiers).with_kind(kind))
        }
        _ => None,
    }
}

pub fn runtime_key_event_from_winit(
    key: &Key,
    modifiers: ModifiersState,
) -> Option<RuntimeKeyEvent> {
    Some(RuntimeKeyEvent::new(
        runtime_key_from_winit(key)?,
        runtime_key_modifiers_from_winit(modifiers),
    ))
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
        .map(|key| RuntimeKeyEvent::new(key, runtime_key_modifiers_from_winit(modifiers)))
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

    runtime_key_event_from_winit_input(event, modifiers)
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

fn runtime_key_event_from_winit_input(
    event: &WinitKeyEvent,
    modifiers: RuntimeKeyModifiers,
) -> Option<RuntimeKeyEvent> {
    let key_without_modifiers = event.key_without_modifiers();
    let key = runtime_key_from_winit_ref(key_without_modifiers.as_ref())
        .or_else(|| runtime_key_from_winit_ref(event.logical_key.as_ref()))
        .or_else(|| {
            associated_text_from_winit(event)
                .as_deref()
                .and_then(single_char)
                .map(RuntimeKey::Character)
        })?;
    let mut runtime_event =
        RuntimeKeyEvent::new(key, modifiers).with_kind(runtime_key_event_kind_from_winit(event));

    if let RuntimeKey::Character(main_key) = key {
        runtime_event =
            runtime_event.with_shifted_key(shifted_key_from_winit(event, modifiers, main_key));
        runtime_event =
            runtime_event.with_base_layout_key(base_layout_key_from_winit(event, main_key));
    }
    if let Some(text) = associated_text_from_winit(event) {
        runtime_event = runtime_event.with_associated_text(text);
    }

    Some(runtime_event)
}

fn shifted_key_from_winit(
    event: &WinitKeyEvent,
    modifiers: RuntimeKeyModifiers,
    main_key: char,
) -> Option<char> {
    if !modifiers.shift {
        return None;
    }

    character_from_winit_key(event.logical_key.as_ref()).filter(|shifted| *shifted != main_key)
}

fn base_layout_key_from_winit(event: &WinitKeyEvent, main_key: char) -> Option<char> {
    let base_layout_key = match event.physical_key {
        PhysicalKey::Code(code) => us_layout_char_from_physical_key(code)?,
        PhysicalKey::Unidentified(_) => return None,
    };

    (base_layout_key != main_key).then_some(base_layout_key)
}

fn associated_text_from_winit(event: &WinitKeyEvent) -> Option<String> {
    for candidate in [event.text_with_all_modifiers(), event.text.as_deref()] {
        if let Some(candidate) = candidate.filter(|text| valid_associated_text(text)) {
            return Some(candidate.to_string());
        }
    }

    None
}

fn valid_associated_text(text: &str) -> bool {
    !text.is_empty()
        && !text
            .chars()
            .any(|ch| matches!(u32::from(ch), 0x00..=0x1f | 0x7f..=0x9f))
}

fn canonicalize_crossterm_character(
    ch: char,
    modifiers: RuntimeKeyModifiers,
) -> CanonicalizedCharacter {
    let associated_text = (!ch.is_control()).then(|| ch.to_string());
    let shifted_key = modifiers.shift.then_some(ch);

    if ch.is_ascii_uppercase() {
        return CanonicalizedCharacter {
            key: ch.to_ascii_lowercase(),
            shifted_key,
            associated_text,
        };
    }

    if let Some(unshifted) = unshifted_us_ascii_char(ch) {
        return CanonicalizedCharacter {
            key: unshifted,
            shifted_key,
            associated_text,
        };
    }

    CanonicalizedCharacter {
        key: ch,
        shifted_key: None,
        associated_text,
    }
}

fn unshifted_us_ascii_char(ch: char) -> Option<char> {
    match ch {
        '!' => Some('1'),
        '@' => Some('2'),
        '#' => Some('3'),
        '$' => Some('4'),
        '%' => Some('5'),
        '^' => Some('6'),
        '&' => Some('7'),
        '*' => Some('8'),
        '(' => Some('9'),
        ')' => Some('0'),
        '_' => Some('-'),
        '+' => Some('='),
        '{' => Some('['),
        '}' => Some(']'),
        '|' => Some('\\'),
        ':' => Some(';'),
        '"' => Some('\''),
        '<' => Some(','),
        '>' => Some('.'),
        '?' => Some('/'),
        '~' => Some('`'),
        _ => None,
    }
}

fn us_layout_char_from_physical_key(code: WinitKeyCode) -> Option<char> {
    match code {
        WinitKeyCode::Digit0 => Some('0'),
        WinitKeyCode::Digit1 => Some('1'),
        WinitKeyCode::Digit2 => Some('2'),
        WinitKeyCode::Digit3 => Some('3'),
        WinitKeyCode::Digit4 => Some('4'),
        WinitKeyCode::Digit5 => Some('5'),
        WinitKeyCode::Digit6 => Some('6'),
        WinitKeyCode::Digit7 => Some('7'),
        WinitKeyCode::Digit8 => Some('8'),
        WinitKeyCode::Digit9 => Some('9'),
        WinitKeyCode::KeyA => Some('a'),
        WinitKeyCode::KeyB => Some('b'),
        WinitKeyCode::KeyC => Some('c'),
        WinitKeyCode::KeyD => Some('d'),
        WinitKeyCode::KeyE => Some('e'),
        WinitKeyCode::KeyF => Some('f'),
        WinitKeyCode::KeyG => Some('g'),
        WinitKeyCode::KeyH => Some('h'),
        WinitKeyCode::KeyI => Some('i'),
        WinitKeyCode::KeyJ => Some('j'),
        WinitKeyCode::KeyK => Some('k'),
        WinitKeyCode::KeyL => Some('l'),
        WinitKeyCode::KeyM => Some('m'),
        WinitKeyCode::KeyN => Some('n'),
        WinitKeyCode::KeyO => Some('o'),
        WinitKeyCode::KeyP => Some('p'),
        WinitKeyCode::KeyQ => Some('q'),
        WinitKeyCode::KeyR => Some('r'),
        WinitKeyCode::KeyS => Some('s'),
        WinitKeyCode::KeyT => Some('t'),
        WinitKeyCode::KeyU => Some('u'),
        WinitKeyCode::KeyV => Some('v'),
        WinitKeyCode::KeyW => Some('w'),
        WinitKeyCode::KeyX => Some('x'),
        WinitKeyCode::KeyY => Some('y'),
        WinitKeyCode::KeyZ => Some('z'),
        WinitKeyCode::Backquote => Some('`'),
        WinitKeyCode::Backslash => Some('\\'),
        WinitKeyCode::BracketLeft => Some('['),
        WinitKeyCode::BracketRight => Some(']'),
        WinitKeyCode::Comma => Some(','),
        WinitKeyCode::Equal => Some('='),
        WinitKeyCode::Minus => Some('-'),
        WinitKeyCode::Period => Some('.'),
        WinitKeyCode::Quote => Some('\''),
        WinitKeyCode::Semicolon => Some(';'),
        WinitKeyCode::Slash => Some('/'),
        WinitKeyCode::Space => Some(' '),
        _ => None,
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
            let ch = single_char(text)?;
            Some(RuntimeKey::Character(ch))
        }
        _ => None,
    }
}

fn character_from_winit_key(key: Key<&str>) -> Option<char> {
    match key {
        Key::Character(text) => single_char(text),
        _ => None,
    }
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Some(ch)
}

#[cfg(test)]
mod tests {
    use super::{
        encode_crossterm_key_event, encode_winit_text_bytes, is_runtime_palette_shortcut_winit,
        runtime_key_event_from_crossterm, runtime_key_from_crossterm,
        runtime_key_from_winit_borrowed,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use rldyourterm_core::{
        RuntimeKey, RuntimeKeyEventKind, RuntimeKeyModifiers, TerminalModeFlags,
    };
    use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

    fn encode_key(key_event: KeyEvent) -> Option<Vec<u8>> {
        encode_crossterm_key_event(key_event, TerminalModeFlags::default())
    }

    #[test]
    fn encode_winit_text_bytes_prefers_plain_text_without_modifiers() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers::default(),
            TerminalModeFlags::default(),
        );
        assert_eq!(bytes, Some(vec![b'x']));
    }

    #[test]
    fn encode_winit_text_bytes_prefixes_alt_text_with_escape() {
        let bytes = encode_winit_text_bytes(
            Some("x"),
            RuntimeKeyModifiers {
                alt: true,
                ..Default::default()
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
                control: true,
                ..Default::default()
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
                alt: true,
                ..Default::default()
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
    fn crossterm_shifted_character_normalizes_key_text_and_event_kind() {
        let key_event = KeyEvent::new_with_kind(
            KeyCode::Char('A'),
            KeyModifiers::SHIFT,
            KeyEventKind::Repeat,
        );
        let runtime_event =
            runtime_key_event_from_crossterm(key_event).expect("shifted character must map");

        assert_eq!(runtime_event.key, RuntimeKey::Character('a'));
        assert_eq!(runtime_event.kind, RuntimeKeyEventKind::Repeat);
        assert_eq!(runtime_event.shifted_key, Some('A'));
        assert_eq!(runtime_event.associated_text.as_deref(), Some("A"));
    }

    #[test]
    fn crossterm_shifted_symbol_normalizes_key_text() {
        let key_event = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT);
        let runtime_event =
            runtime_key_event_from_crossterm(key_event).expect("shifted symbol must map");

        assert_eq!(runtime_event.key, RuntimeKey::Character('='));
        assert_eq!(runtime_event.shifted_key, Some('+'));
        assert_eq!(runtime_event.associated_text.as_deref(), Some("+"));
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
