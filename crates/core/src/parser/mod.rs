// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod csi;
mod handlers;

#[cfg(test)]
mod tests_advanced;
#[cfg(test)]
mod tests_basic;
#[cfg(test)]
mod tests_stress;

use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};
use crate::state::{MouseFormat, MouseMode};

const MAX_CSI_LEN: usize = 256;
const MAX_OSC_LEN: usize = 4096;
const MAX_DCS_LEN: usize = 4096;
const REPLACEMENT_CHAR: char = '\u{FFFD}';

pub const MAX_SGR_PARAMS: usize = 16;
pub const MAX_SGR_SUBPARAMS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SgrParam {
    value: Option<u16>,
    subparams: [Option<u16>; MAX_SGR_SUBPARAMS],
    sub_len: u8,
}

impl SgrParam {
    pub fn new(value: Option<u16>) -> Self {
        Self {
            value,
            ..Self::default()
        }
    }

    pub fn with_subparams(value: Option<u16>, subparams: &[Option<u16>]) -> Self {
        let mut stored = [None; MAX_SGR_SUBPARAMS];
        let sub_len = subparams.len().min(MAX_SGR_SUBPARAMS);
        stored[..sub_len].copy_from_slice(&subparams[..sub_len]);
        Self {
            value,
            subparams: stored,
            sub_len: sub_len as u8,
        }
    }

    pub fn value(&self) -> Option<u16> {
        self.value
    }

    pub fn subparams(&self) -> &[Option<u16>] {
        &self.subparams[..self.sub_len as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SgrParams {
    params: [Option<u16>; MAX_SGR_PARAMS],
    subparams: [[Option<u16>; MAX_SGR_SUBPARAMS]; MAX_SGR_PARAMS],
    sub_lens: [u8; MAX_SGR_PARAMS],
    len: u8,
}

impl SgrParams {
    pub fn from_slice(slice: &[Option<u16>]) -> Self {
        let mut params = [None; MAX_SGR_PARAMS];
        let len = slice.len().min(MAX_SGR_PARAMS);
        params[..len].copy_from_slice(&slice[..len]);
        Self {
            params,
            subparams: [[None; MAX_SGR_SUBPARAMS]; MAX_SGR_PARAMS],
            sub_lens: [0; MAX_SGR_PARAMS],
            len: len as u8,
        }
    }

    pub fn from_params(slice: &[SgrParam]) -> Self {
        let mut params = [None; MAX_SGR_PARAMS];
        let mut subparams = [[None; MAX_SGR_SUBPARAMS]; MAX_SGR_PARAMS];
        let mut sub_lens = [0; MAX_SGR_PARAMS];
        let len = slice.len().min(MAX_SGR_PARAMS);

        for (index, param) in slice.iter().take(len).enumerate() {
            params[index] = param.value();
            let stored_subparams = param.subparams();
            sub_lens[index] = stored_subparams.len() as u8;
            subparams[index][..stored_subparams.len()].copy_from_slice(stored_subparams);
        }

        Self {
            params,
            subparams,
            sub_lens,
            len: len as u8,
        }
    }

    pub fn as_slice(&self) -> &[Option<u16>] {
        &self.params[..self.len as usize]
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, idx: usize) -> Option<SgrParam> {
        if idx >= self.len() {
            return None;
        }

        let sub_len = self.sub_lens[idx] as usize;
        Some(SgrParam {
            value: self.params[idx],
            subparams: self.subparams[idx],
            sub_len: sub_len as u8,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = SgrParam> + '_ {
        (0..self.len()).filter_map(|idx| self.get(idx))
    }
}

impl Default for SgrParams {
    fn default() -> Self {
        Self {
            params: [None; MAX_SGR_PARAMS],
            subparams: [[None; MAX_SGR_SUBPARAMS]; MAX_SGR_PARAMS],
            sub_lens: [0; MAX_SGR_PARAMS],
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusStringRequest {
    Sgr,
    CursorStyle,
    ScrollRegion,
    Unsupported,
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
    CursorBackwardTab(u16),
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
    SetOriginMode(bool),
    SetGraphemeClusterMode(bool),
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
    SendSecondaryDA,
    SendXtversion,
    SendWindowSizeChars,
    SendWindowSizePixels,
    RequestModeReport(u16),
    SendDeviceStatusReport,
    SendDeviceOk,
    HyperlinkStart {
        uri: String,
    },
    HyperlinkEnd,
    SetPaletteColor {
        index: u8,
        rgb: (u8, u8, u8),
    },
    QueryPaletteColor(u8),
    ResetPaletteColor(u8),
    ResetPalette,
    QueryForegroundColor,
    QueryBackgroundColor,
    RequestStatusString(StatusStringRequest),
    PushKittyKeyboardMode(u16),
    PopKittyKeyboardMode(u16),
    QueryKittyKeyboardMode,
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
    dcs_buffer: Vec<u8>,
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
            if self.state != ParseState::Ground && matches!(byte, 0x18 | 0x1A) {
                self.reset_state_to_ground();
                continue;
            }
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
                actions.push(ParserAction::UnsupportedSequence(String::from("\x1B")));
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
