// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::collections::VecDeque;

use crate::grid::Cell;

pub const MAX_SCROLLBACK_CAP: usize = 50_000;
pub(crate) const DEFAULT_SCROLLBACK_BYTE_CAP: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrollback {
    cap: usize,
    byte_cap: usize,
    byte_len: usize,
    lines: VecDeque<String>,
}

impl Scrollback {
    pub fn new(cap: usize) -> Self {
        Self::with_byte_cap(cap, DEFAULT_SCROLLBACK_BYTE_CAP)
    }

    pub fn with_byte_cap(cap: usize, byte_cap: usize) -> Self {
        let cap = cap.min(MAX_SCROLLBACK_CAP);
        let byte_cap = if cap == 0 { 0 } else { byte_cap.max(1) };
        Self {
            cap,
            byte_cap,
            byte_len: 0,
            lines: VecDeque::new(),
        }
    }

    pub const fn cap(&self) -> usize {
        self.cap
    }

    pub const fn byte_cap(&self) -> usize {
        self.byte_cap
    }

    pub const fn byte_len(&self) -> usize {
        self.byte_len
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.byte_len = 0;
        // Explicit clear should also release retained memory for long-running sessions.
        self.lines.shrink_to_fit();
    }

    pub fn push(&mut self, mut line: String) -> usize {
        if self.cap == 0 || self.byte_cap == 0 {
            return 0;
        }

        // Keep logical text while dropping right-side padding cells to reduce RAM.
        if line.as_bytes().last().copied() == Some(b' ') {
            let trimmed_len = line.trim_end_matches(' ').len();
            if trimmed_len < line.len() {
                line.truncate(trimmed_len);
            }
        }

        if line.len() > self.byte_cap {
            return 0;
        }

        self.byte_len = self.byte_len.saturating_add(line.len());
        self.lines.push_back(line);
        let mut dropped = 0usize;
        while self.lines.len() > self.cap || self.byte_len > self.byte_cap {
            if let Some(removed) = self.lines.pop_front() {
                self.byte_len = self.byte_len.saturating_sub(removed.len());
            }
            dropped += 1;
        }
        dropped
    }

    /// Push a row of cells directly into scrollback, avoiding an intermediate
    /// `row_string` allocation in the scroll hot path.
    pub fn push_from_cells(&mut self, cells: &[Cell]) -> usize {
        if self.cap == 0 || self.byte_cap == 0 {
            return 0;
        }

        let mut line = String::with_capacity(cells.len());
        for cell in cells {
            if cell.width == 0 {
                continue;
            }
            line.push(cell.ch);
        }

        self.push(line)
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SCROLLBACK_BYTE_CAP, MAX_SCROLLBACK_CAP, Scrollback};

    #[test]
    fn push_trims_oldest_lines_when_cap_exceeded() {
        let mut scrollback = Scrollback::new(2);

        assert_eq!(scrollback.push("l1".to_string()), 0);
        assert_eq!(scrollback.push("l2".to_string()), 0);
        assert_eq!(scrollback.push("l3".to_string()), 1);

        assert_eq!(scrollback.len(), 2);
        assert_eq!(scrollback.get(0), Some("l2"));
        assert_eq!(scrollback.get(1), Some("l3"));
    }

    #[test]
    fn cap_zero_discards_every_line() {
        let mut scrollback = Scrollback::new(0);

        assert_eq!(scrollback.push("l1".to_string()), 0);
        assert_eq!(scrollback.push("l2".to_string()), 0);
        assert!(scrollback.is_empty());
    }

    #[test]
    fn cap_is_clamped_to_policy_maximum() {
        let scrollback = Scrollback::new(MAX_SCROLLBACK_CAP + 10_000);
        assert_eq!(scrollback.cap(), MAX_SCROLLBACK_CAP);
        assert_eq!(scrollback.byte_cap(), DEFAULT_SCROLLBACK_BYTE_CAP);
    }

    #[test]
    fn push_compacts_trailing_padding_spaces() {
        let mut scrollback = Scrollback::new(2);
        assert_eq!(scrollback.push("abc   ".to_string()), 0);
        assert_eq!(scrollback.get(0), Some("abc"));
    }

    #[test]
    fn byte_cap_trims_oldest_lines_when_budget_is_exceeded() {
        let mut scrollback = Scrollback::with_byte_cap(10, 5);

        assert_eq!(scrollback.push("aaaa".to_string()), 0);
        assert_eq!(scrollback.byte_len(), 4);

        assert_eq!(scrollback.push("bb".to_string()), 1);
        assert_eq!(scrollback.len(), 1);
        assert_eq!(scrollback.byte_len(), 2);
        assert_eq!(scrollback.get(0), Some("bb"));
    }

    #[test]
    fn oversized_line_is_dropped_when_it_exceeds_byte_budget() {
        let mut scrollback = Scrollback::with_byte_cap(10, 3);
        assert_eq!(scrollback.push("🚀".to_string()), 0);
        assert!(scrollback.is_empty());
        assert_eq!(scrollback.byte_len(), 0);
    }

    #[test]
    fn oversized_line_does_not_evict_existing_history() {
        let mut scrollback = Scrollback::with_byte_cap(10, 6);
        assert_eq!(scrollback.push("ab".to_string()), 0);
        assert_eq!(scrollback.push("cd".to_string()), 0);
        assert_eq!(scrollback.push("toolong".to_string()), 0);

        assert_eq!(scrollback.len(), 2);
        assert_eq!(scrollback.get(0), Some("ab"));
        assert_eq!(scrollback.get(1), Some("cd"));
        assert_eq!(scrollback.byte_len(), 4);
    }

    #[test]
    fn clear_resets_byte_accounting() {
        let mut scrollback = Scrollback::with_byte_cap(10, 16);
        assert_eq!(scrollback.push("abcd".to_string()), 0);
        assert_eq!(scrollback.byte_len(), 4);
        scrollback.clear();
        assert_eq!(scrollback.byte_len(), 0);
    }

    #[test]
    fn push_from_cells_skips_continuation_cells() {
        use crate::grid::{Attrs, Cell};

        let cells = vec![
            Cell {
                ch: 'A',
                attrs: Attrs::default(),
                width: 1,
            },
            Cell {
                ch: '\u{6F22}',
                attrs: Attrs::default(),
                width: 2,
            },
            Cell {
                ch: '\u{6F22}',
                attrs: Attrs::default(),
                width: 0,
            }, // continuation
            Cell {
                ch: 'B',
                attrs: Attrs::default(),
                width: 1,
            },
        ];
        let mut scrollback = Scrollback::new(10);
        assert_eq!(scrollback.push_from_cells(&cells), 0);
        // Continuation cell (width=0) should be skipped: "A漢B"
        assert_eq!(scrollback.get(0), Some("A\u{6F22}B"));
    }

    #[test]
    fn push_from_cells_handles_all_blanks() {
        use crate::grid::Cell;

        let cells = vec![Cell::default(); 5];
        let mut scrollback = Scrollback::new(10);
        assert_eq!(scrollback.push_from_cells(&cells), 0);
        // All blanks get trimmed by push()
        assert_eq!(scrollback.get(0), Some(""));
    }
}
