use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::{Cursor, Grid, TerminalState};
use tracing::warn;

pub const DEFAULT_SCROLLBACK_CAP: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRendererConfig {
    pub scrollback_cap: usize,
}

impl Default for CpuRendererConfig {
    fn default() -> Self {
        Self {
            scrollback_cap: DEFAULT_SCROLLBACK_CAP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuRenderRow {
    pub row: u16,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRenderFrameStats {
    pub rendered_rows: usize,
    pub rendered_cells: usize,
    pub rendered_bytes: usize,
    pub fallback_rows: usize,
    pub dropped_rows: usize,
    pub visible_scrollback_lines: usize,
    pub trimmed_scrollback_lines: usize,
    pub full_redraw: bool,
}

impl CpuRenderFrameStats {
    fn new(
        rows: &[CpuRenderRow],
        width: u16,
        visible_scrollback_lines: usize,
        trimmed_scrollback_lines: usize,
        full_redraw: bool,
        fallback_rows: usize,
        dropped_rows: usize,
    ) -> Self {
        let rendered_rows = rows.len();
        let expected_row_cells = width as usize;
        let (rendered_cells, rendered_bytes) =
            rows.iter().fold((0usize, 0usize), |(cells, bytes), row| {
                // Keep stats coupled to emitted payload so they remain internally consistent.
                let row_cells = row.text.chars().count();
                debug_assert_eq!(row_cells, expected_row_cells);
                (
                    cells.saturating_add(row_cells),
                    bytes.saturating_add(row.text.len()),
                )
            });

        Self {
            rendered_rows,
            rendered_cells,
            rendered_bytes,
            fallback_rows,
            dropped_rows,
            visible_scrollback_lines,
            trimmed_scrollback_lines,
            full_redraw,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuRenderFrame {
    pub mode: RenderMode,
    pub width: u16,
    pub height: u16,
    pub cursor: Cursor,
    pub rows: Vec<CpuRenderRow>,
    pub visible_scrollback_lines: usize,
    pub trimmed_scrollback_lines: usize,
    pub full_redraw: bool,
    pub stats: CpuRenderFrameStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuRenderer {
    config: CpuRendererConfig,
}

impl Default for CpuRenderer {
    fn default() -> Self {
        Self::new(CpuRendererConfig::default())
    }
}

impl CpuRenderer {
    pub const fn new(config: CpuRendererConfig) -> Self {
        Self { config }
    }

    pub const fn config(&self) -> CpuRendererConfig {
        self.config
    }

    /// Renders only dirty rows and clears the dirty state.
    pub fn render(&self, state: &mut TerminalState) -> CpuRenderFrame {
        self.render_delta(state)
    }

    /// Produces a deterministic full-frame snapshot without mutating dirty flags.
    pub fn render_full(&self, state: &TerminalState) -> CpuRenderFrame {
        let mut rows = Vec::with_capacity(state.grid.height() as usize);
        let mut fallback_rows = 0usize;
        for row in 0..state.grid.height() {
            let (text, used_fallback) = Self::safe_row_text(&state.grid, row);
            fallback_rows = fallback_rows.saturating_add(usize::from(used_fallback));
            rows.push(CpuRenderRow { row, text });
        }
        self.build_frame(state, rows, true, fallback_rows, 0)
    }

    /// Produces dirty-region updates for low-allocation steady-state rendering.
    pub fn render_delta(&self, state: &mut TerminalState) -> CpuRenderFrame {
        let mut dirty_rows = state.grid.take_dirty_rows();
        // Canonicalize source rows defensively to preserve deterministic output.
        dirty_rows.sort_unstable();
        dirty_rows.dedup();
        let mut rows = Vec::with_capacity(dirty_rows.len());
        let mut fallback_rows = 0usize;
        let mut dropped_rows = 0usize;

        for row in dirty_rows {
            if row >= state.grid.height() {
                dropped_rows = dropped_rows.saturating_add(1);
                warn!(
                    row,
                    height = state.grid.height(),
                    "cpu renderer dirty row out of bounds; dropping row deterministically"
                );
                continue;
            }
            let (text, used_fallback) = Self::safe_row_text(&state.grid, row);
            fallback_rows = fallback_rows.saturating_add(usize::from(used_fallback));
            rows.push(CpuRenderRow { row, text });
        }

        self.build_frame(state, rows, false, fallback_rows, dropped_rows)
    }

    fn build_frame(
        &self,
        state: &TerminalState,
        rows: Vec<CpuRenderRow>,
        full_redraw: bool,
        fallback_rows: usize,
        dropped_rows: usize,
    ) -> CpuRenderFrame {
        let width = state.grid.width();
        let height = state.grid.height();
        let visible_scrollback_lines = state.scrollback.len().min(self.config.scrollback_cap);
        let trimmed_scrollback_lines = state
            .scrollback
            .len()
            .saturating_sub(visible_scrollback_lines);
        let stats = CpuRenderFrameStats::new(
            &rows,
            width,
            visible_scrollback_lines,
            trimmed_scrollback_lines,
            full_redraw,
            fallback_rows,
            dropped_rows,
        );

        CpuRenderFrame {
            mode: RenderMode::Cpu,
            width,
            height,
            cursor: state.cursor,
            rows,
            visible_scrollback_lines,
            trimmed_scrollback_lines,
            full_redraw,
            stats,
        }
    }

    fn safe_row_text(grid: &Grid, row: u16) -> (String, bool) {
        match grid.row_string(row) {
            Ok(text) => (text, false),
            Err(err) => {
                warn!(
                    row,
                    width = grid.width(),
                    height = grid.height(),
                    %err,
                    "cpu renderer row fetch failed; emitting deterministic blank row"
                );
                (" ".repeat(grid.width() as usize), true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CpuRenderer, CpuRendererConfig, DEFAULT_SCROLLBACK_CAP};
    use rldyourterm_services::TerminalState;
    use rldyourterm_services::grid::Attrs;

    fn state_with_default_scrollback(width: u16, height: u16) -> TerminalState {
        TerminalState::new(width, height, DEFAULT_SCROLLBACK_CAP)
    }

    #[test]
    fn full_render_is_deterministic_and_cpu_mode() {
        let mut state = state_with_default_scrollback(4, 2);
        state
            .grid
            .put_char(0, 0, 'a', Attrs::default())
            .expect("put row 0 col 0");
        state
            .grid
            .put_char(0, 1, 'b', Attrs::default())
            .expect("put row 0 col 1");
        state
            .grid
            .put_char(1, 0, 'x', Attrs::default())
            .expect("put row 1 col 0");
        state.cursor.row = 1;
        state.cursor.col = 2;

        let renderer = CpuRenderer::default();
        let first = renderer.render_full(&state);
        let second = renderer.render_full(&state);

        assert_eq!(first, second);
        assert!(first.full_redraw);
        assert_eq!(first.rows.len(), 2);
        assert_eq!(first.rows[0].text, "ab  ");
        assert_eq!(first.rows[1].text, "x   ");
        assert_eq!(first.cursor.row, 1);
        assert_eq!(first.cursor.col, 2);
        assert_eq!(first.stats, second.stats);
        assert_eq!(first.stats.rendered_rows, 2);
        assert_eq!(first.stats.rendered_cells, 8);
        assert_eq!(first.stats.rendered_bytes, 8);
        assert_eq!(first.stats.fallback_rows, 0);
        assert_eq!(first.stats.dropped_rows, 0);
        assert_eq!(first.stats.visible_scrollback_lines, 0);
        assert_eq!(first.stats.trimmed_scrollback_lines, 0);
        assert!(first.stats.full_redraw);
    }

    #[test]
    fn delta_render_tracks_dirty_rows_in_stable_order() {
        let mut state = state_with_default_scrollback(3, 3);
        let renderer = CpuRenderer::default();

        let initial = renderer.render_delta(&mut state);
        assert_eq!(initial.rows.len(), 3);
        assert_eq!(initial.rows[0].row, 0);
        assert_eq!(initial.rows[1].row, 1);
        assert_eq!(initial.rows[2].row, 2);
        assert_eq!(initial.stats.rendered_rows, 3);
        assert_eq!(initial.stats.rendered_cells, 9);
        assert_eq!(initial.stats.rendered_bytes, 9);
        assert!(!initial.stats.full_redraw);

        let no_changes = renderer.render_delta(&mut state);
        assert!(no_changes.rows.is_empty());
        assert_eq!(no_changes.stats.rendered_rows, 0);
        assert_eq!(no_changes.stats.rendered_cells, 0);
        assert_eq!(no_changes.stats.rendered_bytes, 0);
        assert_eq!(no_changes.stats.fallback_rows, 0);
        assert_eq!(no_changes.stats.dropped_rows, 0);

        state
            .grid
            .put_char(1, 2, 'x', Attrs::default())
            .expect("dirty row update");
        let delta = renderer.render(&mut state);
        assert_eq!(delta.rows.len(), 1);
        assert_eq!(delta.rows[0].row, 1);
        assert_eq!(delta.rows[0].text, "  x");
        assert_eq!(delta.stats.rendered_rows, 1);
        assert_eq!(delta.stats.rendered_cells, 3);
        assert_eq!(delta.stats.rendered_bytes, 3);
        assert!(!delta.stats.full_redraw);
    }

    #[test]
    fn scrollback_visibility_is_bounded_by_renderer_cap() {
        let mut state = TerminalState::new(1, 1, 100_000);
        for idx in 0..7 {
            state.scrollback.push(format!("line-{idx}"));
        }

        let renderer = CpuRenderer::new(CpuRendererConfig { scrollback_cap: 5 });
        let frame = renderer.render_full(&state);

        assert_eq!(frame.visible_scrollback_lines, 5);
        assert_eq!(frame.trimmed_scrollback_lines, 2);
        assert_eq!(frame.stats.visible_scrollback_lines, 5);
        assert_eq!(frame.stats.trimmed_scrollback_lines, 2);
    }

    #[test]
    fn zero_sized_grid_is_rendered_without_panic() {
        let mut state = state_with_default_scrollback(0, 0);
        let renderer = CpuRenderer::default();

        let frame = renderer.render_delta(&mut state);
        assert_eq!(frame.width, 0);
        assert_eq!(frame.height, 0);
        assert!(frame.rows.is_empty());
        assert_eq!(frame.visible_scrollback_lines, 0);
        assert_eq!(frame.trimmed_scrollback_lines, 0);
        assert_eq!(frame.stats.rendered_rows, 0);
        assert_eq!(frame.stats.rendered_cells, 0);
        assert_eq!(frame.stats.rendered_bytes, 0);
        assert_eq!(frame.stats.fallback_rows, 0);
        assert_eq!(frame.stats.dropped_rows, 0);
    }

    #[test]
    fn stats_account_for_utf8_bytes_without_losing_cell_count() {
        let mut state = state_with_default_scrollback(2, 1);
        state
            .grid
            .put_char(0, 0, 'é', Attrs::default())
            .expect("put row 0 col 0");
        state
            .grid
            .put_char(0, 1, '🦀', Attrs::default())
            .expect("put row 0 col 1");

        let renderer = CpuRenderer::default();
        let frame = renderer.render_full(&state);

        assert_eq!(frame.rows[0].text, "é🦀");
        assert_eq!(frame.stats.rendered_rows, 1);
        assert_eq!(frame.stats.rendered_cells, 2);
        assert_eq!(frame.stats.rendered_bytes, "é🦀".len());
        assert_eq!(frame.stats.fallback_rows, 0);
        assert_eq!(frame.stats.dropped_rows, 0);
    }

    #[test]
    fn invalid_row_fallback_is_deterministic_and_bounded() {
        let state = state_with_default_scrollback(4, 1);

        let first = CpuRenderer::safe_row_text(&state.grid, 7);
        let second = CpuRenderer::safe_row_text(&state.grid, 7);

        assert_eq!(first, second);
        assert_eq!(first.0, "    ");
        assert!(first.1);
    }

    #[test]
    fn full_render_does_not_consume_dirty_rows() {
        let mut state = state_with_default_scrollback(2, 2);
        state
            .grid
            .put_char(0, 0, 'a', Attrs::default())
            .expect("put row 0 col 0");
        state
            .grid
            .put_char(1, 1, 'z', Attrs::default())
            .expect("put row 1 col 1");
        let renderer = CpuRenderer::default();

        let full_first = renderer.render_full(&state);
        let full_second = renderer.render_full(&state);
        assert_eq!(full_first.rows, full_second.rows);
        assert!(full_first.full_redraw);
        assert!(full_second.full_redraw);

        let delta_after_full = renderer.render_delta(&mut state);
        assert_eq!(delta_after_full.rows.len(), 2);
        assert_eq!(delta_after_full.rows[0].row, 0);
        assert_eq!(delta_after_full.rows[1].row, 1);
        assert_eq!(delta_after_full.stats.rendered_rows, 2);
        assert_eq!(delta_after_full.stats.rendered_cells, 4);
        assert_eq!(delta_after_full.stats.rendered_bytes, 4);
        assert!(!delta_after_full.stats.full_redraw);

        let settled = renderer.render_delta(&mut state);
        assert!(settled.rows.is_empty());
        assert_eq!(settled.stats.rendered_rows, 0);
        assert_eq!(settled.stats.rendered_cells, 0);
        assert_eq!(settled.stats.rendered_bytes, 0);
    }

    #[test]
    fn delta_render_coalesces_repeated_writes_per_dirty_row() {
        let mut state = state_with_default_scrollback(4, 3);
        let renderer = CpuRenderer::default();
        let _ = renderer.render_delta(&mut state);

        state
            .grid
            .put_char(2, 1, 'z', Attrs::default())
            .expect("put row 2 col 1");
        state
            .grid
            .put_char(0, 0, 'a', Attrs::default())
            .expect("put row 0 col 0");
        state
            .grid
            .put_char(2, 2, 'x', Attrs::default())
            .expect("put row 2 col 2");
        state
            .grid
            .put_char(1, 3, 'q', Attrs::default())
            .expect("put row 1 col 3");

        let frame = renderer.render_delta(&mut state);
        assert_eq!(frame.rows.len(), 3);
        assert_eq!(frame.rows[0].row, 0);
        assert_eq!(frame.rows[1].row, 1);
        assert_eq!(frame.rows[2].row, 2);
        assert_eq!(frame.rows[0].text, "a   ");
        assert_eq!(frame.rows[1].text, "   q");
        assert_eq!(frame.rows[2].text, " zx ");
        assert_eq!(frame.stats.rendered_rows, 3);
        assert_eq!(frame.stats.rendered_cells, 12);
        assert_eq!(frame.stats.rendered_bytes, 12);
        assert_eq!(frame.stats.fallback_rows, 0);
        assert_eq!(frame.stats.dropped_rows, 0);
        assert!(!frame.stats.full_redraw);
    }

    #[test]
    fn delta_stats_track_utf8_bytes_and_cells_from_payload() {
        let mut state = state_with_default_scrollback(3, 2);
        let renderer = CpuRenderer::default();
        let _ = renderer.render_delta(&mut state);

        state
            .grid
            .put_char(0, 0, 'é', Attrs::default())
            .expect("put row 0 col 0");
        state
            .grid
            .put_char(0, 1, '🦀', Attrs::default())
            .expect("put row 0 col 1");
        state
            .grid
            .put_char(0, 2, 'a', Attrs::default())
            .expect("put row 0 col 2");
        state
            .grid
            .put_char(1, 0, 'ß', Attrs::default())
            .expect("put row 1 col 0");

        let frame = renderer.render_delta(&mut state);
        assert_eq!(frame.rows.len(), 2);
        assert_eq!(frame.rows[0].text, "é🦀a");
        assert_eq!(frame.rows[1].text, "ß  ");

        let expected_cells = frame.rows.iter().fold(0usize, |total, row| {
            total.saturating_add(row.text.chars().count())
        });
        let expected_bytes = frame
            .rows
            .iter()
            .fold(0usize, |total, row| total.saturating_add(row.text.len()));
        assert_eq!(expected_cells, 6);
        assert_eq!(expected_bytes, "é🦀a".len() + "ß  ".len());
        assert_eq!(frame.stats.rendered_rows, 2);
        assert_eq!(frame.stats.rendered_cells, expected_cells);
        assert_eq!(frame.stats.rendered_bytes, expected_bytes);
    }
}
