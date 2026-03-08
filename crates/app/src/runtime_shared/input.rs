use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use winit::event::KeyEvent as WinitKeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::shared::{
    csi_modified, encode_ctrl_letter, fkey_ss3_modified, tilde_modified, xterm_modifier_param,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeKey {
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
pub(crate) struct RuntimeKeyModifiers {
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
pub(crate) struct RuntimeKeyEvent {
    pub key: RuntimeKey,
    pub modifiers: RuntimeKeyModifiers,
}

pub(crate) fn runtime_key_event_from_crossterm(key_event: KeyEvent) -> Option<RuntimeKeyEvent> {
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
        KeyCode::F(index @ 1..=12) => RuntimeKey::F(index),
        _ => return None,
    };

    Some(RuntimeKeyEvent { key, modifiers })
}

pub(crate) fn runtime_key_event_from_winit(
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

pub(crate) fn runtime_key_from_winit_borrowed(key: Key<&str>) -> Option<RuntimeKey> {
    runtime_key_from_winit_ref(key)
}

pub(crate) fn is_runtime_palette_shortcut(key_event: RuntimeKeyEvent) -> bool {
    key_event.modifiers.shift
        && (key_event.modifiers.control || key_event.modifiers.super_key)
        && matches!(
            key_event.key,
            RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'p')
        )
}

pub(crate) fn is_runtime_palette_shortcut_crossterm(key_event: KeyEvent) -> bool {
    runtime_key_event_from_crossterm(key_event).is_some_and(is_runtime_palette_shortcut)
}

pub(crate) fn is_runtime_palette_shortcut_winit(key: Key<&str>, modifiers: ModifiersState) -> bool {
    runtime_key_from_winit_borrowed(key)
        .map(|key| RuntimeKeyEvent {
            key,
            modifiers: RuntimeKeyModifiers::from_winit(modifiers),
        })
        .is_some_and(is_runtime_palette_shortcut)
}

pub(crate) fn is_local_shutdown_key(key_event: RuntimeKeyEvent) -> bool {
    key_event.modifiers.control
        && matches!(
            key_event.key,
            RuntimeKey::Character(ch) if ch.eq_ignore_ascii_case(&'q')
        )
}

pub(crate) fn is_local_shutdown_key_crossterm(key_event: KeyEvent) -> bool {
    runtime_key_event_from_crossterm(key_event).is_some_and(is_local_shutdown_key)
}

pub(crate) fn is_local_shutdown_key_winit(
    event: &WinitKeyEvent,
    modifiers: ModifiersState,
) -> bool {
    runtime_key_event_from_winit(&event.logical_key, modifiers).is_some_and(is_local_shutdown_key)
}

pub(crate) fn encode_runtime_key_event(key_event: RuntimeKeyEvent) -> Option<Vec<u8>> {
    let modifiers = key_event.modifiers;
    let mod_param = modifiers.xterm_modifier_param();
    let has_mod = mod_param > 1;

    match key_event.key {
        RuntimeKey::Enter => Some(vec![b'\r']),
        RuntimeKey::Backspace if modifiers.alt && !modifiers.control => Some(b"\x1b\x7f".to_vec()),
        RuntimeKey::Backspace => Some(vec![0x7f]),
        RuntimeKey::Tab if modifiers.shift && !modifiers.alt && !modifiers.control => {
            Some(b"\x1b[Z".to_vec())
        }
        RuntimeKey::Tab if !modifiers.alt && !modifiers.control => Some(vec![b'\t']),
        RuntimeKey::Escape => Some(vec![0x1b]),
        RuntimeKey::Up => Some(csi_modified(b'A', mod_param, has_mod)),
        RuntimeKey::Down => Some(csi_modified(b'B', mod_param, has_mod)),
        RuntimeKey::Right => Some(csi_modified(b'C', mod_param, has_mod)),
        RuntimeKey::Left => Some(csi_modified(b'D', mod_param, has_mod)),
        RuntimeKey::Home => Some(csi_modified(b'H', mod_param, has_mod)),
        RuntimeKey::End => Some(csi_modified(b'F', mod_param, has_mod)),
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

pub(crate) fn encode_crossterm_key_event(key_event: KeyEvent) -> Option<Vec<u8>> {
    let key_event = runtime_key_event_from_crossterm(key_event)?;
    encode_runtime_key_event(key_event)
}

pub(crate) fn encode_winit_key_event(
    event: &WinitKeyEvent,
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    let modifiers = RuntimeKeyModifiers::from_winit(modifiers);
    encode_winit_text_bytes(event.text.as_deref(), modifiers).or_else(|| {
        runtime_key_from_winit(&event.logical_key)
            .map(|key| RuntimeKeyEvent { key, modifiers })
            .and_then(encode_runtime_key_event)
    })
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
        RuntimeKeyModifiers, encode_crossterm_key_event, encode_winit_text_bytes,
        is_runtime_palette_shortcut_winit, runtime_key_from_crossterm,
        runtime_key_from_winit_borrowed,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use winit::keyboard::{Key, ModifiersState, NamedKey, SmolStr};

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
            Some(super::RuntimeKey::Character('p'))
        );
        assert_eq!(
            runtime_key_from_winit_borrowed(Key::Named(NamedKey::Escape)),
            Some(super::RuntimeKey::Escape)
        );
    }

    #[test]
    fn crossterm_key_maps_runtime_key() {
        assert_eq!(
            runtime_key_from_crossterm(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(super::RuntimeKey::Up)
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
            encode_crossterm_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(vec![b'\r'])
        );
    }
}
