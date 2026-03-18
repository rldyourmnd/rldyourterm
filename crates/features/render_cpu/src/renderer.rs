// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::terminal::{Cursor, DEFAULT_SCROLLBACK_CAP, Grid, TerminalState};
use tracing::warn;

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

    pub(crate) fn safe_row_text(grid: &Grid, row: u16) -> (String, bool) {
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
