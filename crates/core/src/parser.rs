use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};

const MAX_CSI_LEN: usize = 64;
const REPLACEMENT_CHAR: char = '\u{FFFD}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserAction {
    Print(char),
    LineFeed,
    CarriageReturn,
    Bell,
    Backspace,
    CursorUp(u16),
    CursorDown(u16),
    CursorForward(u16),
    CursorBack(u16),
    CursorPosition {
        row: u16,
        col: u16,
    },
    ClearDisplay(DisplayClearMode),
    ClearLine(LineClearMode),
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
}

#[derive(Debug, Default)]
pub struct Parser {
    state: ParseState,
    text_buffer: Vec<u8>,
    csi_buffer: Vec<u8>,
    csi_dropped: usize,
}

impl Parser {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<ParserAction> {
        let mut actions = Vec::new();
        for &byte in bytes {
            match self.state {
                ParseState::Ground => self.handle_ground_byte(byte, &mut actions),
                ParseState::Escape => self.handle_escape_byte(byte, &mut actions),
                ParseState::Csi => self.handle_csi_byte(byte, &mut actions),
                ParseState::CsiDiscard => self.handle_csi_discard_byte(byte, &mut actions),
            }
        }
        self.flush_text_buffer(&mut actions, true);
        actions
    }

    pub fn resync_after_truncation(&mut self) -> Vec<ParserAction> {
        let mut actions = Vec::new();
        self.flush_text_buffer(&mut actions, false);

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
            ParseState::CsiDiscard => self.emit_oversized_csi(&mut actions),
        }

        self.reset_state_to_ground();
        actions
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
            0x00..=0x1F | 0x7F => {
                // Unsupported control bytes are ignored by design, but still
                // terminate any pending UTF-8 run deterministically.
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

    fn complete_csi(&mut self, actions: &mut Vec<ParserAction>) {
        let action = self.parse_csi_action().unwrap_or_else(|| {
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
    }

    fn parse_csi_action(&self) -> Option<ParserAction> {
        let (&final_byte, params) = self.csi_buffer.split_last()?;
        let parsed = parse_params(params).ok()?;

        match final_byte {
            b'A' => Some(ParserAction::CursorUp(step_param(&parsed))),
            b'B' => Some(ParserAction::CursorDown(step_param(&parsed))),
            b'C' => Some(ParserAction::CursorForward(step_param(&parsed))),
            b'D' => Some(ParserAction::CursorBack(step_param(&parsed))),
            b'H' | b'f' => {
                let row = position_param(&parsed, 0);
                let col = position_param(&parsed, 1);
                Some(ParserAction::CursorPosition { row, col })
            }
            b'J' => {
                let mode = mode_param(&parsed)?;
                Some(ParserAction::ClearDisplay(match mode {
                    0 => DisplayClearMode::Below,
                    1 => DisplayClearMode::Above,
                    2 => DisplayClearMode::All,
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
                    actions.extend(valid.chars().map(ParserAction::Print));
                    consumed = self.text_buffer.len();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        match std::str::from_utf8(&slice[..valid_up_to]) {
                            Ok(prefix) => actions.extend(prefix.chars().map(ParserAction::Print)),
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

fn parse_params(input: &[u8]) -> Result<Vec<Option<u16>>, ()> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
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
                out.push(current.map(|value| value as u16));
                current = None;
            }
            _ => return Err(()),
        }
    }

    out.push(current.map(|value| value as u16));
    Ok(out)
}

fn step_param(params: &[Option<u16>]) -> u16 {
    params.first().copied().flatten().unwrap_or(1).max(1)
}

fn position_param(params: &[Option<u16>], idx: usize) -> u16 {
    params
        .get(idx)
        .copied()
        .flatten()
        .unwrap_or(1)
        .max(1)
        .saturating_sub(1)
}

fn mode_param(params: &[Option<u16>]) -> Option<u16> {
    let mode = params.first().copied().flatten().unwrap_or(0);
    if mode <= 2 { Some(mode) } else { None }
}

const fn is_csi_final_byte(byte: u8) -> bool {
    byte >= 0x40 && byte <= 0x7E
}

#[cfg(test)]
mod tests {
    use crate::events::{DisplayClearMode, IngestDegradeReason, LineClearMode};

    use super::{MAX_CSI_LEN, Parser, ParserAction};

    #[test]
    fn parses_printable_and_basic_controls() {
        let mut parser = Parser::default();
        let actions = parser.feed(b"ab\r\n\x07\x08");

        assert_eq!(
            actions,
            vec![
                ParserAction::Print('a'),
                ParserAction::Print('b'),
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
    fn invalid_or_unsupported_sequences_are_reported() {
        let mut parser = Parser::default();

        let actions = parser.feed(b"\x1b[?25l\x1bP");
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], ParserAction::UnsupportedSequence(_)));
        assert!(matches!(actions[1], ParserAction::UnsupportedSequence(_)));
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
}
