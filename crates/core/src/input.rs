// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

const KITTY_FLAG_REPORT_EVENT_TYPES: u16 = 0b00010;
const KITTY_FLAG_REPORT_ALTERNATE_KEYS: u16 = 0b00100;
const KITTY_FLAG_REPORT_ALL_KEYS_AS_ESCAPE_CODES: u16 = 0b01000;
const KITTY_FLAG_REPORT_ASSOCIATED_TEXT: u16 = 0b10000;
const KITTY_PUA_FUNCTION_KEY_BASE: u32 = 57_376;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeKeyEventKind {
    #[default]
    Press,
    Repeat,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeKeyEvent {
    pub key: RuntimeKey,
    pub modifiers: RuntimeKeyModifiers,
    pub kind: RuntimeKeyEventKind,
    pub shifted_key: Option<char>,
    pub base_layout_key: Option<char>,
    pub associated_text: Option<String>,
}

impl RuntimeKeyEvent {
    pub const fn new(key: RuntimeKey, modifiers: RuntimeKeyModifiers) -> Self {
        Self {
            key,
            modifiers,
            kind: RuntimeKeyEventKind::Press,
            shifted_key: None,
            base_layout_key: None,
            associated_text: None,
        }
    }

    pub const fn with_kind(mut self, kind: RuntimeKeyEventKind) -> Self {
        self.kind = kind;
        self
    }

    pub const fn with_shifted_key(mut self, shifted_key: Option<char>) -> Self {
        self.shifted_key = shifted_key;
        self
    }

    pub const fn with_base_layout_key(mut self, base_layout_key: Option<char>) -> Self {
        self.base_layout_key = base_layout_key;
        self
    }

    pub fn with_associated_text(mut self, associated_text: impl Into<String>) -> Self {
        self.associated_text = Some(associated_text.into());
        self
    }
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
        return encode_kitty_key_event(&key_event, modes);
    }
    if key_event.kind == RuntimeKeyEventKind::Release {
        return None;
    }
    encode_legacy_key_event(&key_event, modes)
}

fn encode_legacy_key_event(
    key_event: &RuntimeKeyEvent,
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
        RuntimeKey::Character(_)
            if modifiers.alt
                && !modifiers.control
                && !modifiers.super_key
                && modes.alt_modifier_sends_escape() =>
        {
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(&legacy_character_bytes(key_event)?);
            Some(bytes)
        }
        RuntimeKey::Character(_) if !modifiers.control && !modifiers.super_key => {
            Some(legacy_character_bytes(key_event)?)
        }
        RuntimeKey::Character(_) => None,
        RuntimeKey::Tab => None,
        RuntimeKey::F(_) => None,
    }
}

fn encode_kitty_key_event(
    key_event: &RuntimeKeyEvent,
    modes: TerminalModeFlags,
) -> Option<Vec<u8>> {
    let flags = modes.kitty_keyboard_flags;
    let event_type = kitty_event_type_subfield(key_event.kind, flags)?;

    if matches!(
        key_event.kind,
        RuntimeKeyEventKind::Repeat | RuntimeKeyEventKind::Release
    ) && !kitty_reports_all_keys(flags)
        && key_event_requires_report_all_for_event_type(key_event.key)
    {
        return None;
    }

    match key_event.key {
        RuntimeKey::Enter => Some(kitty_csi_u(13, key_event, flags, event_type)),
        RuntimeKey::Tab => Some(kitty_csi_u(9, key_event, flags, event_type)),
        RuntimeKey::Backspace => Some(kitty_csi_u(127, key_event, flags, event_type)),
        RuntimeKey::Escape => Some(kitty_csi_u(27, key_event, flags, event_type)),
        RuntimeKey::Character(ch) => Some(kitty_csi_u(u32::from(ch), key_event, flags, event_type)),
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
        | RuntimeKey::F(_)
            if kitty_requires_canonical_functional_encoding(key_event.kind, flags) =>
        {
            encode_kitty_functional_key(key_event, flags, event_type)
        }
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

fn legacy_character_bytes(key_event: &RuntimeKeyEvent) -> Option<Vec<u8>> {
    match key_event.key {
        RuntimeKey::Character(ch) => {
            if let Some(text) = key_event
                .associated_text
                .as_deref()
                .filter(|text| !text.is_empty())
            {
                return Some(text.as_bytes().to_vec());
            }
            if key_event.modifiers.shift
                && let Some(shifted_key) = key_event.shifted_key
            {
                return Some(shifted_key.to_string().into_bytes());
            }
            Some(ch.to_string().into_bytes())
        }
        _ => None,
    }
}

fn key_event_requires_report_all_for_event_type(key: RuntimeKey) -> bool {
    matches!(
        key,
        RuntimeKey::Character(_) | RuntimeKey::Enter | RuntimeKey::Tab | RuntimeKey::Backspace
    )
}

fn kitty_requires_canonical_functional_encoding(kind: RuntimeKeyEventKind, flags: u16) -> bool {
    kitty_reports_all_keys(flags)
        || matches!(
            kind,
            RuntimeKeyEventKind::Repeat | RuntimeKeyEventKind::Release
        )
}

fn kitty_reports_all_keys(flags: u16) -> bool {
    flags & KITTY_FLAG_REPORT_ALL_KEYS_AS_ESCAPE_CODES != 0
}

fn kitty_event_type_subfield(kind: RuntimeKeyEventKind, flags: u16) -> Option<Option<u8>> {
    if flags & KITTY_FLAG_REPORT_EVENT_TYPES == 0 {
        return match kind {
            RuntimeKeyEventKind::Press | RuntimeKeyEventKind::Repeat => Some(None),
            RuntimeKeyEventKind::Release => None,
        };
    }

    Some(match kind {
        RuntimeKeyEventKind::Press => None,
        RuntimeKeyEventKind::Repeat => Some(2),
        RuntimeKeyEventKind::Release => Some(3),
    })
}

fn kitty_key_field(codepoint: u32, key_event: &RuntimeKeyEvent, flags: u16) -> String {
    if flags & KITTY_FLAG_REPORT_ALTERNATE_KEYS == 0 {
        return codepoint.to_string();
    }

    let shifted_key = if key_event.modifiers.shift {
        key_event
            .shifted_key
            .filter(|shifted| u32::from(*shifted) != codepoint)
    } else {
        None
    };
    let base_layout_key = key_event
        .base_layout_key
        .filter(|base| u32::from(*base) != codepoint);

    if shifted_key.is_none() && base_layout_key.is_none() {
        return codepoint.to_string();
    }

    let mut field = codepoint.to_string();
    if let Some(shifted_key) = shifted_key {
        field.push(':');
        field.push_str(&u32::from(shifted_key).to_string());
    }
    if let Some(base_layout_key) = base_layout_key {
        if shifted_key.is_none() {
            field.push(':');
        }
        field.push(':');
        field.push_str(&u32::from(base_layout_key).to_string());
    }
    field
}

fn kitty_modifier_field(modifiers: RuntimeKeyModifiers, event_type: Option<u8>) -> Option<String> {
    let modifier_param = kitty_modifier_param(modifiers);

    match event_type {
        Some(event_type) => Some(format!("{modifier_param}:{event_type}")),
        None if modifier_param > 1 => Some(modifier_param.to_string()),
        None => None,
    }
}

fn kitty_associated_text_field(key_event: &RuntimeKeyEvent, flags: u16) -> Option<String> {
    if flags & KITTY_FLAG_REPORT_ASSOCIATED_TEXT == 0 || !kitty_reports_all_keys(flags) {
        return None;
    }

    let text = key_event
        .associated_text
        .as_deref()
        .filter(|text| !text.is_empty() && !contains_control_codes(text))?;
    let mut field = String::new();

    for (index, codepoint) in text.chars().map(u32::from).enumerate() {
        if index > 0 {
            field.push(':');
        }
        field.push_str(&codepoint.to_string());
    }

    Some(field)
}

fn contains_control_codes(text: &str) -> bool {
    text.chars()
        .any(|ch| matches!(u32::from(ch), 0x00..=0x1f | 0x7f..=0x9f))
}

fn kitty_csi_u(
    codepoint: u32,
    key_event: &RuntimeKeyEvent,
    flags: u16,
    event_type: Option<u8>,
) -> Vec<u8> {
    let key_field = kitty_key_field(codepoint, key_event, flags);
    let modifier_field = kitty_modifier_field(key_event.modifiers, event_type);
    let text_field = kitty_associated_text_field(key_event, flags);
    let mut sequence = format!("\x1b[{key_field}");

    if modifier_field.is_some() || text_field.is_some() {
        sequence.push(';');
        if let Some(modifier_field) = modifier_field {
            sequence.push_str(&modifier_field);
        }
    }
    if let Some(text_field) = text_field {
        sequence.push(';');
        sequence.push_str(&text_field);
    }
    sequence.push('u');
    sequence.into_bytes()
}

enum KittyFunctionalKey {
    CsiLetter(u8),
    CsiTilde(u16),
    CsiUnicode(u32),
}

fn kitty_functional_key(key: RuntimeKey) -> Option<KittyFunctionalKey> {
    match key {
        RuntimeKey::Up => Some(KittyFunctionalKey::CsiLetter(b'A')),
        RuntimeKey::Down => Some(KittyFunctionalKey::CsiLetter(b'B')),
        RuntimeKey::Right => Some(KittyFunctionalKey::CsiLetter(b'C')),
        RuntimeKey::Left => Some(KittyFunctionalKey::CsiLetter(b'D')),
        RuntimeKey::End => Some(KittyFunctionalKey::CsiLetter(b'F')),
        RuntimeKey::Home => Some(KittyFunctionalKey::CsiLetter(b'H')),
        RuntimeKey::F(1) => Some(KittyFunctionalKey::CsiLetter(b'P')),
        RuntimeKey::F(2) => Some(KittyFunctionalKey::CsiLetter(b'Q')),
        RuntimeKey::F(3) => Some(KittyFunctionalKey::CsiLetter(b'R')),
        RuntimeKey::F(4) => Some(KittyFunctionalKey::CsiLetter(b'S')),
        RuntimeKey::Insert => Some(KittyFunctionalKey::CsiTilde(2)),
        RuntimeKey::Delete => Some(KittyFunctionalKey::CsiTilde(3)),
        RuntimeKey::PageUp => Some(KittyFunctionalKey::CsiTilde(5)),
        RuntimeKey::PageDown => Some(KittyFunctionalKey::CsiTilde(6)),
        RuntimeKey::F(5) => Some(KittyFunctionalKey::CsiTilde(15)),
        RuntimeKey::F(6) => Some(KittyFunctionalKey::CsiTilde(17)),
        RuntimeKey::F(7) => Some(KittyFunctionalKey::CsiTilde(18)),
        RuntimeKey::F(8) => Some(KittyFunctionalKey::CsiTilde(19)),
        RuntimeKey::F(9) => Some(KittyFunctionalKey::CsiTilde(20)),
        RuntimeKey::F(10) => Some(KittyFunctionalKey::CsiTilde(21)),
        RuntimeKey::F(11) => Some(KittyFunctionalKey::CsiTilde(23)),
        RuntimeKey::F(12) => Some(KittyFunctionalKey::CsiTilde(24)),
        RuntimeKey::F(index @ 13..=20) => Some(KittyFunctionalKey::CsiUnicode(
            KITTY_PUA_FUNCTION_KEY_BASE + u32::from(index - 13),
        )),
        _ => None,
    }
}

fn encode_kitty_functional_key(
    key_event: &RuntimeKeyEvent,
    flags: u16,
    event_type: Option<u8>,
) -> Option<Vec<u8>> {
    let modifier_field = kitty_modifier_field(key_event.modifiers, event_type);

    match kitty_functional_key(key_event.key)? {
        KittyFunctionalKey::CsiLetter(letter) => {
            if let Some(modifier_field) = modifier_field {
                Some(format!("\x1b[1;{modifier_field}{}", letter as char).into_bytes())
            } else {
                Some(format!("\x1b[{}", letter as char).into_bytes())
            }
        }
        KittyFunctionalKey::CsiTilde(number) => {
            if let Some(modifier_field) = modifier_field {
                Some(format!("\x1b[{number};{modifier_field}~").into_bytes())
            } else {
                Some(format!("\x1b[{number}~").into_bytes())
            }
        }
        KittyFunctionalKey::CsiUnicode(codepoint) => {
            Some(kitty_csi_u(codepoint, key_event, flags, event_type))
        }
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

#[cfg(test)]
mod tests {
    use super::{
        RuntimeKey, RuntimeKeyEvent, RuntimeKeyEventKind, RuntimeKeyModifiers, TerminalModeFlags,
        cursor_key, encode_ctrl_letter, encode_runtime_key_event, fkey_ss3_modified,
        tilde_modified, xterm_modifier_param,
    };

    fn key_event(key: RuntimeKey, modifiers: RuntimeKeyModifiers) -> RuntimeKeyEvent {
        RuntimeKeyEvent::new(key, modifiers)
    }

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
        let no_mods = RuntimeKeyModifiers::default();

        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Up, no_mods), modes),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Down, no_mods), modes),
            Some(b"\x1bOB".to_vec())
        );
        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Right, no_mods), modes),
            Some(b"\x1bOC".to_vec())
        );
        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Left, no_mods), modes),
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
            control: true,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Up, ctrl), modes),
            Some(b"\x1b[1;5A".to_vec())
        );
    }

    #[test]
    fn legacy_character_encoding_uses_associated_text() {
        let shift = RuntimeKeyModifiers {
            shift: true,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('a'), shift)
            .with_shifted_key(Some('A'))
            .with_associated_text("A");

        assert_eq!(
            encode_runtime_key_event(key_event, TerminalModeFlags::default()),
            Some(vec![b'A'])
        );
    }

    #[test]
    fn kitty_mode_encodes_regular_keys_as_csi_u() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('a'), RuntimeKeyModifiers::default());

        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_enter_as_csi_13_u() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Enter, RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[13u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_tab_as_csi_u() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Tab, RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[9u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_backspace_as_csi_u() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Backspace, RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[127u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_escape_as_csi_u() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Escape, RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[27u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_ctrl_a_with_modifier() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let ctrl = RuntimeKeyModifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Character('a'), ctrl), modes),
            Some(b"\x1b[97;5u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_encodes_shift_tab_with_modifier() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let shift = RuntimeKeyModifiers {
            shift: true,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Tab, shift), modes),
            Some(b"\x1b[9;2u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_arrows_use_legacy_encoding() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 1,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Up, RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[A".to_vec())
        );
    }

    #[test]
    fn kitty_mode_report_event_types_encodes_repeat_and_release() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b11,
            ..Default::default()
        };
        let repeat = key_event(RuntimeKey::Escape, RuntimeKeyModifiers::default())
            .with_kind(RuntimeKeyEventKind::Repeat);
        let release = key_event(RuntimeKey::Up, RuntimeKeyModifiers::default())
            .with_kind(RuntimeKeyEventKind::Release);

        assert_eq!(
            encode_runtime_key_event(repeat, modes),
            Some(b"\x1b[27;1:2u".to_vec())
        );
        assert_eq!(
            encode_runtime_key_event(release, modes),
            Some(b"\x1b[1;1:3A".to_vec())
        );
    }

    #[test]
    fn kitty_mode_report_alternates_encodes_shifted_key() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b101,
            ..Default::default()
        };
        let shift = RuntimeKeyModifiers {
            shift: true,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('='), shift).with_shifted_key(Some('+'));

        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[61:43;2u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_report_alternates_encodes_base_layout_key_without_shifted_key() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b101,
            ..Default::default()
        };
        let ctrl = RuntimeKeyModifiers {
            control: true,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('с'), ctrl).with_base_layout_key(Some('c'));

        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some("\x1b[1089::99;5u".as_bytes().to_vec())
        );
    }

    #[test]
    fn kitty_mode_report_associated_text_requires_report_all_keys() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b1_0001,
            ..Default::default()
        };
        let shift = RuntimeKeyModifiers {
            shift: true,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('a'), shift)
            .with_shifted_key(Some('A'))
            .with_associated_text("A");

        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97;2u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_report_associated_text_embeds_utf8_text() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b1_1001,
            ..Default::default()
        };
        let shift = RuntimeKeyModifiers {
            shift: true,
            ..Default::default()
        };
        let key_event = key_event(RuntimeKey::Character('a'), shift)
            .with_shifted_key(Some('A'))
            .with_associated_text("A");

        assert_eq!(
            encode_runtime_key_event(key_event, modes),
            Some(b"\x1b[97;2;65u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_disallows_release_for_text_keys_without_report_all_keys() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b11,
            ..Default::default()
        };
        let release = key_event(RuntimeKey::Character('a'), RuntimeKeyModifiers::default())
            .with_kind(RuntimeKeyEventKind::Release);

        assert_eq!(encode_runtime_key_event(release, modes), None);
    }

    #[test]
    fn kitty_mode_report_all_keys_uses_canonical_function_key_forms() {
        let modes = TerminalModeFlags {
            kitty_keyboard_flags: 0b1001,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::F(1), RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[P".to_vec())
        );
        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::F(13), RuntimeKeyModifiers::default()),
                modes
            ),
            Some(b"\x1b[57376u".to_vec())
        );
    }

    #[test]
    fn kitty_mode_disabled_uses_legacy() {
        let modes = TerminalModeFlags::default();

        assert_eq!(
            encode_runtime_key_event(
                key_event(RuntimeKey::Character('a'), RuntimeKeyModifiers::default()),
                modes
            ),
            Some(vec![b'a'])
        );
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
        let key_event = key_event(
            RuntimeKey::Backspace,
            RuntimeKeyModifiers {
                alt: true,
                ..Default::default()
            },
        );

        assert_eq!(
            encode_runtime_key_event(key_event.clone(), disabled),
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
            kitty_keyboard_flags: 1,
            ..Default::default()
        };
        let super_key = RuntimeKeyModifiers {
            super_key: true,
            ..Default::default()
        };

        assert_eq!(
            encode_runtime_key_event(key_event(RuntimeKey::Character('a'), super_key), modes),
            Some(b"\x1b[97;9u".to_vec())
        );
    }
}
