// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use std::collections::VecDeque;

use crate::grid::Cell;

pub const MAX_SCROLLBACK_CAP: usize = 50_000;
pub(crate) const DEFAULT_SCROLLBACK_BYTE_CAP: usize = 512 * 1024 * 1024;

/// Cell-based scrollback buffer that preserves full visual state (colors,
/// attributes) for each scrolled-out line. Uses dual-cap eviction: line count
/// and memory budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrollback {
    cap: usize,
    byte_cap: usize,
    byte_len: usize,
    lines: VecDeque<Vec<Cell>>,
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
        self.lines.shrink_to_fit();
    }

    /// Push a row of cells into scrollback, preserving full visual state.
    /// Trailing default blank cells are trimmed to save memory.
    /// Returns the number of lines evicted (0 if no eviction).
    pub fn push_from_cells(&mut self, cells: &[Cell]) -> usize {
        if self.cap == 0 || self.byte_cap == 0 {
            return 0;
        }

        // Trim trailing default blank cells to reduce memory.
        let trimmed_end = cells
            .iter()
            .rposition(|c| !c.is_blank_space() || c.attrs != Default::default() || c.width != 1)
            .map_or(0, |pos| pos + 1);

        // Skip continuation cells (width=0) since the owning wide cell is preserved.
        let row: Vec<Cell> = cells[..trimmed_end]
            .iter()
            .filter(|c| c.width != 0)
            .copied()
            .collect();

        let row_bytes = row.len() * std::mem::size_of::<Cell>();
        if row_bytes > self.byte_cap {
            return 0;
        }

        self.byte_len = self.byte_len.saturating_add(row_bytes);
        self.lines.push_back(row);

        let mut dropped = 0usize;
        while self.lines.len() > self.cap || self.byte_len > self.byte_cap {
            if let Some(removed) = self.lines.pop_front() {
                self.byte_len = self
                    .byte_len
                    .saturating_sub(removed.len() * std::mem::size_of::<Cell>());
            }
            dropped += 1;
        }
        dropped
    }

    /// Get a scrollback line as a slice of cells with full attributes.
    pub fn get(&self, index: usize) -> Option<&[Cell]> {
        self.lines.get(index).map(Vec::as_slice)
    }

    /// Get the text content of a scrollback line (for search/export).
    pub fn get_text(&self, index: usize) -> Option<String> {
        self.lines.get(index).map(|cells| {
            let mut text = String::with_capacity(cells.len());
            for cell in cells {
                cell.append_text_to(&mut text);
            }
            text
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &[Cell]> {
        self.lines.iter().map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SCROLLBACK_BYTE_CAP, MAX_SCROLLBACK_CAP, Scrollback};
    use crate::grid::{Attrs, Cell, Color};

    fn cells_from_str(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|ch| Cell {
                ch,
                attrs: Attrs::default(),
                width: 1,
            })
            .collect()
    }

    fn text_of(scrollback: &Scrollback, index: usize) -> Option<String> {
        scrollback.get_text(index)
    }

    #[test]
    fn push_trims_oldest_lines_when_cap_exceeded() {
        let mut scrollback = Scrollback::new(2);

        assert_eq!(scrollback.push_from_cells(&cells_from_str("l1")), 0);
        assert_eq!(scrollback.push_from_cells(&cells_from_str("l2")), 0);
        assert_eq!(scrollback.push_from_cells(&cells_from_str("l3")), 1);

        assert_eq!(scrollback.len(), 2);
        assert_eq!(text_of(&scrollback, 0).as_deref(), Some("l2"));
        assert_eq!(text_of(&scrollback, 1).as_deref(), Some("l3"));
    }

    #[test]
    fn cap_zero_discards_every_line() {
        let mut scrollback = Scrollback::new(0);

        assert_eq!(scrollback.push_from_cells(&cells_from_str("l1")), 0);
        assert_eq!(scrollback.push_from_cells(&cells_from_str("l2")), 0);
        assert!(scrollback.is_empty());
    }

    #[test]
    fn cap_is_clamped_to_policy_maximum() {
        let scrollback = Scrollback::new(MAX_SCROLLBACK_CAP + 10_000);
        assert_eq!(scrollback.cap(), MAX_SCROLLBACK_CAP);
        assert_eq!(scrollback.byte_cap(), DEFAULT_SCROLLBACK_BYTE_CAP);
    }

    #[test]
    fn push_trims_trailing_default_blank_cells() {
        let cells = cells_from_str("abc   ");
        // Make the trailing spaces truly default (they already are from cells_from_str)
        let mut scrollback = Scrollback::new(2);
        assert_eq!(scrollback.push_from_cells(&cells), 0);
        assert_eq!(text_of(&scrollback, 0).as_deref(), Some("abc"));
    }

    #[test]
    fn push_preserves_styled_trailing_spaces() {
        let mut cells = cells_from_str("ab ");
        cells[2].attrs = Attrs::default().with_bold();
        let mut scrollback = Scrollback::new(2);
        assert_eq!(scrollback.push_from_cells(&cells), 0);
        // Bold space is NOT trimmed because it has non-default attrs
        let row = scrollback.get(0).unwrap();
        assert_eq!(row.len(), 3);
        assert!(row[2].attrs.bold());
    }

    #[test]
    fn push_from_cells_skips_continuation_cells() {
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
        let row = scrollback.get(0).unwrap();
        assert_eq!(row.len(), 3); // A, 漢, B (no continuation)
        assert_eq!(row[0].ch, 'A');
        assert_eq!(row[1].ch, '\u{6F22}');
        assert_eq!(row[2].ch, 'B');
    }

    #[test]
    fn push_from_cells_handles_all_blanks() {
        let cells = vec![Cell::default(); 5];
        let mut scrollback = Scrollback::new(10);
        assert_eq!(scrollback.push_from_cells(&cells), 0);
        // All default blanks get trimmed
        let row = scrollback.get(0).unwrap();
        assert!(row.is_empty());
    }

    #[test]
    fn cell_attrs_preserved_in_scrollback() {
        let mut cells = cells_from_str("red");
        for cell in &mut cells {
            cell.attrs = Attrs::default().with_fg(Color::Indexed(196)).with_bold();
        }
        let mut scrollback = Scrollback::new(10);
        scrollback.push_from_cells(&cells);

        let row = scrollback.get(0).unwrap();
        assert_eq!(row[0].attrs.fg, Color::Indexed(196));
        assert!(row[0].attrs.bold());
        assert_eq!(row[1].ch, 'e');
    }

    #[test]
    fn clear_resets_byte_accounting() {
        let mut scrollback = Scrollback::with_byte_cap(10, 4096);
        scrollback.push_from_cells(&cells_from_str("abcd"));
        assert!(scrollback.byte_len() > 0);
        scrollback.clear();
        assert_eq!(scrollback.byte_len(), 0);
    }

    #[test]
    fn byte_cap_trims_oldest_lines() {
        // Each Cell is 20 bytes. "ab" = 2 cells = 40 bytes.
        let cell_size = std::mem::size_of::<Cell>();
        let mut scrollback = Scrollback::with_byte_cap(10, cell_size * 3);

        scrollback.push_from_cells(&cells_from_str("ab")); // 2 cells = 40B
        assert_eq!(scrollback.len(), 1);

        scrollback.push_from_cells(&cells_from_str("cd")); // total 80B > 60B cap
        assert_eq!(scrollback.len(), 1); // oldest evicted
        assert_eq!(text_of(&scrollback, 0).as_deref(), Some("cd"));
    }
}
