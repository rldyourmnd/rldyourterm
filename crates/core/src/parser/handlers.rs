// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::events::IngestDegradeReason;

use super::{MAX_CSI_LEN, MAX_OSC_LEN, ParseState, Parser, ParserAction, REPLACEMENT_CHAR};

impl Parser {
    pub(super) fn handle_ground_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        match byte {
            0x1B => {
                self.flush_text_buffer(actions, false);
                self.state = ParseState::Escape;
            }
            b'\n' => {
                self.flush_text_buffer(actions, false);
                actions.push(ParserAction::LineFeed);
            }
            b'\r' => {
                self.flush_text_buffer(actions, false);
                actions.push(ParserAction::CarriageReturn);
            }
            0x07 => {
                self.flush_text_buffer(actions, false);
                actions.push(ParserAction::Bell);
            }
            0x08 => {
                self.flush_text_buffer(actions, false);
                actions.push(ParserAction::Backspace);
            }
            0x09 => {
                self.flush_text_buffer(actions, false);
                actions.push(ParserAction::Tab);
            }
            0x00..=0x1F | 0x7F => {
                self.flush_text_buffer(actions, false);
            }
            _ => self.text_buffer.push(byte),
        }
    }

    pub(super) fn handle_escape_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        match byte {
            b'[' => {
                self.csi_buffer.clear();
                self.csi_dropped = 0;
                self.state = ParseState::Csi;
            }
            b']' => {
                self.osc_buffer.clear();
                self.state = ParseState::Osc;
            }
            b'7' => {
                actions.push(ParserAction::CursorSavePosition);
                self.state = ParseState::Ground;
            }
            b'8' => {
                actions.push(ParserAction::CursorRestorePosition);
                self.state = ParseState::Ground;
            }
            b'M' => {
                actions.push(ParserAction::ReverseIndex);
                self.state = ParseState::Ground;
            }
            b'E' => {
                actions.push(ParserAction::NextLine);
                self.state = ParseState::Ground;
            }
            b'=' => {
                actions.push(ParserAction::ApplicationKeypadMode(true));
                self.state = ParseState::Ground;
            }
            b'>' => {
                actions.push(ParserAction::ApplicationKeypadMode(false));
                self.state = ParseState::Ground;
            }
            b'H' => {
                actions.push(ParserAction::HorizontalTabSet);
                self.state = ParseState::Ground;
            }
            b'P' => {
                // DCS (Device Control String) - absorb payload until ST
                self.state = ParseState::Dcs;
            }
            _ => {
                let raw = [0x1B, byte];
                actions.push(ParserAction::UnsupportedSequence(
                    String::from_utf8_lossy(&raw).into_owned(),
                ));
                self.state = ParseState::Ground;
            }
        }
    }

    pub(super) fn handle_csi_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        if self.csi_buffer.len() < MAX_CSI_LEN {
            self.csi_buffer.push(byte);
            if is_csi_final_byte(byte) {
                self.complete_csi(actions);
            }
            return;
        }

        self.csi_dropped = self.csi_dropped.saturating_add(1);
        if is_csi_final_byte(byte) {
            self.emit_oversized_csi(actions);
            self.reset_state_to_ground();
            return;
        }

        self.state = ParseState::CsiDiscard;
    }

    pub(super) fn handle_csi_discard_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        self.csi_dropped = self.csi_dropped.saturating_add(1);
        if is_csi_final_byte(byte) {
            self.emit_oversized_csi(actions);
            self.reset_state_to_ground();
        }
    }

    pub(super) fn handle_osc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        match byte {
            0x07 => {
                // BEL terminates OSC
                self.complete_osc(actions);
            }
            0x1B => {
                // Potential ST (ESC \) - transition to intermediate state
                self.state = ParseState::OscEsc;
            }
            _ => {
                if self.osc_buffer.len() < MAX_OSC_LEN {
                    self.osc_buffer.push(byte);
                } else {
                    self.state = ParseState::OscDiscard;
                }
            }
        }
    }

    pub(super) fn handle_osc_discard_byte(&mut self, byte: u8, _actions: &mut Vec<ParserAction>) {
        match byte {
            0x07 => self.reset_state_to_ground(),
            0x1B => self.state = ParseState::OscEsc,
            _ => {}
        }
    }

    pub(super) fn handle_osc_esc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        if byte == b'\\' {
            // ST complete (ESC \)
            self.complete_osc(actions);
        } else {
            // Bare ESC inside OSC - complete OSC, re-process byte as new escape sequence
            self.complete_osc(actions);
            // The byte following ESC could start a new sequence
            self.state = ParseState::Escape;
            self.handle_escape_byte(byte, actions);
        }
    }

    pub(super) fn handle_dcs_byte(&mut self, byte: u8) {
        if byte == 0x1B {
            self.state = ParseState::DcsEsc;
        }
        // All other bytes are silently absorbed (DCS payload)
    }

    pub(super) fn handle_dcs_esc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        if byte == b'\\' {
            // ST complete (ESC \) - DCS terminated, return to Ground
            self.state = ParseState::Ground;
        } else {
            // Not ST - the ESC starts a new escape sequence outside DCS
            self.state = ParseState::Escape;
            self.handle_escape_byte(byte, actions);
        }
    }

    fn complete_osc(&mut self, actions: &mut Vec<ParserAction>) {
        let raw = String::from_utf8_lossy(&self.osc_buffer).into_owned();
        actions.extend(parse_osc(&raw));
        self.reset_state_to_ground();
    }

    pub(super) fn emit_oversized_csi(&self, actions: &mut Vec<ParserAction>) {
        actions.push(ParserAction::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted: MAX_CSI_LEN,
            dropped: self.csi_dropped.max(1),
        });
        let sequence = format!("{}...", self.csi_sequence_string(&self.csi_buffer));
        actions.push(ParserAction::UnsupportedSequence(sequence));
    }

    pub(super) fn reset_state_to_ground(&mut self) {
        self.state = ParseState::Ground;
        self.csi_buffer.clear();
        self.csi_dropped = 0;
        self.osc_buffer.clear();
    }

    pub(super) fn flush_text_buffer(
        &mut self,
        actions: &mut Vec<ParserAction>,
        allow_incomplete_tail: bool,
    ) {
        if self.text_buffer.is_empty() {
            return;
        }

        let mut consumed = 0usize;
        loop {
            let slice = &self.text_buffer[consumed..];
            if slice.is_empty() {
                break;
            }

            match std::str::from_utf8(slice) {
                Ok(valid) => {
                    emit_text(valid, actions);
                    consumed = self.text_buffer.len();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        match std::str::from_utf8(&slice[..valid_up_to]) {
                            Ok(prefix) => emit_text(prefix, actions),
                            Err(_) => actions.push(ParserAction::Print(REPLACEMENT_CHAR)),
                        }
                    }
                    consumed += valid_up_to;

                    match err.error_len() {
                        Some(error_len) => {
                            actions.push(ParserAction::Print(REPLACEMENT_CHAR));
                            consumed += error_len;
                        }
                        None => {
                            if allow_incomplete_tail {
                                break;
                            }
                            actions.push(ParserAction::Print(REPLACEMENT_CHAR));
                            consumed = self.text_buffer.len();
                            break;
                        }
                    }
                }
            }
        }

        if consumed > 0 {
            if consumed == self.text_buffer.len() {
                self.text_buffer.clear();
            } else {
                self.text_buffer.drain(0..consumed);
            }
        }

        if !allow_incomplete_tail && !self.text_buffer.is_empty() {
            actions.push(ParserAction::Print(REPLACEMENT_CHAR));
            self.text_buffer.clear();
        }
    }
}

fn emit_text(text: &str, actions: &mut Vec<ParserAction>) {
    let mut chars = text.chars();
    match chars.next() {
        None => {}
        Some(first) => match chars.next() {
            None => actions.push(ParserAction::Print(first)),
            Some(_) => {
                // text is already valid UTF-8; to_owned() is a single memcpy
                // vs the previous char-by-char push+extend rebuild.
                actions.push(ParserAction::PrintText(text.to_owned()));
            }
        },
    }
}

fn parse_osc(raw: &str) -> Vec<ParserAction> {
    let (code_str, payload) = raw
        .split_once(';')
        .map_or((raw, ""), |(code, payload)| (code, payload));
    let Ok(code) = code_str.parse::<u16>() else {
        return Vec::new();
    };
    match code {
        0 | 2 => vec![ParserAction::SetWindowTitle(payload.to_string())],
        4 => parse_osc_4_palette(payload),
        7 => parse_osc_7_cwd(payload).into_iter().collect(),
        8 => parse_osc_8_hyperlink(payload).into_iter().collect(),
        10 if payload == "?" => vec![ParserAction::QueryForegroundColor],
        11 if payload == "?" => vec![ParserAction::QueryBackgroundColor],
        52 => parse_osc_52_clipboard(payload).into_iter().collect(),
        104 => parse_osc_104_palette_reset(payload),
        133 => parse_osc_133_shell_marker(payload).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn parse_osc_4_palette(payload: &str) -> Vec<ParserAction> {
    let mut actions = Vec::new();
    let mut parts = payload.split(';');

    loop {
        let Some(index_str) = parts.next() else {
            break;
        };
        let Some(spec) = parts.next() else {
            break;
        };
        let Ok(index) = index_str.parse::<u16>() else {
            continue;
        };
        if index > u8::MAX as u16 {
            continue;
        }
        let index = index as u8;

        if spec == "?" {
            actions.push(ParserAction::QueryPaletteColor(index));
            continue;
        }

        if let Some(rgb) = parse_osc_color_spec(spec) {
            actions.push(ParserAction::SetPaletteColor { index, rgb });
        }
    }

    actions
}

fn parse_osc_104_palette_reset(payload: &str) -> Vec<ParserAction> {
    if payload.is_empty() {
        return vec![ParserAction::ResetPalette];
    }

    let mut actions = Vec::new();
    for index_str in payload.split(';').filter(|part| !part.is_empty()) {
        let Ok(index) = index_str.parse::<u16>() else {
            continue;
        };
        if index <= u8::MAX as u16 {
            actions.push(ParserAction::ResetPaletteColor(index as u8));
        }
    }
    actions
}

fn parse_osc_color_spec(spec: &str) -> Option<(u8, u8, u8)> {
    if let Some(rgb_spec) = spec.strip_prefix("rgb:") {
        return parse_rgb_triplet(rgb_spec, '/');
    }

    spec.strip_prefix('#').and_then(parse_hash_triplet)
}

fn parse_rgb_triplet(spec: &str, separator: char) -> Option<(u8, u8, u8)> {
    let mut parts = spec.split(separator);
    let red = parse_hex_component(parts.next()?)?;
    let green = parse_hex_component(parts.next()?)?;
    let blue = parse_hex_component(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some((red, green, blue))
}

fn parse_hash_triplet(spec: &str) -> Option<(u8, u8, u8)> {
    let component_len = spec.len() / 3;
    if spec.is_empty() || spec.len() % 3 != 0 || !(1..=4).contains(&component_len) {
        return None;
    }

    let red = parse_hex_component(&spec[..component_len])?;
    let green = parse_hex_component(&spec[component_len..component_len * 2])?;
    let blue = parse_hex_component(&spec[component_len * 2..])?;
    Some((red, green, blue))
}

fn parse_hex_component(component: &str) -> Option<u8> {
    if component.is_empty() || component.len() > 4 {
        return None;
    }

    let value = u16::from_str_radix(component, 16).ok()? as u32;
    let max_value = (1u32 << (component.len() * 4)) - 1;
    Some(((value * 255 + max_value / 2) / max_value) as u8)
}

fn parse_osc_7_cwd(payload: &str) -> Option<ParserAction> {
    // OSC 7 ; file://hostname/path ST
    // Extract path from file:// URI, or accept raw path
    let path = if let Some(rest) = payload.strip_prefix("file://") {
        // Skip hostname (up to next '/')
        rest.find('/').map(|idx| &rest[idx..]).unwrap_or(rest)
    } else {
        payload
    };
    if path.is_empty() {
        return None;
    }
    Some(ParserAction::SetCurrentWorkingDirectory(path.to_string()))
}

fn parse_osc_8_hyperlink(payload: &str) -> Option<ParserAction> {
    // OSC 8 ; params ; uri ST
    let (_, uri) = payload.split_once(';')?;
    if uri.is_empty() {
        Some(ParserAction::HyperlinkEnd)
    } else {
        Some(ParserAction::HyperlinkStart {
            uri: uri.to_string(),
        })
    }
}

const OSC_52_MAX_DECODED_BYTES: usize = 100 * 1024;

fn parse_osc_52_clipboard(payload: &str) -> Option<ParserAction> {
    // OSC 52 ; selection ; base64-data ST
    let (selection_str, data) = payload.split_once(';')?;
    let selection = selection_str.chars().next().unwrap_or('c');
    if data.is_empty() || data == "?" {
        // Query or empty - not a set
        return None;
    }
    // Validate base64 length won't exceed decoded cap
    let estimated_decoded = data.len() * 3 / 4;
    if estimated_decoded > OSC_52_MAX_DECODED_BYTES {
        return None;
    }
    Some(ParserAction::ClipboardSet {
        selection,
        base64_data: data.to_string(),
    })
}

fn parse_osc_133_shell_marker(payload: &str) -> Option<ParserAction> {
    // OSC 133 ; A|B|C|D ST
    let marker = payload.chars().next()?;
    let kind = match marker {
        'A' => super::ShellMarkerKind::PromptStart,
        'B' => super::ShellMarkerKind::CommandStart,
        'C' => super::ShellMarkerKind::OutputStart,
        'D' => super::ShellMarkerKind::OutputEnd,
        _ => return None,
    };
    Some(ParserAction::ShellMarker(kind))
}

const fn is_csi_final_byte(byte: u8) -> bool {
    byte >= 0x40 && byte <= 0x7E
}
