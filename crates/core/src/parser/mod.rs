// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod csi;
mod handlers;

#[cfg(test)]
mod tests_advanced;
#[cfg(test)]
mod tests_basic;

use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};
use crate::state::{MouseFormat, MouseMode};

const MAX_CSI_LEN: usize = 64;
const MAX_OSC_LEN: usize = 4096;
const REPLACEMENT_CHAR: char = '\u{FFFD}';

pub const MAX_SGR_PARAMS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SgrParams {
    params: [Option<u16>; MAX_SGR_PARAMS],
    len: u8,
}

impl SgrParams {
    pub fn from_slice(slice: &[Option<u16>]) -> Self {
        let mut params = [None; MAX_SGR_PARAMS];
        let len = slice.len().min(MAX_SGR_PARAMS);
        params[..len].copy_from_slice(&slice[..len]);
        Self {
            params,
            len: len as u8,
        }
    }

    pub fn as_slice(&self) -> &[Option<u16>] {
        &self.params[..self.len as usize]
    }
}

impl Default for SgrParams {
    fn default() -> Self {
        Self {
            params: [None; MAX_SGR_PARAMS],
            len: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellMarkerKind {
    PromptStart,
    CommandStart,
    OutputStart,
    OutputEnd,
}

const MAX_CSI_PARAMS: usize = 32;

/// Stack-allocated CSI parameter array. ECMA-48 limits CSI to 16 parameters.
#[derive(Debug, Clone, Copy)]
struct CsiParams {
    params: [Option<u16>; MAX_CSI_PARAMS],
    len: u8,
}

impl CsiParams {
    fn get(&self, idx: usize) -> Option<Option<u16>> {
        if idx < self.len as usize {
            Some(self.params[idx])
        } else {
            None
        }
    }

    fn first(&self) -> Option<Option<u16>> {
        self.get(0)
    }

    fn as_slice(&self) -> &[Option<u16>] {
        &self.params[..self.len as usize]
    }

    fn len(&self) -> usize {
        self.len as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserAction {
    Print(char),
    PrintText(String),
    LineFeed,
    CarriageReturn,
    Bell,
    Backspace,
    Tab,
    CursorUp(u16),
    CursorDown(u16),
    CursorForward(u16),
    CursorBack(u16),
    CursorPosition {
        row: u16,
        col: u16,
    },
    CursorHorizontalAbsolute(u16),
    CursorNextLine(u16),
    CursorPreviousLine(u16),
    VerticalPositionAbsolute(u16),
    ClearDisplay(DisplayClearMode),
    ClearLine(LineClearMode),
    SetGraphicsRendition(SgrParams),
    CursorSavePosition,
    CursorRestorePosition,
    SetCursorVisible(bool),
    AlternateScreenEnter,
    AlternateScreenLeave,
    InsertLines(u16),
    DeleteLines(u16),
    ScrollUp(u16),
    ScrollDown(u16),
    EraseCharacters(u16),
    InsertCharacters(u16),
    DeleteCharacters(u16),
    SetScrollRegion {
        top: u16,
        bottom: Option<u16>,
    },
    SetWindowTitle(String),
    BracketedPasteMode(bool),
    ApplicationCursorKeys(bool),
    AutoWrapMode(bool),
    ReverseIndex,
    NextLine,
    ApplicationKeypadMode(bool),
    RepeatLastChar(u16),
    HorizontalTabSet,
    TabClear(u16),
    SetMouseMode(MouseMode),
    SetMouseFormat(MouseFormat),
    AlternateScreenEnterSimple,
    AlternateScreenLeaveSimple,
    CursorSavePositionDec,
    CursorRestorePositionDec,
    SetCursorBlink(bool),
    SetFocusReporting(bool),
    SetSynchronizedOutput(bool),
    SetCursorShape(u8),
    SetCurrentWorkingDirectory(String),
    ClipboardSet {
        selection: char,
        base64_data: String,
    },
    ShellMarker(ShellMarkerKind),
    SendPrimaryDA,
    SendDeviceStatusReport,
    SendDeviceOk,
    UnsupportedSequence(String),
    IngestDegraded {
        reason: IngestDegradeReason,
        accepted: usize,
        dropped: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ParseState {
    #[default]
    Ground,
    Escape,
    Csi,
    CsiDiscard,
    Osc,
    OscDiscard,
    OscEsc,
    /// DCS (Device Control String): absorb payload until ST terminator.
    Dcs,
    /// ESC seen inside DCS payload, waiting for `\` to form ST.
    DcsEsc,
}

#[derive(Debug, Default)]
pub struct Parser {
    state: ParseState,
    text_buffer: Vec<u8>,
    csi_buffer: Vec<u8>,
    csi_dropped: usize,
    osc_buffer: Vec<u8>,
}

impl Parser {
    #[cfg(test)]
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ParserAction> {
        let mut actions = Vec::new();
        self.feed_into(bytes, &mut actions);
        actions
    }

    pub fn feed_into(&mut self, bytes: &[u8], actions: &mut Vec<ParserAction>) {
        actions.clear();
        let expected = bytes.len() / 2;
        if actions.capacity() < expected {
            actions.reserve(expected - actions.capacity());
        }

        for &byte in bytes {
            match self.state {
                ParseState::Ground => self.handle_ground_byte(byte, actions),
                ParseState::Escape => self.handle_escape_byte(byte, actions),
                ParseState::Csi => self.handle_csi_byte(byte, actions),
                ParseState::CsiDiscard => self.handle_csi_discard_byte(byte, actions),
                ParseState::Osc => self.handle_osc_byte(byte, actions),
                ParseState::OscDiscard => self.handle_osc_discard_byte(byte, actions),
                ParseState::OscEsc => self.handle_osc_esc_byte(byte, actions),
                ParseState::Dcs => self.handle_dcs_byte(byte),
                ParseState::DcsEsc => self.handle_dcs_esc_byte(byte, actions),
            }
        }
        self.flush_text_buffer(actions, true);
    }

    #[cfg(test)]
    pub fn resync_after_truncation(&mut self) -> Vec<ParserAction> {
        let mut actions = Vec::new();
        self.resync_after_truncation_into(&mut actions);
        actions
    }

    pub fn resync_after_truncation_into(&mut self, actions: &mut Vec<ParserAction>) {
        actions.clear();
        self.flush_text_buffer(actions, false);

        match self.state {
            ParseState::Ground => {}
            ParseState::Escape => {
                actions.push(ParserAction::UnsupportedSequence(
                    String::from_utf8_lossy(&[0x1B]).into_owned(),
                ));
            }
            ParseState::Csi => {
                actions.push(ParserAction::UnsupportedSequence(
                    self.csi_sequence_string(&self.csi_buffer),
                ));
            }
            ParseState::CsiDiscard => self.emit_oversized_csi(actions),
            ParseState::Osc | ParseState::OscDiscard | ParseState::OscEsc => {
                // Discard incomplete OSC
            }
            ParseState::Dcs | ParseState::DcsEsc => {
                // Discard incomplete DCS
            }
        }

        self.reset_state_to_ground();
    }

    fn csi_sequence_string(&self, payload: &[u8]) -> String {
        let mut raw = Vec::with_capacity(payload.len() + 2);
        raw.push(0x1B);
        raw.push(b'[');
        raw.extend_from_slice(payload);
        String::from_utf8_lossy(&raw).into_owned()
    }
}
