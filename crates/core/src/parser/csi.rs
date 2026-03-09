use crate::events::{DisplayClearMode, LineClearMode};

use crate::state::{MouseFormat, MouseMode};

use super::{CsiParams, MAX_CSI_PARAMS, Parser, ParserAction, SgrParams};

impl Parser {
    pub(super) fn complete_csi(&mut self, actions: &mut Vec<ParserAction>) {
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

        // ECMA-48: intermediate bytes (0x20-0x2F) appear between params and final byte.
        // Split off the trailing intermediate byte to dispatch CSI sequences like DECSCUSR.
        if let Some((&intermediate, numeric_params)) = params_raw.split_last()
            && (0x20..=0x2F).contains(&intermediate)
        {
            let action = self
                .parse_csi_with_intermediate(numeric_params, intermediate, final_byte)
                .unwrap_or_else(|| {
                    ParserAction::UnsupportedSequence(self.csi_sequence_string(&self.csi_buffer))
                });
            actions.push(action);
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
                    parsed
                        .get(1)
                        .flatten()
                        .filter(|&value| value > 0)
                        .map(|value| value.saturating_sub(1))
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
            b'b' => Some(ParserAction::RepeatLastChar(step_param(&parsed))),
            b'g' => {
                let mode = parsed.first().and_then(|p| p).unwrap_or(0);
                Some(ParserAction::TabClear(mode))
            }
            _ => None,
        }
    }

    fn parse_csi_with_intermediate(
        &self,
        params_raw: &[u8],
        intermediate: u8,
        final_byte: u8,
    ) -> Option<ParserAction> {
        let parsed = parse_params(params_raw).ok()?;
        match (intermediate, final_byte) {
            // DECSCUSR: CSI Ps SP q — set cursor shape (0=reset, 1=blink block,
            // 2=steady block, 3=blink underline, 4=steady underline,
            // 5=blink bar, 6=steady bar)
            (b' ', b'q') => {
                let shape = parsed.first().and_then(|p| p).unwrap_or(0);
                Some(ParserAction::SetCursorShape(shape.min(6) as u8))
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
            (12, b'h') => Some(ParserAction::SetCursorBlink(true)),
            (12, b'l') => Some(ParserAction::SetCursorBlink(false)),
            (47, b'h') | (1047, b'h') => Some(ParserAction::AlternateScreenEnterSimple),
            (47, b'l') | (1047, b'l') => Some(ParserAction::AlternateScreenLeaveSimple),
            (1000, b'h') => Some(ParserAction::SetMouseMode(MouseMode::Basic)),
            (1000, b'l') => Some(ParserAction::SetMouseMode(MouseMode::Off)),
            (1002, b'h') => Some(ParserAction::SetMouseMode(MouseMode::ButtonTrack)),
            (1002, b'l') => Some(ParserAction::SetMouseMode(MouseMode::Off)),
            (1003, b'h') => Some(ParserAction::SetMouseMode(MouseMode::AnyEvent)),
            (1003, b'l') => Some(ParserAction::SetMouseMode(MouseMode::Off)),
            (1004, b'h') => Some(ParserAction::SetFocusReporting(true)),
            (1004, b'l') => Some(ParserAction::SetFocusReporting(false)),
            (1006, b'h') => Some(ParserAction::SetMouseFormat(MouseFormat::Sgr)),
            (1006, b'l') => Some(ParserAction::SetMouseFormat(MouseFormat::Normal)),
            (1048, b'h') => Some(ParserAction::CursorSavePositionDec),
            (1048, b'l') => Some(ParserAction::CursorRestorePositionDec),
            (2004, b'h') => Some(ParserAction::BracketedPasteMode(true)),
            (2004, b'l') => Some(ParserAction::BracketedPasteMode(false)),
            (2026, b'h') => Some(ParserAction::SetSynchronizedOutput(true)),
            (2026, b'l') => Some(ParserAction::SetSynchronizedOutput(false)),
            _ => None,
        }
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
