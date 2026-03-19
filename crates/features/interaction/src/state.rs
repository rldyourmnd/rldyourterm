// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_core::SearchMatch;
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchState {
    active: bool,
    query: String,
    preedit: Option<String>,
    matches: Vec<SearchMatch>,
    active_match: Option<usize>,
    error: Option<String>,
}

impl SearchState {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn preedit(&self) -> Option<&str> {
        self.preedit.as_deref()
    }

    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    pub fn active_match_index(&self) -> Option<usize> {
        self.active_match
    }

    pub fn active_match(&self) -> Option<&SearchMatch> {
        self.active_match.and_then(|index| self.matches.get(index))
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn enter(&mut self) -> bool {
        let changed = !self.active
            || !self.query.is_empty()
            || self.preedit.is_some()
            || !self.matches.is_empty()
            || self.active_match.is_some()
            || self.error.is_some();
        self.active = true;
        self.clear_runtime_state();
        changed
    }

    pub fn exit(&mut self) -> bool {
        let changed = self.active
            || !self.query.is_empty()
            || self.preedit.is_some()
            || !self.matches.is_empty()
            || self.active_match.is_some()
            || self.error.is_some();
        self.active = false;
        self.clear_runtime_state();
        changed
    }

    pub fn push_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.preedit = None;
        self.query.push_str(text);
        true
    }

    pub fn pop_char(&mut self) -> bool {
        let Some((index, _)) = self.query.char_indices().next_back() else {
            return false;
        };
        self.query.truncate(index);
        true
    }

    pub fn set_preedit(&mut self, text: Option<&str>) -> bool {
        let next = text.filter(|text| !text.is_empty()).map(str::to_owned);
        if self.preedit == next {
            return false;
        }
        self.preedit = next;
        true
    }

    pub fn set_matches(
        &mut self,
        matches: Vec<SearchMatch>,
        error: Option<String>,
        reset_active: bool,
    ) -> bool {
        let next_active = if error.is_some() || matches.is_empty() {
            None
        } else if reset_active {
            Some(0)
        } else {
            Some(self.active_match.unwrap_or(0).min(matches.len() - 1))
        };
        if self.matches == matches && self.error == error && self.active_match == next_active {
            return false;
        }
        self.matches = matches;
        self.error = error;
        self.active_match = next_active;
        true
    }

    pub fn advance_match(&mut self) -> bool {
        if self.matches.len() <= 1 {
            return false;
        }
        let next = (self.active_match.unwrap_or(0) + 1) % self.matches.len();
        if self.active_match == Some(next) {
            return false;
        }
        self.active_match = Some(next);
        true
    }

    pub fn retreat_match(&mut self) -> bool {
        if self.matches.len() <= 1 {
            return false;
        }
        let current = self.active_match.unwrap_or(0);
        let next = if current == 0 {
            self.matches.len() - 1
        } else {
            current - 1
        };
        if self.active_match == Some(next) {
            return false;
        }
        self.active_match = Some(next);
        true
    }

    fn clear_runtime_state(&mut self) {
        self.query.clear();
        self.preedit = None;
        self.matches.clear();
        self.active_match = None;
        self.error = None;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InteractionState {
    palette_open: bool,
    viewport_offset: usize,
    pointer: PointerState,
    selection: SelectionRange,
    search: SearchState,
}

impl InteractionState {
    pub const fn palette_open(&self) -> bool {
        self.palette_open
    }

    pub fn set_palette_open(&mut self, open: bool) -> bool {
        if self.palette_open == open {
            return false;
        }
        self.palette_open = open;
        true
    }

    pub const fn viewport_offset(&self) -> usize {
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

    pub const fn pointer(&self) -> PointerState {
        self.pointer
    }

    pub const fn selection(&self) -> SelectionRange {
        self.selection
    }

    pub fn search(&self) -> &SearchState {
        &self.search
    }

    pub fn search_active(&self) -> bool {
        self.search.is_active()
    }

    pub fn enter_search_mode(&mut self) -> bool {
        self.search.enter()
    }

    pub fn exit_search_mode(&mut self) -> bool {
        self.search.exit()
    }

    pub fn search_query(&self) -> &str {
        self.search.query()
    }

    pub fn set_search_preedit(&mut self, text: Option<&str>) -> bool {
        self.search.set_preedit(text)
    }

    pub fn append_search_text(&mut self, text: &str) -> bool {
        self.search.push_text(text)
    }

    pub fn pop_search_text(&mut self) -> bool {
        self.search.pop_char()
    }

    pub fn set_search_results(
        &mut self,
        matches: Vec<SearchMatch>,
        error: Option<String>,
        reset_active: bool,
    ) -> bool {
        self.search.set_matches(matches, error, reset_active)
    }

    pub fn advance_search_match(&mut self) -> bool {
        self.search.advance_match()
    }

    pub fn retreat_search_match(&mut self) -> bool {
        self.search.retreat_match()
    }

    pub fn align_viewport_to_active_search_match(&mut self, scrollback_len: usize) -> bool {
        let Some(search_match) = self.search.active_match() else {
            return false;
        };
        let next = if search_match.line_index < scrollback_len {
            scrollback_len.saturating_sub(search_match.line_index)
        } else {
            0
        };
        self.set_viewport_offset(next)
    }

    pub fn search_flat_range(
        &self,
        cols: usize,
        scrollback_len: usize,
        display_rows: usize,
    ) -> Option<(u32, u32)> {
        let search_match = self.search.active_match()?;
        let top_line_index = scrollback_len.saturating_sub(self.viewport_offset);
        project_search_match_flat_range(search_match, cols, top_line_index, display_rows)
    }

    pub fn collect_visible_search_flat_ranges(
        &self,
        cols: usize,
        scrollback_len: usize,
        display_rows: usize,
        include_active_match: bool,
        out: &mut Vec<(u32, u32)>,
    ) {
        out.clear();
        if cols == 0 || display_rows == 0 {
            return;
        }

        let top_line_index = scrollback_len.saturating_sub(self.viewport_offset);
        for (index, search_match) in self.search.matches().iter().enumerate() {
            if !include_active_match && Some(index) == self.search.active_match_index() {
                continue;
            }
            if let Some(range) =
                project_search_match_flat_range(search_match, cols, top_line_index, display_rows)
            {
                out.push(range);
            }
        }
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

fn project_search_match_flat_range(
    search_match: &SearchMatch,
    cols: usize,
    top_line_index: usize,
    display_rows: usize,
) -> Option<(u32, u32)> {
    if cols == 0
        || display_rows == 0
        || search_match.start_col >= search_match.end_col
        || search_match.start_col >= cols
    {
        return None;
    }

    let display_row = search_match.line_index.checked_sub(top_line_index)?;
    if display_row >= display_rows {
        return None;
    }

    let row_offset = display_row.checked_mul(cols)?;
    let end_col = search_match
        .end_col
        .saturating_sub(1)
        .min(cols.saturating_sub(1));
    let start = u32::try_from(row_offset.checked_add(search_match.start_col)?).ok()?;
    let end = u32::try_from(row_offset.checked_add(end_col)?).ok()?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::{GridPoint, InteractionState, PointerState, SearchState, SelectionRange};
    use rldyourterm_core::SearchMatch;
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

    #[test]
    fn search_state_tracks_query_preedit_matches_and_navigation() {
        let mut search = SearchState::default();
        assert!(search.enter());
        assert!(search.is_active());
        assert!(search.push_text("foo"));
        assert_eq!(search.query(), "foo");
        assert!(search.set_preedit(Some("bar")));
        assert_eq!(search.preedit(), Some("bar"));
        assert!(search.set_matches(
            vec![
                SearchMatch {
                    line_index: 5,
                    start_col: 2,
                    end_col: 5,
                    text: "foo".to_owned(),
                },
                SearchMatch {
                    line_index: 9,
                    start_col: 1,
                    end_col: 4,
                    text: "foo".to_owned(),
                },
            ],
            None,
            true,
        ));
        assert_eq!(search.active_match_index(), Some(0));
        assert!(search.advance_match());
        assert_eq!(search.active_match_index(), Some(1));
        assert!(search.retreat_match());
        assert_eq!(search.active_match_index(), Some(0));
        assert!(search.pop_char());
        assert_eq!(search.query(), "fo");
        assert!(search.exit());
        assert!(!search.is_active());
        assert!(search.query().is_empty());
        assert!(search.matches().is_empty());
    }

    #[test]
    fn interaction_state_maps_active_search_match_into_viewport_range() {
        let mut state = InteractionState::default();
        assert!(state.enter_search_mode());
        assert!(state.set_search_results(
            vec![SearchMatch {
                line_index: 95,
                start_col: 3,
                end_col: 6,
                text: "foo".to_owned(),
            }],
            None,
            true,
        ));
        assert!(state.align_viewport_to_active_search_match(100));
        assert_eq!(state.viewport_offset(), 5);
        assert_eq!(state.search_flat_range(80, 100, 24), Some((3, 5)));
    }

    #[test]
    fn interaction_state_collects_visible_search_ranges_excluding_active_match() {
        let mut state = InteractionState::default();
        assert!(state.enter_search_mode());
        assert!(state.set_search_results(
            vec![
                SearchMatch {
                    line_index: 95,
                    start_col: 3,
                    end_col: 6,
                    text: "foo".to_owned(),
                },
                SearchMatch {
                    line_index: 96,
                    start_col: 10,
                    end_col: 12,
                    text: "bar".to_owned(),
                },
                SearchMatch {
                    line_index: 99,
                    start_col: 1,
                    end_col: 3,
                    text: "baz".to_owned(),
                },
            ],
            None,
            true,
        ));
        assert!(state.align_viewport_to_active_search_match(100));

        let mut ranges = Vec::new();
        state.collect_visible_search_flat_ranges(80, 100, 4, false, &mut ranges);

        assert_eq!(ranges, vec![(90, 91)]);
    }
}
