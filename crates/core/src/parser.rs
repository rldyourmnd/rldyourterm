use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};

const MAX_CSI_LEN: usize = 64;
const MAX_OSC_LEN: usize = 512;
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

    fn handle_ground_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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

    fn handle_escape_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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

    fn handle_csi_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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

    fn handle_csi_discard_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
        self.csi_dropped = self.csi_dropped.saturating_add(1);
        if is_csi_final_byte(byte) {
            self.emit_oversized_csi(actions);
            self.reset_state_to_ground();
        }
    }

    fn handle_osc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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

    fn handle_osc_discard_byte(&mut self, byte: u8, _actions: &mut Vec<ParserAction>) {
        match byte {
            0x07 => self.reset_state_to_ground(),
            0x1B => self.state = ParseState::OscEsc,
            _ => {}
        }
    }

    fn handle_osc_esc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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

    fn handle_dcs_byte(&mut self, byte: u8) {
        if byte == 0x1B {
            self.state = ParseState::DcsEsc;
        }
        // All other bytes are silently absorbed (DCS payload)
    }

    fn handle_dcs_esc_byte(&mut self, byte: u8, actions: &mut Vec<ParserAction>) {
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
        if let Some(action) = parse_osc(&raw) {
            actions.push(action);
        }
        self.reset_state_to_ground();
    }

    fn complete_csi(&mut self, actions: &mut Vec<ParserAction>) {
        let (&final_byte, params_raw) = match self.csi_buffer.split_last() {
            Some(pair) => pair,
            None => {
                self.reset_state_to_ground();
                return;
            }
        };

        if let Some((&b'?', rest)) = params_raw.split_first() {
            self.dispatch_private_csi(rest, final_byte, actions);
            self.reset_state_to_ground();
            return;
        }

        let action = self
            .parse_standard_csi_action(params_raw, final_byte)
            .unwrap_or_else(|| {
                ParserAction::UnsupportedSequence(self.csi_sequence_string(&self.csi_buffer))
            });
        actions.push(action);
        self.reset_state_to_ground();
    }

    fn emit_oversized_csi(&self, actions: &mut Vec<ParserAction>) {
        actions.push(ParserAction::IngestDegraded {
            reason: IngestDegradeReason::CsiSequenceTooLong,
            accepted: MAX_CSI_LEN,
            dropped: self.csi_dropped.max(1),
        });
        let sequence = format!("{}...", self.csi_sequence_string(&self.csi_buffer));
        actions.push(ParserAction::UnsupportedSequence(sequence));
    }

    fn reset_state_to_ground(&mut self) {
        self.state = ParseState::Ground;
        self.csi_buffer.clear();
        self.csi_dropped = 0;
        self.osc_buffer.clear();
    }

    fn parse_standard_csi_action(&self, params_raw: &[u8], final_byte: u8) -> Option<ParserAction> {
        let parsed = parse_params(params_raw).ok()?;

        match final_byte {
            b'A' => Some(ParserAction::CursorUp(step_param(&parsed))),
            b'B' => Some(ParserAction::CursorDown(step_param(&parsed))),
            b'C' => Some(ParserAction::CursorForward(step_param(&parsed))),
            b'D' => Some(ParserAction::CursorBack(step_param(&parsed))),
            b'E' => Some(ParserAction::CursorNextLine(step_param(&parsed))),
            b'F' => Some(ParserAction::CursorPreviousLine(step_param(&parsed))),
            b'H' | b'f' => {
                let row = position_param(&parsed, 0);
                let col = position_param(&parsed, 1);
                Some(ParserAction::CursorPosition { row, col })
            }
            b'G' => {
                let col = position_param(&parsed, 0);
                Some(ParserAction::CursorHorizontalAbsolute(col))
            }
            b'd' => {
                let row = position_param(&parsed, 0);
                Some(ParserAction::VerticalPositionAbsolute(row))
            }
            b'J' => {
                let mode = mode_param(&parsed)?;
                Some(ParserAction::ClearDisplay(match mode {
                    0 => DisplayClearMode::Below,
                    1 => DisplayClearMode::Above,
                    2 => DisplayClearMode::All,
                    3 => DisplayClearMode::Scrollback,
                    _ => return None,
                }))
            }
            b'K' => {
                let mode = mode_param(&parsed)?;
                Some(ParserAction::ClearLine(match mode {
                    0 => LineClearMode::Right,
                    1 => LineClearMode::Left,
                    2 => LineClearMode::All,
                    _ => return None,
                }))
            }
            b'm' => Some(ParserAction::SetGraphicsRendition(SgrParams::from_slice(
                parsed.as_slice(),
            ))),
            b's' => Some(ParserAction::CursorSavePosition),
            b'u' => Some(ParserAction::CursorRestorePosition),
            b'L' => Some(ParserAction::InsertLines(step_param(&parsed))),
            b'M' => Some(ParserAction::DeleteLines(step_param(&parsed))),
            b'S' => Some(ParserAction::ScrollUp(step_param(&parsed))),
            b'T' => Some(ParserAction::ScrollDown(step_param(&parsed))),
            b'X' => Some(ParserAction::EraseCharacters(step_param(&parsed))),
            b'@' => Some(ParserAction::InsertCharacters(step_param(&parsed))),
            b'P' => Some(ParserAction::DeleteCharacters(step_param(&parsed))),
            b'r' => {
                let top = position_param(&parsed, 0);
                let bottom = if parsed.len() >= 2 {
                    parsed.get(1).flatten().map(|value| value - 1)
                } else {
                    None
                };
                Some(ParserAction::SetScrollRegion { top, bottom })
            }
            b'c' => {
                let param = parsed.first().and_then(|p| p).unwrap_or(0);
                if param == 0 {
                    Some(ParserAction::SendPrimaryDA)
                } else {
                    None
                }
            }
            b'n' => {
                let param = parsed.first().and_then(|p| p)?;
                match param {
                    5 => Some(ParserAction::SendDeviceOk),
                    6 => Some(ParserAction::SendDeviceStatusReport),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn dispatch_private_csi(
        &self,
        params_raw: &[u8],
        final_byte: u8,
        actions: &mut Vec<ParserAction>,
    ) {
        let parsed = match parse_params(params_raw) {
            Ok(p) => p,
            Err(()) => return,
        };
        let mut dispatched = false;
        for i in 0..parsed.len() {
            if let Some(Some(mode)) = parsed.get(i)
                && let Some(action) = self.private_mode_action(mode, final_byte)
            {
                actions.push(action);
                dispatched = true;
            }
        }
        if !dispatched {
            actions.push(ParserAction::UnsupportedSequence(
                self.csi_sequence_string(&self.csi_buffer),
            ));
        }
    }

    fn private_mode_action(&self, mode: u16, final_byte: u8) -> Option<ParserAction> {
        match (mode, final_byte) {
            (1, b'h') => Some(ParserAction::ApplicationCursorKeys(true)),
            (1, b'l') => Some(ParserAction::ApplicationCursorKeys(false)),
            (7, b'h') => Some(ParserAction::AutoWrapMode(true)),
            (7, b'l') => Some(ParserAction::AutoWrapMode(false)),
            (25, b'h') => Some(ParserAction::SetCursorVisible(true)),
            (25, b'l') => Some(ParserAction::SetCursorVisible(false)),
            (1049, b'h') => Some(ParserAction::AlternateScreenEnter),
            (1049, b'l') => Some(ParserAction::AlternateScreenLeave),
            (2004, b'h') => Some(ParserAction::BracketedPasteMode(true)),
            (2004, b'l') => Some(ParserAction::BracketedPasteMode(false)),
            _ => None,
        }
    }

    fn csi_sequence_string(&self, payload: &[u8]) -> String {
        let mut raw = Vec::with_capacity(payload.len() + 2);
        raw.push(0x1B);
        raw.push(b'[');
        raw.extend_from_slice(payload);
        String::from_utf8_lossy(&raw).into_owned()
    }

    fn flush_text_buffer(&mut self, actions: &mut Vec<ParserAction>, allow_incomplete_tail: bool) {
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
            self.text_buffer.drain(0..consumed);
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
            Some(second) => {
                let mut s = String::with_capacity(text.len());
                s.push(first);
                s.push(second);
                s.extend(chars);
                actions.push(ParserAction::PrintText(s));
            }
        },
    }
}

fn parse_osc(raw: &str) -> Option<ParserAction> {
    let (code_str, payload) = raw.split_once(';')?;
    let code: u16 = code_str.parse().ok()?;
    match code {
        0 | 2 => Some(ParserAction::SetWindowTitle(payload.to_string())),
        _ => None,
    }
}

fn parse_params(input: &[u8]) -> Result<CsiParams, ()> {
    let mut params = [None; MAX_CSI_PARAMS];
    let mut count: u8 = 0;
    if input.is_empty() {
        return Ok(CsiParams { params, len: 0 });
    }
    let mut current: Option<u32> = None;
    for &byte in input {
        match byte {
            b'0'..=b'9' => {
                let digit = u32::from(byte - b'0');
                let next = current
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit);
                current = Some(next.min(u16::MAX as u32));
            }
            b';' => {
                if count as usize >= MAX_CSI_PARAMS {
                    return Err(());
                }
                params[count as usize] = current.map(|v| v as u16);
                count += 1;
                current = None;
            }
            _ => return Err(()),
        }
    }
    if count as usize >= MAX_CSI_PARAMS {
        return Err(());
    }
    params[count as usize] = current.map(|v| v as u16);
    count += 1;
    Ok(CsiParams { params, len: count })
}

fn step_param(parsed: &CsiParams) -> u16 {
    parsed.first().and_then(|p| p).unwrap_or(1).max(1)
}

fn position_param(parsed: &CsiParams, idx: usize) -> u16 {
    parsed
        .get(idx)
        .and_then(|p| p)
        .unwrap_or(1)
        .max(1)
        .saturating_sub(1)
}

fn mode_param(parsed: &CsiParams) -> Option<u16> {
    let mode = parsed.first()?.unwrap_or(0);
    if mode <= 3 { Some(mode) } else { None }
}

const fn is_csi_final_byte(byte: u8) -> bool {
    byte >= 0x40 && byte <= 0x7E
}

#[cfg(test)]
mod tests {
    use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};

    use super::{MAX_CSI_LEN, MAX_OSC_LEN, MAX_SGR_PARAMS, Parser, ParserAction, SgrParams};

    #[test]
    fn parses_printable_and_basic_controls() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"ab\r\n\x07\x08");

        assert_eq!(
            actions,
            vec![
                ParserAction::PrintText("ab".into()),
                ParserAction::CarriageReturn,
                ParserAction::LineFeed,
                ParserAction::Bell,
                ParserAction::Backspace,
            ]
        );
    }

    #[test]
    fn parses_supported_csi_subset() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[2A\x1b[10;3H\x1b[2J\x1b[1K");

        assert_eq!(
            actions,
            vec![
                ParserAction::CursorUp(2),
                ParserAction::CursorPosition { row: 9, col: 2 },
                ParserAction::ClearDisplay(DisplayClearMode::All),
                ParserAction::ClearLine(LineClearMode::Left),
            ]
        );
    }

    #[test]
    fn supports_split_escape_sequence_across_feeds() {
        let mut parser = Parser::default();

        assert!(parser.feed(b"\x1b[12").is_empty());
        let actions = parser.feed(b"D");
        assert_eq!(actions, vec![ParserAction::CursorBack(12)]);
    }

    #[test]
    fn incomplete_utf8_is_buffered_and_completed_on_next_feed() {
        let mut parser = Parser::default();

        assert!(parser.feed(&[0xE2, 0x82]).is_empty());
        let actions = parser.feed(&[0xAC]);

        assert_eq!(actions, vec![ParserAction::Print('\u{20AC}')]);
    }

    #[test]
    fn malformed_utf8_never_panics_and_emits_replacement_char() {
        let mut parser = Parser::default();
        let actions = parser.feed(&[0xF0, 0x28, 0x8C, 0x28]);

        assert!(
            actions
                .iter()
                .any(|action| matches!(action, ParserAction::Print('\u{FFFD}')))
        );
    }

    #[test]
    fn parses_cursor_visibility() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?25l\x1b[?25h");
        assert_eq!(
            actions,
            vec![
                ParserAction::SetCursorVisible(false),
                ParserAction::SetCursorVisible(true),
            ]
        );
    }

    #[test]
    fn parses_alternate_screen() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?1049h\x1b[?1049l");
        assert_eq!(
            actions,
            vec![
                ParserAction::AlternateScreenEnter,
                ParserAction::AlternateScreenLeave,
            ]
        );
    }

    #[test]
    fn parses_sgr_reset() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[0m");
        assert_eq!(
            actions,
            vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
                &[Some(0)]
            ))]
        );
    }

    #[test]
    fn parses_sgr_bold_red() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[1;31m");
        assert_eq!(
            actions,
            vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
                &[Some(1), Some(31)]
            ))]
        );
    }

    #[test]
    fn parses_sgr_256_color() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[38;5;196m");
        assert_eq!(
            actions,
            vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
                &[Some(38), Some(5), Some(196),]
            ))]
        );
    }

    #[test]
    fn parses_sgr_truecolor() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[38;2;255;128;0m");
        assert_eq!(
            actions,
            vec![ParserAction::SetGraphicsRendition(SgrParams::from_slice(
                &[Some(38), Some(2), Some(255), Some(128), Some(0),]
            ))]
        );
    }

    #[test]
    fn parses_tab() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\t");
        assert_eq!(actions, vec![ParserAction::Tab]);
    }

    #[test]
    fn parses_cursor_save_restore_esc() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b7\x1b8");
        assert_eq!(
            actions,
            vec![
                ParserAction::CursorSavePosition,
                ParserAction::CursorRestorePosition,
            ]
        );
    }

    #[test]
    fn parses_cursor_save_restore_csi() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[s\x1b[u");
        assert_eq!(
            actions,
            vec![
                ParserAction::CursorSavePosition,
                ParserAction::CursorRestorePosition,
            ]
        );
    }

    #[test]
    fn parses_osc_window_title() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b]0;Hello World\x07");
        assert_eq!(
            actions,
            vec![ParserAction::SetWindowTitle("Hello World".to_string())]
        );
    }

    #[test]
    fn parses_osc_title_code_2() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b]2;Title\x07");
        assert_eq!(
            actions,
            vec![ParserAction::SetWindowTitle("Title".to_string())]
        );
    }

    #[test]
    fn parses_csi_horizontal_absolute() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[10G");
        assert_eq!(actions, vec![ParserAction::CursorHorizontalAbsolute(9)]);
    }

    #[test]
    fn parses_cursor_next_line() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[3E");
        assert_eq!(actions, vec![ParserAction::CursorNextLine(3)]);
    }

    #[test]
    fn parses_cursor_previous_line() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[2F");
        assert_eq!(actions, vec![ParserAction::CursorPreviousLine(2)]);
    }

    #[test]
    fn parses_vertical_position_absolute() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[5d");
        assert_eq!(actions, vec![ParserAction::VerticalPositionAbsolute(4)]);
    }

    #[test]
    fn parses_insert_delete_lines() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[3L\x1b[2M");
        assert_eq!(
            actions,
            vec![ParserAction::InsertLines(3), ParserAction::DeleteLines(2)]
        );
    }

    #[test]
    fn parses_scroll_up_down() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[5S\x1b[3T");
        assert_eq!(
            actions,
            vec![ParserAction::ScrollUp(5), ParserAction::ScrollDown(3)]
        );
    }

    #[test]
    fn parses_erase_insert_delete_chars() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[4X\x1b[2@\x1b[3P");
        assert_eq!(
            actions,
            vec![
                ParserAction::EraseCharacters(4),
                ParserAction::InsertCharacters(2),
                ParserAction::DeleteCharacters(3),
            ]
        );
    }

    #[test]
    fn parses_scroll_region() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[5;20r");
        assert_eq!(
            actions,
            vec![ParserAction::SetScrollRegion {
                top: 4,
                bottom: Some(19)
            }]
        );
    }

    #[test]
    fn parses_scroll_region_with_empty_bottom_as_none() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[5;r");
        assert_eq!(
            actions,
            vec![ParserAction::SetScrollRegion {
                top: 4,
                bottom: None
            }]
        );
    }

    #[test]
    fn oversized_csi_sequence_is_degraded_without_panicking() {
        let mut parser = Parser::default();

        let mut payload = vec![0x1B, b'['];
        payload.extend(std::iter::repeat_n(b'1', MAX_CSI_LEN + 3));
        assert!(parser.feed(&payload).is_empty());

        let actions = parser.feed(b"AZ");

        assert_eq!(actions.len(), 3);
        assert!(matches!(
            actions[0],
            ParserAction::IngestDegraded {
                reason: IngestDegradeReason::CsiSequenceTooLong,
                accepted,
                dropped
            } if accepted == MAX_CSI_LEN && dropped == 4
        ));
        assert!(matches!(actions[1], ParserAction::UnsupportedSequence(_)));
        assert_eq!(actions[2], ParserAction::Print('Z'));
    }

    #[test]
    fn resync_after_truncation_resets_escape_state() {
        let mut parser = Parser::default();
        assert!(parser.feed(b"\x1b[12").is_empty());

        let actions = parser.resync_after_truncation();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ParserAction::UnsupportedSequence(_)));

        assert_eq!(parser.feed(b"A"), vec![ParserAction::Print('A')]);
    }

    #[test]
    fn resync_after_truncation_flushes_incomplete_utf8_tail() {
        let mut parser = Parser::default();
        assert!(parser.feed(&[0xE2, 0x82]).is_empty());

        assert_eq!(
            parser.resync_after_truncation(),
            vec![ParserAction::Print('\u{FFFD}')]
        );
        assert_eq!(parser.feed(b"B"), vec![ParserAction::Print('B')]);
    }

    #[test]
    fn unknown_private_mode_is_unsupported() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?9999h");
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ParserAction::UnsupportedSequence(_)));
    }

    #[test]
    fn oversized_osc_is_discarded() {
        let mut parser = Parser::default();
        let mut payload = vec![0x1B, b']', b'0', b';'];
        payload.extend(std::iter::repeat_n(b'X', 600));
        payload.push(0x07);
        let actions = parser.feed(&payload);
        // Should not crash; OSC was too long and discarded
        assert!(actions.is_empty());
    }

    #[test]
    fn dcs_payload_is_silently_absorbed() {
        let mut parser = Parser::default();
        // Simulates fish XTGETTCAP: DCS +q696e646e ST
        let actions = parser.feed(b"\x1bP+q696e646e\x1b\\");
        // DCS payload should be completely absorbed, no actions emitted
        assert!(actions.is_empty());
    }

    #[test]
    fn dcs_followed_by_normal_text() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1bP+q71756572792d6f732d6e616d65\x1b\\Hello");
        // DCS absorbed, then "Hello" printed as batch
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ParserAction::PrintText(t) if t == "Hello"))
        );
    }

    #[test]
    fn incomplete_dcs_discarded_on_resync() {
        let mut parser = Parser::default();
        // Start DCS but don't terminate it
        let actions = parser.feed(b"\x1bPsome_payload");
        assert!(actions.is_empty());
        // Resync should discard the incomplete DCS
        let resync = parser.resync_after_truncation();
        assert!(resync.is_empty());
        // Parser should be back in Ground state
        let actions = parser.feed(b"A");
        assert_eq!(actions, vec![ParserAction::Print('A')]);
    }

    #[test]
    fn sgr_bare_m_means_reset() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[m");
        assert_eq!(
            actions,
            vec![ParserAction::SetGraphicsRendition(SgrParams::default())]
        );
    }

    #[test]
    fn osc_st_terminator_no_stray_backslash() {
        let mut parser = Parser::default();
        // OSC title with ST terminator (ESC \)
        let actions = parser.feed(b"\x1b]0;Title\x1b\\");
        assert_eq!(
            actions,
            vec![ParserAction::SetWindowTitle("Title".to_string())]
        );
        // No Print('\\') should appear
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, ParserAction::Print('\\')))
        );
    }

    #[test]
    fn osc_st_followed_by_normal_text() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b]0;Hello\x1b\\ABC");
        assert_eq!(
            actions,
            vec![
                ParserAction::SetWindowTitle("Hello".to_string()),
                ParserAction::PrintText("ABC".into()),
            ]
        );
    }

    #[test]
    fn parses_reverse_index() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1bM");
        assert_eq!(actions, vec![ParserAction::ReverseIndex]);
    }

    #[test]
    fn parses_next_line() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1bE");
        assert_eq!(actions, vec![ParserAction::NextLine]);
    }

    #[test]
    fn parses_application_keypad_mode() {
        let mut parser = Parser::default();
        let enable = parser.feed(b"\x1b=");
        assert_eq!(enable, vec![ParserAction::ApplicationKeypadMode(true)]);
        let disable = parser.feed(b"\x1b>");
        assert_eq!(disable, vec![ParserAction::ApplicationKeypadMode(false)]);
    }

    #[test]
    fn parses_clear_scrollback() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[3J");
        assert_eq!(
            actions,
            vec![ParserAction::ClearDisplay(DisplayClearMode::Scrollback)]
        );
    }

    #[test]
    fn parses_bracketed_paste_mode() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?2004h\x1b[?2004l");
        assert_eq!(
            actions,
            vec![
                ParserAction::BracketedPasteMode(true),
                ParserAction::BracketedPasteMode(false),
            ]
        );
    }

    #[test]
    fn parses_application_cursor_keys() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?1h\x1b[?1l");
        assert_eq!(
            actions,
            vec![
                ParserAction::ApplicationCursorKeys(true),
                ParserAction::ApplicationCursorKeys(false),
            ]
        );
    }

    #[test]
    fn parses_auto_wrap_mode() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?7h\x1b[?7l");
        assert_eq!(
            actions,
            vec![
                ParserAction::AutoWrapMode(true),
                ParserAction::AutoWrapMode(false),
            ]
        );
    }

    #[test]
    fn parses_primary_da_query() {
        let mut parser = Parser::default();
        // \e[c (no params)
        let actions = parser.feed(b"\x1b[c");
        assert_eq!(actions, vec![ParserAction::SendPrimaryDA]);
        // \e[0c (explicit param 0)
        let actions = parser.feed(b"\x1b[0c");
        assert_eq!(actions, vec![ParserAction::SendPrimaryDA]);
    }

    #[test]
    fn parses_device_status_report() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[6n");
        assert_eq!(actions, vec![ParserAction::SendDeviceStatusReport]);
    }

    #[test]
    fn parses_device_ok_query() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[5n");
        assert_eq!(actions, vec![ParserAction::SendDeviceOk]);
    }

    // ── Coverage gap tests: boundary conditions ────────────────

    #[test]
    fn csi_at_exactly_max_len_is_accepted() {
        let mut parser = Parser::default();
        // Build a CSI sequence that fills exactly MAX_CSI_LEN bytes
        // CSI buffer starts accumulating after ESC[, so we need MAX_CSI_LEN
        // bytes of param+final. Use params that fill up to MAX_CSI_LEN-1
        // bytes, then the final byte 'm' as the last byte.
        let mut payload = vec![0x1B, b'['];
        // Fill with '1;' pairs to approach MAX_CSI_LEN-1 param bytes
        let param_bytes = MAX_CSI_LEN - 1; // leave room for final byte 'm'
        for i in 0..param_bytes {
            if i % 2 == 0 {
                payload.push(b'1');
            } else {
                payload.push(b';');
            }
        }
        payload.push(b'm'); // final byte, this is the MAX_CSI_LEN-th byte
        let actions = parser.feed(&payload);
        // Should parse as SGR (not degrade) since it fits exactly in buffer
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
            "CSI at exactly MAX_CSI_LEN should be accepted, got: {actions:?}"
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, ParserAction::IngestDegraded { .. })),
            "CSI at exactly MAX_CSI_LEN should not degrade"
        );
    }

    #[test]
    fn csi_one_byte_over_max_len_degrades() {
        let mut parser = Parser::default();
        let mut payload = vec![0x1B, b'['];
        // Fill MAX_CSI_LEN bytes of params (no final byte yet)
        payload.extend(std::iter::repeat_n(b'1', MAX_CSI_LEN));
        // Now add the final byte - this is the (MAX_CSI_LEN+1)-th byte
        payload.push(b'm');
        let actions = parser.feed(&payload);
        assert!(
            actions.iter().any(|a| matches!(
                a,
                ParserAction::IngestDegraded {
                    reason: IngestDegradeReason::CsiSequenceTooLong,
                    ..
                }
            )),
            "CSI one byte over MAX_CSI_LEN should degrade"
        );
    }

    #[test]
    fn osc_at_exactly_max_len_is_accepted() {
        let mut parser = Parser::default();
        // OSC format: ESC ] <id> ; <payload> BEL
        // OSC buffer accumulates bytes after ESC ]
        let prefix = b"0;";
        let fill_len = MAX_OSC_LEN - prefix.len();
        let mut payload = vec![0x1B, b']'];
        payload.extend_from_slice(prefix);
        payload.extend(std::iter::repeat_n(b'T', fill_len));
        payload.push(0x07); // BEL terminator
        let actions = parser.feed(&payload);
        // Should parse as SetWindowTitle since it fits exactly
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
            "OSC at exactly MAX_OSC_LEN should be accepted, got: {actions:?}"
        );
    }

    #[test]
    fn osc_one_byte_over_max_len_is_discarded() {
        let mut parser = Parser::default();
        let prefix = b"0;";
        let fill_len = MAX_OSC_LEN - prefix.len() + 1; // one byte over
        let mut payload = vec![0x1B, b']'];
        payload.extend_from_slice(prefix);
        payload.extend(std::iter::repeat_n(b'T', fill_len));
        payload.push(0x07); // BEL
        let actions = parser.feed(&payload);
        // Should be discarded (no title set)
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, ParserAction::SetWindowTitle(_))),
            "OSC one byte over MAX_OSC_LEN should be discarded"
        );
    }

    #[test]
    fn sgr_with_exactly_max_params() {
        let mut parser = Parser::default();
        // Build SGR with exactly MAX_SGR_PARAMS params: ESC[1;1;1;...;1m
        let params_str = vec!["1"; MAX_SGR_PARAMS].join(";");
        let seq = format!("\x1b[{params_str}m");
        let actions = parser.feed(seq.as_bytes());
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ParserAction::SetGraphicsRendition(sgr) if sgr.as_slice().len() == MAX_SGR_PARAMS)),
            "SGR with exactly MAX_SGR_PARAMS should preserve all params"
        );
    }

    #[test]
    fn sgr_with_params_beyond_max_truncates_silently() {
        let mut parser = Parser::default();
        // Build SGR with MAX_SGR_PARAMS + 4 params
        let params_str = vec!["1"; MAX_SGR_PARAMS + 4].join(";");
        let seq = format!("\x1b[{params_str}m");
        let actions = parser.feed(seq.as_bytes());
        // Should still parse (not crash), extra params truncated
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
            "SGR with excess params should still parse"
        );
    }

    #[test]
    fn mixed_utf8_and_csi_across_feed_boundaries() {
        let mut parser = Parser::default();
        // Send first 2 bytes of a 3-byte UTF-8 char (€ = E2 82 AC)
        let actions1 = parser.feed(&[0xE2, 0x82]);
        assert!(actions1.is_empty(), "Incomplete UTF-8 should buffer");
        // Complete the UTF-8 char, then immediately start a CSI
        let actions2 = parser.feed(&[0xAC, 0x1B, b'[']);
        assert!(
            actions2.contains(&ParserAction::Print('€')),
            "€ should be printed after completion"
        );
        // Complete the CSI in a third feed
        let actions3 = parser.feed(b"31mA");
        assert!(
            actions3
                .iter()
                .any(|a| matches!(a, ParserAction::SetGraphicsRendition(_))),
            "CSI should complete across feeds"
        );
        assert!(
            actions3.contains(&ParserAction::Print('A')),
            "'A' should print after CSI"
        );
    }

    #[test]
    fn empty_csi_params_use_defaults() {
        let mut parser = Parser::default();
        // ESC[H with no params should default to (1,1)
        let actions = parser.feed(b"\x1b[H");
        assert_eq!(
            actions,
            vec![ParserAction::CursorPosition { row: 0, col: 0 }]
        );
        // ESC[;H with both params empty should also default
        let actions = parser.feed(b"\x1b[;H");
        assert_eq!(
            actions,
            vec![ParserAction::CursorPosition { row: 0, col: 0 }]
        );
    }

    #[test]
    fn multi_param_private_csi_dispatches_all_modes() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?1;25h");
        assert!(actions.contains(&ParserAction::ApplicationCursorKeys(true)));
        assert!(actions.contains(&ParserAction::SetCursorVisible(true)));
    }

    #[test]
    fn multi_param_private_csi_reset() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"\x1b[?1;25;2004l");
        assert_eq!(actions.len(), 3);
        assert!(actions.contains(&ParserAction::ApplicationCursorKeys(false)));
        assert!(actions.contains(&ParserAction::SetCursorVisible(false)));
        assert!(actions.contains(&ParserAction::BracketedPasteMode(false)));
    }
}
