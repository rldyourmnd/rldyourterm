use rldyourterm_font::{GlyphBitmap, GlyphCache};
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Cursor, DEFAULT_SCROLLBACK_CAP, Grid, TerminalState,
    color_to_u32,
};
use tracing::warn;

pub const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
pub const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
pub const DEFAULT_BG_U32: u32 = rgb_to_u32(DEFAULT_BG.0, DEFAULT_BG.1, DEFAULT_BG.2);
pub const DEFAULT_FG_U32: u32 = rgb_to_u32(DEFAULT_FG.0, DEFAULT_FG.1, DEFAULT_FG.2);

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

pub fn render_terminal_buffer(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    terminal: &mut TerminalState,
    glyph_cache: &mut GlyphCache,
    prev_cursor_row: Option<u16>,
    dirty_rows_scratch: &mut Vec<u16>,
) {
    if width == 0 || height == 0 {
        return;
    }

    let grid_rows = terminal.grid.height() as usize;
    let grid_cols = terminal.grid.width() as usize;
    let visible_rows = (height / CELL_HEIGHT).max(1).min(grid_rows);
    let visible_cols = (width / CELL_WIDTH).max(1).min(grid_cols);

    let dirty_flags = terminal.grid.dirty_rows();
    let cursor_row = terminal.cursor.row;

    let mut dirty = std::mem::take(dirty_rows_scratch);
    dirty.clear();
    if dirty.capacity() < (visible_rows / 4 + 2) {
        dirty.reserve((visible_rows / 4 + 2) - dirty.capacity());
    }
    for row in 0..visible_rows {
        let row_u16 = row as u16;
        if dirty_flags.get(row).copied().unwrap_or(false)
            || row_u16 == cursor_row
            || prev_cursor_row == Some(row_u16)
        {
            dirty.push(row_u16);
        }
    }
    terminal.grid.clear_dirty_rows();

    if dirty.is_empty() {
        *dirty_rows_scratch = dirty;
        return;
    }

    for &row in &dirty {
        let row_idx = row as usize;
        if row_idx >= visible_rows {
            continue;
        }
        let base_y = row_idx * CELL_HEIGHT;
        let clear_end_y = (base_y + CELL_HEIGHT).min(height);

        for py in base_y..clear_end_y {
            let start = py * width;
            buffer[start..start + width].fill(DEFAULT_BG_U32);
        }

        if let Ok(cells) = terminal.grid.row_cells(row) {
            for (col, cell) in cells.iter().take(visible_cols).enumerate() {
                let x = col * CELL_WIDTH;
                let (fg, bg) = resolve_cell_colors(&cell.attrs);

                if bg != DEFAULT_BG_U32 {
                    draw_cell_bg(buffer, width, height, x, base_y, bg);
                }

                if cell.ch != ' ' {
                    let glyph = glyph_cache.get(cell.ch);
                    draw_glyph_blended(
                        buffer,
                        width,
                        height,
                        x,
                        base_y,
                        glyph,
                        fg,
                        cell.attrs.bold,
                    );
                }

                if cell.attrs.underline {
                    draw_underline(buffer, width, height, x, base_y, fg);
                }

                if cell.attrs.strikethrough {
                    draw_strikethrough(buffer, width, height, x, base_y, fg);
                }
            }
        }
    }

    let grid_pixel_height = visible_rows * CELL_HEIGHT;
    if grid_pixel_height < height {
        let any_bottom_dirty = dirty.iter().any(|&row| (row as usize) + 1 >= visible_rows);
        if any_bottom_dirty {
            for py in grid_pixel_height..height {
                let start = py * width;
                buffer[start..start + width].fill(DEFAULT_BG_U32);
            }
        }
    }

    if terminal.cursor.visible {
        let cursor_row = terminal.cursor.row as usize;
        let cursor_col = terminal.cursor.col as usize;
        if cursor_row < visible_rows && cursor_col < visible_cols {
            draw_cursor(
                buffer,
                width,
                height,
                cursor_col * CELL_WIDTH,
                cursor_row * CELL_HEIGHT,
            );
        }
    }

    *dirty_rows_scratch = dirty;
}

pub fn resolve_cell_colors(attrs: &Attrs) -> (u32, u32) {
    let mut fg = color_to_u32(attrs.fg, DEFAULT_FG);
    let mut bg = color_to_u32(attrs.bg, DEFAULT_BG);

    if attrs.dim {
        let (r, g, b) = u32_to_rgb(fg);
        fg = rgb_to_u32(r / 2, g / 2, b / 2);
    }

    if attrs.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }

    (fg, bg)
}

pub const fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn u32_to_rgb(color: u32) -> (u8, u8, u8) {
    ((color >> 16) as u8, (color >> 8) as u8, color as u8)
}

fn draw_cell_bg(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, bg: u32) {
    for py in y..(y + CELL_HEIGHT).min(height) {
        let row_start = py * width;
        for px in x..(x + CELL_WIDTH).min(width) {
            buffer[row_start + px] = bg;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_blended(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    cell_x: usize,
    cell_y: usize,
    glyph: &GlyphBitmap,
    fg: u32,
    bold: bool,
) {
    if glyph.glyph_width == 0 || glyph.glyph_height == 0 {
        return;
    }

    let (fg_r, fg_g, fg_b) = u32_to_rgb(fg);

    for gy in 0..glyph.glyph_height {
        for gx in 0..glyph.glyph_width {
            let coverage = glyph.data[gy * glyph.glyph_width + gx];
            if coverage == 0 {
                continue;
            }

            let px = cell_x as i32 + glyph.x_offset + gx as i32;
            let py = cell_y as i32 + glyph.y_offset + gy as i32;
            if px < 0 || py < 0 {
                continue;
            }
            let px = px as usize;
            let py = py as usize;
            if px >= width || py >= height {
                continue;
            }

            let idx = py * width + px;
            let (bg_r, bg_g, bg_b) = u32_to_rgb(buffer[idx]);

            let a = coverage as u32;
            let inv_a = 255 - a;
            let r = (bg_r as u32 * inv_a + fg_r as u32 * a) / 255;
            let g = (bg_g as u32 * inv_a + fg_g as u32 * a) / 255;
            let b = (bg_b as u32 * inv_a + fg_b as u32 * a) / 255;
            buffer[idx] = rgb_to_u32(r as u8, g as u8, b as u8);

            if bold && px + 1 < width {
                let bold_idx = py * width + px + 1;
                let (bbg_r, bbg_g, bbg_b) = u32_to_rgb(buffer[bold_idx]);
                let br = (bbg_r as u32 * inv_a + fg_r as u32 * a) / 255;
                let bg_val = (bbg_g as u32 * inv_a + fg_g as u32 * a) / 255;
                let bb = (bbg_b as u32 * inv_a + fg_b as u32 * a) / 255;
                buffer[bold_idx] = rgb_to_u32(br as u8, bg_val as u8, bb as u8);
            }
        }
    }
}

fn draw_underline(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, fg: u32) {
    let line_y = y + CELL_HEIGHT - 1;
    if line_y >= height {
        return;
    }
    let row_start = line_y * width;
    for px in x..(x + CELL_WIDTH).min(width) {
        buffer[row_start + px] = fg;
    }
}

fn draw_strikethrough(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    fg: u32,
) {
    let line_y = y + CELL_HEIGHT / 2;
    if line_y >= height {
        return;
    }
    let row_start = line_y * width;
    for px in x..(x + CELL_WIDTH).min(width) {
        buffer[row_start + px] = fg;
    }
}

fn draw_cursor(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize) {
    for glyph_y in 0..CELL_HEIGHT {
        let pixel_y = y + glyph_y;
        if pixel_y >= height {
            break;
        }
        for glyph_x in 0..CELL_WIDTH {
            let pixel_x = x + glyph_x;
            if pixel_x >= width {
                break;
            }
            let index = pixel_y * width + pixel_x;
            buffer[index] ^= 0x00FF_FFFF;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CpuRenderer, CpuRendererConfig, DEFAULT_BG_U32, DEFAULT_SCROLLBACK_CAP,
        render_terminal_buffer,
    };
    use rldyourterm_font::GlyphCache;
    use rldyourterm_services::terminal::TerminalState;
    use rldyourterm_services::terminal::{Attrs, CELL_HEIGHT, CELL_WIDTH};

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
    fn pixel_renderer_draws_dirty_row_and_clears_dirty_flags() {
        let mut state = state_with_default_scrollback(2, 1);
        state
            .grid
            .put_char(0, 0, 'A', Attrs::default())
            .expect("put char");

        let width = CELL_WIDTH * 2;
        let height = CELL_HEIGHT;
        let mut buffer = vec![0; width * height];
        let mut glyph_cache = GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16);
        let mut dirty_rows = Vec::new();

        render_terminal_buffer(
            &mut buffer,
            width,
            height,
            &mut state,
            &mut glyph_cache,
            None,
            &mut dirty_rows,
        );

        assert!(
            buffer.iter().any(|pixel| *pixel != DEFAULT_BG_U32),
            "glyph rendering should modify at least one pixel"
        );
        assert!(
            state.grid.dirty_rows().iter().all(|dirty| !*dirty),
            "pixel renderer must clear dirty row flags after rendering"
        );
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
