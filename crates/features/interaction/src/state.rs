// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use winit::dpi::PhysicalPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridPoint {
    pub row: u16,
    pub col: u16,
}

impl GridPoint {
    pub const fn new(row: u16, col: u16) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectionRange {
    anchor: Option<GridPoint>,
    end: Option<GridPoint>,
}

impl SelectionRange {
    pub const fn anchor(self) -> Option<GridPoint> {
        self.anchor
    }

    pub const fn end(self) -> Option<GridPoint> {
        self.end
    }

    pub const fn has_anchor(self) -> bool {
        self.anchor.is_some()
    }

    pub fn begin(&mut self, point: GridPoint) -> bool {
        let next = Some(point);
        let changed = self.anchor != next || self.end != next;
        self.anchor = next;
        self.end = next;
        changed
    }

    pub fn update(&mut self, point: GridPoint) -> bool {
        if self.anchor.is_none() {
            return false;
        }
        let next = Some(point);
        if self.end == next {
            return false;
        }
        self.end = next;
        true
    }

    pub fn clear(&mut self) -> bool {
        if self.anchor.is_none() {
            return false;
        }
        self.anchor = None;
        self.end = None;
        true
    }

    pub fn ordered_flat_range(self, cols: usize) -> Option<(u32, u32)> {
        let anchor = self.anchor?;
        let end = self.end?;
        let cols = u32::try_from(cols).ok()?;
        if cols == 0 {
            return None;
        }
        let start = u32::from(anchor.row) * cols + u32::from(anchor.col);
        let finish = u32::from(end.row) * cols + u32::from(end.col);
        Some((start.min(finish), start.max(finish)))
    }

    pub fn is_single_cell(self) -> bool {
        matches!((self.anchor, self.end), (Some(anchor), Some(end)) if anchor == end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PointerState {
    cell: GridPoint,
    buttons_mask: u8,
}

impl PointerState {
    pub const fn cell(self) -> GridPoint {
        self.cell
    }

    pub const fn cell_col(self) -> u16 {
        self.cell.col
    }

    pub const fn cell_row(self) -> u16 {
        self.cell.row
    }

    pub const fn buttons_mask(self) -> u8 {
        self.buttons_mask
    }

    pub const fn primary_button_held(self) -> bool {
        self.buttons_mask & 1 != 0
    }

    pub fn set_button_state(&mut self, button_code: u8, is_press: bool) -> bool {
        let next = if is_press {
            self.buttons_mask | (1 << button_code)
        } else {
            self.buttons_mask & !(1 << button_code)
        };
        if self.buttons_mask == next {
            return false;
        }
        self.buttons_mask = next;
        true
    }

    pub fn update_cell_position(
        &mut self,
        position: PhysicalPosition<f64>,
        cell_width: usize,
        cell_height: usize,
        grid_cols: usize,
        grid_rows: usize,
    ) -> bool {
        let col = if cell_width == 0 {
            0
        } else {
            (position.x as usize / cell_width) as u16
        };
        let row = if cell_height == 0 {
            0
        } else {
            (position.y as usize / cell_height) as u16
        };

        let next = GridPoint {
            col: col.min(grid_cols.saturating_sub(1) as u16),
            row: row.min(grid_rows.saturating_sub(1) as u16),
        };
        if self.cell == next {
            return false;
        }
        self.cell = next;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InteractionState {
    palette_open: bool,
    viewport_offset: usize,
    pointer: PointerState,
    selection: SelectionRange,
}

impl InteractionState {
    pub const fn palette_open(self) -> bool {
        self.palette_open
    }

    pub fn set_palette_open(&mut self, open: bool) -> bool {
        if self.palette_open == open {
            return false;
        }
        self.palette_open = open;
        true
    }

    pub const fn viewport_offset(self) -> usize {
        self.viewport_offset
    }

    pub fn set_viewport_offset(&mut self, offset: usize) -> bool {
        if self.viewport_offset == offset {
            return false;
        }
        self.viewport_offset = offset;
        true
    }

    pub fn reset_viewport(&mut self) -> bool {
        self.set_viewport_offset(0)
    }

    pub const fn pointer(self) -> PointerState {
        self.pointer
    }

    pub const fn selection(self) -> SelectionRange {
        self.selection
    }

    pub fn update_pointer_cell(
        &mut self,
        position: PhysicalPosition<f64>,
        cell_width: usize,
        cell_height: usize,
        grid_cols: usize,
        grid_rows: usize,
    ) -> bool {
        self.pointer
            .update_cell_position(position, cell_width, cell_height, grid_cols, grid_rows)
    }

    pub fn set_pointer_button_state(&mut self, button_code: u8, is_press: bool) -> bool {
        self.pointer.set_button_state(button_code, is_press)
    }

    pub fn begin_selection_at_pointer(&mut self) -> bool {
        self.selection.begin(self.pointer.cell())
    }

    pub fn update_selection_to_pointer(&mut self) -> bool {
        self.selection.update(self.pointer.cell())
    }

    pub fn has_selection(&self) -> bool {
        self.selection.has_anchor()
    }

    pub fn selection_flat_range(&self, cols: usize) -> Option<(u32, u32)> {
        self.selection.ordered_flat_range(cols)
    }

    pub fn clear_selection(&mut self) -> bool {
        self.selection.clear()
    }

    pub fn scroll_viewport_by_lines(&mut self, lines: i32, max_offset: usize) -> bool {
        if lines == 0 {
            return false;
        }

        let next = if lines < 0 {
            (self.viewport_offset + (-lines) as usize).min(max_offset)
        } else {
            self.viewport_offset.saturating_sub(lines as usize)
        };
        self.set_viewport_offset(next)
    }

    pub fn scroll_viewport_page_up(&mut self, page_size: usize, max_offset: usize) -> bool {
        self.set_viewport_offset((self.viewport_offset + page_size).min(max_offset))
    }

    pub fn scroll_viewport_page_down(&mut self, page_size: usize) -> bool {
        self.set_viewport_offset(self.viewport_offset.saturating_sub(page_size))
    }
}

#[cfg(test)]
mod tests {
    use super::{GridPoint, InteractionState, PointerState, SelectionRange};
    use winit::dpi::PhysicalPosition;

    #[test]
    fn selection_begin_update_and_clear_round_trip() {
        let mut selection = SelectionRange::default();
        assert!(selection.begin(GridPoint::new(2, 3)));
        assert_eq!(selection.anchor(), Some(GridPoint::new(2, 3)));
        assert_eq!(selection.end(), Some(GridPoint::new(2, 3)));
        assert!(selection.update(GridPoint::new(4, 1)));
        assert_eq!(selection.ordered_flat_range(10), Some((23, 41)));
        assert!(selection.clear());
        assert_eq!(selection.ordered_flat_range(10), None);
    }

    #[test]
    fn selection_reports_single_cell_click() {
        let mut selection = SelectionRange::default();
        selection.begin(GridPoint::new(1, 1));
        assert!(selection.is_single_cell());
        selection.update(GridPoint::new(1, 2));
        assert!(!selection.is_single_cell());
    }

    #[test]
    fn pointer_state_tracks_buttons_and_clamps_cell_position() {
        let mut pointer = PointerState::default();
        assert!(pointer.set_button_state(0, true));
        assert!(pointer.primary_button_held());
        assert!(pointer.update_cell_position(PhysicalPosition::new(999.0, 999.0), 8, 16, 80, 24));
        assert_eq!(pointer.cell(), GridPoint::new(23, 79));
        assert!(pointer.set_button_state(0, false));
        assert!(!pointer.primary_button_held());
    }

    #[test]
    fn interaction_state_scrolls_viewport_in_both_directions() {
        let mut state = InteractionState::default();
        assert!(state.scroll_viewport_by_lines(-3, 10));
        assert_eq!(state.viewport_offset(), 3);
        assert!(state.scroll_viewport_by_lines(2, 10));
        assert_eq!(state.viewport_offset(), 1);
    }

    #[test]
    fn interaction_state_uses_pointer_for_selection() {
        let mut state = InteractionState::default();
        assert!(state.update_pointer_cell(PhysicalPosition::new(16.0, 32.0), 8, 16, 80, 24));
        assert!(state.begin_selection_at_pointer());
        assert!(state.update_pointer_cell(PhysicalPosition::new(40.0, 32.0), 8, 16, 80, 24));
        assert!(state.update_selection_to_pointer());
        assert_eq!(state.selection_flat_range(80), Some((162, 165)));
    }
}
