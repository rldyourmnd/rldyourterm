// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::super::encode_winit_key_event;
use winit::keyboard::{Key, ModifiersState, NamedKey};

#[test]
fn encode_named_keys_without_modifiers() {
    let mods = ModifiersState::empty();

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F1), mods),
        Some(b"\x1bOP".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F5), mods),
        Some(b"\x1b[15~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::PageUp), mods),
        Some(b"\x1b[5~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Insert), mods),
        Some(b"\x1b[2~".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), mods),
        Some(b"\x1b[A".to_vec()),
    );
}

#[test]
fn encode_ctrl_arrow_produces_modified_csi() {
    let ctrl = ModifiersState::CONTROL;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), ctrl),
        Some(b"\x1b[1;5D".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowRight), ctrl),
        Some(b"\x1b[1;5C".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), ctrl),
        Some(b"\x1b[1;5A".to_vec()),
    );
    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowDown), ctrl),
        Some(b"\x1b[1;5B".to_vec()),
    );
}

#[test]
fn encode_shift_arrow_produces_modified_csi() {
    let shift = ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowUp), shift),
        Some(b"\x1b[1;2A".to_vec()),
    );
}

#[test]
fn encode_alt_arrow_produces_modified_csi() {
    let alt = ModifiersState::ALT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::ArrowLeft), alt),
        Some(b"\x1b[1;3D".to_vec()),
    );
}

#[test]
fn encode_ctrl_shift_f1_produces_modified_csi() {
    let ctrl_shift = ModifiersState::CONTROL | ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::F1), ctrl_shift),
        Some(b"\x1b[1;6P".to_vec()),
    );
}

#[test]
fn encode_shift_tab_produces_reverse_tab() {
    let shift = ModifiersState::SHIFT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Tab), shift),
        Some(b"\x1b[Z".to_vec()),
    );
}

#[test]
fn encode_alt_backspace_produces_esc_del() {
    let alt = ModifiersState::ALT;

    assert_eq!(
        encode_winit_key_event(&Key::Named(NamedKey::Backspace), alt),
        Some(b"\x1b\x7f".to_vec()),
    );
}
