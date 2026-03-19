// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

//! Stress and correctness tests for the CPU rasterization pipeline.
//!
//! These tests exercise real rendering paths (no mocks) at the pixel level
//! through `render_terminal_buffer`, verifying framebuffer integrity, SGR
//! attribute rendering, wide character layout, scrollback compositing,
//! selection highlighting, cursor overlays, and sustained-load stability.

use super::{DEFAULT_BG_U32, DEFAULT_FG_U32, render_terminal_buffer, rgb_to_u32};
use rldyourterm_font::GlyphCache;
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Cell, Color, DEFAULT_SCROLLBACK_CAP, TerminalState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn state_with_default_scrollback(width: u16, height: u16) -> TerminalState {
    TerminalState::new(width, height, DEFAULT_SCROLLBACK_CAP)
}

fn cells_from_str(s: &str) -> Vec<Cell> {
    s.chars()
        .map(|ch| Cell {
            ch,
            attrs: Attrs::default(),
            width: 1,
        })
        .collect()
}

/// Pixel value used to detect untouched regions of the framebuffer.
const SENTINEL: u32 = 0x00DE_ADBE;

/// Convenience wrapper that bundles the scratch vectors required by
/// `render_terminal_buffer` alongside the pixel buffer and glyph cache.
struct RenderCtx {
    buffer: Vec<u32>,
    width: usize,
    height: usize,
    glyph_cache: GlyphCache,
    current_damage: Vec<u16>,
    repaint: Vec<u16>,
    persisted_damage: Vec<u16>,
}

impl RenderCtx {
    fn new(cols: usize, rows: usize) -> Self {
        let width = cols * CELL_WIDTH;
        let height = rows * CELL_HEIGHT;
        Self {
            buffer: vec![DEFAULT_BG_U32; width * height],
            width,
            height,
            glyph_cache: GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16),
            current_damage: Vec::new(),
            repaint: Vec::new(),
            persisted_damage: Vec::new(),
        }
    }

    /// Render a full frame (framebuffer_age = 0) with no selection and no
    /// previous cursor row, blink visible.
    fn render_full(&mut self, state: &mut TerminalState) {
        render_terminal_buffer(
            &mut self.buffer,
            self.width,
            self.height,
            state,
            &mut self.glyph_cache,
            0,
            &[],
            None,
            &mut self.current_damage,
            &mut self.repaint,
            &mut self.persisted_damage,
            true,
            0,
            u32::MAX,
            u32::MAX,
            &[],
            &[],
        );
    }

    /// Render a delta frame (framebuffer_age = 1) with no selection and no
    /// previous cursor row, blink visible.
    fn render_delta(&mut self, state: &mut TerminalState) {
        let prev = self.persisted_damage.clone();
        render_terminal_buffer(
            &mut self.buffer,
            self.width,
            self.height,
            state,
            &mut self.glyph_cache,
            1,
            &prev,
            None,
            &mut self.current_damage,
            &mut self.repaint,
            &mut self.persisted_damage,
            true,
            0,
            u32::MAX,
            u32::MAX,
            &[],
            &[],
        );
    }

    /// Render with explicit selection range.
    fn render_with_selection(
        &mut self,
        state: &mut TerminalState,
        selection_start: u32,
        selection_end: u32,
    ) {
        render_terminal_buffer(
            &mut self.buffer,
            self.width,
            self.height,
            state,
            &mut self.glyph_cache,
            0,
            &[],
            None,
            &mut self.current_damage,
            &mut self.repaint,
            &mut self.persisted_damage,
            true,
            0,
            selection_start,
            selection_end,
            &[],
            &[],
        );
    }

    /// Render with explicit viewport offset (for scrollback visibility).
    fn render_with_viewport(&mut self, state: &mut TerminalState, viewport_offset: usize) {
        render_terminal_buffer(
            &mut self.buffer,
            self.width,
            self.height,
            state,
            &mut self.glyph_cache,
            0,
            &[],
            None,
            &mut self.current_damage,
            &mut self.repaint,
            &mut self.persisted_damage,
            true,
            viewport_offset,
            u32::MAX,
            u32::MAX,
            &[],
            &[],
        );
    }

    /// Return the pixel value at framebuffer coordinates (x, y).
    fn pixel_at(&self, x: usize, y: usize) -> u32 {
        self.buffer[y * self.width + x]
    }

    /// Extract all pixel values inside the cell at grid position (row, col).
    fn cell_pixels(&self, row: usize, col: usize) -> Vec<u32> {
        let base_x = col * CELL_WIDTH;
        let base_y = row * CELL_HEIGHT;
        let mut pixels = Vec::with_capacity(CELL_WIDTH * CELL_HEIGHT);
        for py in base_y..base_y + CELL_HEIGHT {
            for px in base_x..base_x + CELL_WIDTH {
                pixels.push(self.buffer[py * self.width + px]);
            }
        }
        pixels
    }

    /// Return the slice of pixel values for one pixel-row within the grid.
    fn pixel_row(&self, row_idx: usize) -> &[u32] {
        let start = row_idx * CELL_HEIGHT * self.width;
        let end = start + CELL_HEIGHT * self.width;
        &self.buffer[start..end]
    }

    /// Fill the entire buffer with a sentinel value.
    fn fill_sentinel(&mut self) {
        self.buffer.fill(SENTINEL);
    }
}

// ===========================================================================
// Framebuffer Correctness
// ===========================================================================

#[test]
fn full_frame_render_fills_all_pixels() {
    let cols = 10;
    let rows = 5;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state
        .grid
        .put_char(0, 0, 'A', Attrs::default())
        .expect("put char");
    state
        .grid
        .put_char(2, 5, 'Z', Attrs::default())
        .expect("put char");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.fill_sentinel();
    ctx.render_full(&mut state);

    // After a full render (age=0), every pixel must have been written.
    // No sentinel values should remain within the visible grid area.
    let visible_pixel_count = ctx.width * (rows * CELL_HEIGHT).min(ctx.height);
    let sentinel_count = ctx.buffer[..visible_pixel_count]
        .iter()
        .filter(|&&p| p == SENTINEL)
        .count();
    assert_eq!(
        sentinel_count, 0,
        "full frame render must overwrite every pixel in the visible area"
    );
}

#[test]
fn delta_render_only_touches_dirty_rows() {
    let cols: usize = 4;
    let rows: usize = 3;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;
    // Move cursor to row 1 so that the implicit cursor-row redraw does not
    // pollute row 0 or row 2. The rasterizer always considers the cursor row
    // dirty regardless of cursor visibility.
    state.cursor.row = 1;

    let mut ctx = RenderCtx::new(cols, rows);
    // First pass: full render to establish baseline and clear dirty flags.
    ctx.render_full(&mut state);

    // Mark only row 1 dirty by writing a character.
    state
        .grid
        .put_char(1, 0, 'X', Attrs::default())
        .expect("put char");

    // Refill buffer with sentinel so we can detect which rows are redrawn.
    ctx.fill_sentinel();
    ctx.render_delta(&mut state);

    // Row 0 should remain untouched (sentinel) because it was not dirty
    // and the cursor is not on row 0.
    let row0 = ctx.pixel_row(0);
    assert!(
        row0.iter().all(|&p| p == SENTINEL),
        "delta render must not touch clean row 0"
    );

    // Row 1 was dirty (explicit put_char + cursor row) and must be repainted.
    let row1 = ctx.pixel_row(1);
    assert!(
        row1.iter().all(|&p| p != SENTINEL),
        "delta render must repaint dirty row 1"
    );

    // Row 2 should remain untouched.
    let row2 = ctx.pixel_row(2);
    assert!(
        row2.iter().all(|&p| p == SENTINEL),
        "delta render must not touch clean row 2"
    );
}

#[test]
fn cursor_renders_at_correct_position() {
    let cols: usize = 4;
    let rows: usize = 2;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.row = 1;
    state.cursor.col = 2;
    state.cursor.visible = true;

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // The cursor uses XOR (^= 0x00FF_FFFF) on DEFAULT_BG_U32. After XOR the
    // pixel value differs from DEFAULT_BG_U32.
    let cursor_cell = ctx.cell_pixels(1, 2);
    let xor_pixels = cursor_cell.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    assert!(
        xor_pixels > 0,
        "cursor cell at (1,2) must contain XOR-inverted pixels"
    );

    // A non-cursor blank cell should be entirely DEFAULT_BG_U32.
    let blank_cell = ctx.cell_pixels(0, 0);
    assert!(
        blank_cell.iter().all(|&p| p == DEFAULT_BG_U32),
        "blank cell at (0,0) must be uniform default background"
    );
}

#[test]
fn cursor_hidden_produces_no_cursor_pixels() {
    let cols: usize = 3;
    let rows: usize = 2;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.row = 0;
    state.cursor.col = 0;
    state.cursor.visible = false;

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // With no content and cursor hidden, every pixel should be DEFAULT_BG_U32.
    assert!(
        ctx.buffer.iter().all(|&p| p == DEFAULT_BG_U32),
        "hidden cursor on blank grid must produce all-default-bg pixels"
    );
}

// ===========================================================================
// SGR Attribute Rendering
// ===========================================================================

#[test]
fn bold_text_produces_wider_glyph_coverage() {
    let cols: usize = 4;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Place normal 'H' at col 0.
    state
        .grid
        .put_char(0, 0, 'H', Attrs::default())
        .expect("put normal H");
    // Place bold 'H' at col 2.
    let bold_attrs = Attrs::default().with_bold();
    state
        .grid
        .put_char(0, 2, 'H', bold_attrs)
        .expect("put bold H");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    let normal_pixels = ctx.cell_pixels(0, 0);
    let bold_pixels = ctx.cell_pixels(0, 2);

    let normal_non_bg = normal_pixels
        .iter()
        .filter(|&&p| p != DEFAULT_BG_U32)
        .count();
    let bold_non_bg = bold_pixels.iter().filter(|&&p| p != DEFAULT_BG_U32).count();

    assert!(
        normal_non_bg > 0,
        "normal glyph must produce non-background pixels"
    );
    // Bold rendering doubles each coverage pixel rightward, so it should
    // produce at least as many (typically more) colored pixels.
    assert!(
        bold_non_bg >= normal_non_bg,
        "bold glyph ({bold_non_bg} non-bg pixels) must cover at least as many pixels as \
         normal glyph ({normal_non_bg} non-bg pixels)"
    );
}

#[test]
fn inverse_attr_paints_cell_bg_with_fg_color() {
    let cols: usize = 2;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let inverse_attrs = Attrs::default().with_inverse();
    // A space with inverse: bg becomes DEFAULT_FG color, fg becomes DEFAULT_BG color.
    // Since the char is space, only the background fill matters.
    state
        .grid
        .put_char(0, 0, ' ', inverse_attrs)
        .expect("put inverse space");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    let cell = ctx.cell_pixels(0, 0);
    // With inverse on a space, the cell background should be painted with
    // DEFAULT_FG_U32 (the swap makes fg->bg and bg->fg).
    assert!(
        cell.iter().all(|&p| p == DEFAULT_FG_U32),
        "inverse space cell must be filled with the default foreground color as background"
    );

    // The non-inverse cell should remain DEFAULT_BG_U32.
    let normal_cell = ctx.cell_pixels(0, 1);
    assert!(
        normal_cell.iter().all(|&p| p == DEFAULT_BG_U32),
        "non-inverse blank cell must remain default background"
    );
}

#[test]
fn dim_attr_halves_rendered_pixel_brightness() {
    let cols: usize = 2;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let bright_fg = Color::Rgb(200, 100, 50);
    let normal_attrs = Attrs::default().with_fg(bright_fg);
    let dim_attrs = Attrs::default().with_fg(bright_fg).with_dim();

    state
        .grid
        .put_char(0, 0, 'M', normal_attrs)
        .expect("put normal M");
    state
        .grid
        .put_char(0, 1, 'M', dim_attrs)
        .expect("put dim M");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // Collect the brightest pixel from each cell (highest R channel)
    // to verify that dim reduces intensity.
    let normal_pixels = ctx.cell_pixels(0, 0);
    let dim_pixels = ctx.cell_pixels(0, 1);

    let max_r = |pixels: &[u32]| -> u8 {
        pixels
            .iter()
            .filter(|&&p| p != DEFAULT_BG_U32)
            .map(|&p| (p >> 16) as u8)
            .max()
            .unwrap_or(0)
    };

    let normal_max_r = max_r(&normal_pixels);
    let dim_max_r = max_r(&dim_pixels);

    assert!(
        normal_max_r > 0,
        "normal text must produce visible foreground pixels"
    );
    assert!(
        dim_max_r < normal_max_r,
        "dim text peak red ({dim_max_r}) must be less than normal text peak red ({normal_max_r})"
    );
}

#[test]
fn strikethrough_draws_line_at_mid_height() {
    let cols: usize = 2;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let st_attrs = Attrs::default()
        .with_fg(Color::Rgb(255, 0, 0))
        .with_strikethrough();
    state
        .grid
        .put_char(0, 0, ' ', st_attrs)
        .expect("put strikethrough space");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // Strikethrough is drawn at y = base_y + CELL_HEIGHT/2 = pixel row 8.
    let strike_y = CELL_HEIGHT / 2;
    let fg_u32 = rgb_to_u32(255, 0, 0);
    for px in 0..CELL_WIDTH {
        assert_eq!(
            ctx.pixel_at(px, strike_y),
            fg_u32,
            "strikethrough pixel at ({px}, {strike_y}) must be fg color"
        );
    }

    // One pixel row above should remain background.
    if strike_y > 0 {
        for px in 0..CELL_WIDTH {
            assert_eq!(
                ctx.pixel_at(px, strike_y - 1),
                DEFAULT_BG_U32,
                "pixel above strikethrough line must be background"
            );
        }
    }
}

#[test]
fn underline_draws_line_at_cell_bottom() {
    let cols: usize = 2;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let ul_attrs = Attrs::default()
        .with_fg(Color::Rgb(0, 255, 0))
        .with_underline();
    state
        .grid
        .put_char(0, 0, ' ', ul_attrs)
        .expect("put underlined space");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // Underline is drawn at y = base_y + CELL_HEIGHT - 1 = pixel row 15.
    let ul_y = CELL_HEIGHT - 1;
    let fg_u32 = rgb_to_u32(0, 255, 0);
    for px in 0..CELL_WIDTH {
        assert_eq!(
            ctx.pixel_at(px, ul_y),
            fg_u32,
            "underline pixel at ({px}, {ul_y}) must be fg color"
        );
    }

    // Pixel row above underline should remain background (space glyph).
    if ul_y > 1 {
        for px in 0..CELL_WIDTH {
            assert_eq!(
                ctx.pixel_at(px, ul_y - 1),
                DEFAULT_BG_U32,
                "pixel above underline must be background"
            );
        }
    }
}

#[test]
fn dotted_underline_draws_alternating_bottom_pixels() {
    let cols: usize = 1;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let attrs = Attrs::default()
        .with_fg(Color::Rgb(255, 255, 0))
        .with_dotted_underline();
    state
        .grid
        .put_char(0, 0, ' ', attrs)
        .expect("put dotted underline");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    let ul_y = CELL_HEIGHT - 1;
    let fg_u32 = rgb_to_u32(255, 255, 0);
    for px in 0..CELL_WIDTH {
        let expected = if px % 2 == 0 { fg_u32 } else { DEFAULT_BG_U32 };
        assert_eq!(
            ctx.pixel_at(px, ul_y),
            expected,
            "unexpected dotted pixel at {px}"
        );
    }
}

#[test]
fn dashed_underline_draws_repeating_runs_at_cell_bottom() {
    let cols: usize = 1;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let attrs = Attrs::default()
        .with_fg(Color::Rgb(255, 128, 0))
        .with_dashed_underline();
    state
        .grid
        .put_char(0, 0, ' ', attrs)
        .expect("put dashed underline");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    let ul_y = CELL_HEIGHT - 1;
    let fg_u32 = rgb_to_u32(255, 128, 0);
    for px in 0..CELL_WIDTH {
        let expected = if px % 4 < 3 { fg_u32 } else { DEFAULT_BG_U32 };
        assert_eq!(
            ctx.pixel_at(px, ul_y),
            expected,
            "unexpected dashed pixel at {px}"
        );
    }
}

#[test]
fn curly_underline_draws_wave_across_last_three_rows() {
    let cols: usize = 1;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let attrs = Attrs::default()
        .with_fg(Color::Rgb(0, 255, 255))
        .with_curly_underline();
    state
        .grid
        .put_char(0, 0, ' ', attrs)
        .expect("put curly underline");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    let fg_u32 = rgb_to_u32(0, 255, 255);
    let expected_offsets = [2usize, 1, 0, 1, 2, 1, 0, 1];
    for (px, offset) in expected_offsets.iter().copied().enumerate() {
        let py = CELL_HEIGHT - 3 + offset;
        assert_eq!(
            ctx.pixel_at(px, py),
            fg_u32,
            "missing curly pixel at ({px}, {py})"
        );
    }
}

// ===========================================================================
// Wide Character Rendering
// ===========================================================================

#[test]
fn wide_char_renders_across_two_cell_columns() {
    let cols: usize = 4;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Place a CJK character (width=2) at col 0.
    state
        .grid
        .put_char_with_width(0, 0, '\u{6F22}', Attrs::default(), 2)
        .expect("put wide char");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // The wide character glyph should produce non-bg pixels across both
    // cell 0 and potentially cell 1.
    let cell0 = ctx.cell_pixels(0, 0);
    let cell0_non_bg = cell0.iter().filter(|&&p| p != DEFAULT_BG_U32).count();

    // Cell 2 (first normal-width cell after the wide char) should be blank.
    let cell2 = ctx.cell_pixels(0, 2);
    let cell2_non_bg = cell2.iter().filter(|&&p| p != DEFAULT_BG_U32).count();

    assert!(
        cell0_non_bg > 0,
        "wide character owning cell must contain glyph pixels"
    );
    assert_eq!(
        cell2_non_bg, 0,
        "cell after the wide char pair must be blank background"
    );
}

#[test]
fn continuation_cell_renders_no_extra_glyph() {
    let cols: usize = 4;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Place a normal char at col 0 for comparison.
    state
        .grid
        .put_char(0, 0, 'A', Attrs::default())
        .expect("put normal A");
    // Place a wide char at col 2 (continuation at col 3).
    state
        .grid
        .put_char_with_width(0, 2, '\u{6F22}', Attrs::default(), 2)
        .expect("put wide char");

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);

    // The continuation cell (col 3) should not render its own glyph.
    // It is either part of the wide char quad or blank bg. The rasterizer
    // skips cells with width==0, so no separate glyph is drawn there.
    // We verify by checking that col 3 does not have MORE glyph pixels
    // than the wide char's owning cell (col 2) alone - because the
    // continuation cell contributes no independent glyph.
    let cell3 = ctx.cell_pixels(0, 3);
    let cell0 = ctx.cell_pixels(0, 0);

    // The continuation cell pixels should be all-background because the
    // rasterizer skips width-0 cells entirely. The wide char quad draws
    // into cols 2..4 from the owning cell, but the continuation cell itself
    // emits no draw call.
    let cell0_non_bg = cell0.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    assert!(
        cell0_non_bg > 0,
        "normal char at col 0 must have glyph pixels"
    );

    // The wide char's owning cell (col 2) should have glyph pixels.
    let cell2 = ctx.cell_pixels(0, 2);
    let cell2_non_bg = cell2.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    assert!(
        cell2_non_bg > 0,
        "wide char owning cell at col 2 must have glyph pixels"
    );

    // The continuation cell (col 3) may receive bleed from the wide char
    // glyph (the font rasterizer may extend into it), but it does NOT
    // independently trigger a separate glyph render call. We just verify
    // the test completes without panic and the grid state is consistent.
    let cell3_non_bg = cell3.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    // The rasterizer skips width-0 continuation cells entirely, so no
    // independent glyph should be drawn. Any non-bg pixels indicate the
    // continuation cell was incorrectly treated as an owning cell.
    assert_eq!(
        cell3_non_bg, 0,
        "continuation cell should render no independent glyph pixels"
    );
}

// ===========================================================================
// Scrollback Rendering
// ===========================================================================

#[test]
fn scrollback_rows_render_with_default_fg() {
    let cols: usize = 10;
    let rows: usize = 3;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Push text into scrollback.
    state.scrollback.push_from_cells(&cells_from_str("ABCDE"));
    state.scrollback.push_from_cells(&cells_from_str("FGHIJ"));

    let mut ctx = RenderCtx::new(cols, rows);
    // Render with viewport_offset=2 so the top 2 rows come from scrollback.
    ctx.render_with_viewport(&mut state, 2);

    // Row 0 should contain scrollback line 0 ("ABCDE") rendered with
    // DEFAULT_FG_U32 glyphs on DEFAULT_BG_U32 background.
    let row0 = ctx.pixel_row(0);
    let row0_non_bg = row0.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    assert!(
        row0_non_bg > 0,
        "scrollback row 0 must contain visible glyph pixels"
    );

    // Row 1 should also have scrollback content.
    let row1 = ctx.pixel_row(1);
    let row1_non_bg = row1.iter().filter(|&&p| p != DEFAULT_BG_U32).count();
    assert!(
        row1_non_bg > 0,
        "scrollback row 1 must contain visible glyph pixels"
    );
}

#[test]
fn scrollback_offset_shifts_visible_content() {
    let cols: usize = 5;
    let rows: usize = 2;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    state.scrollback.push_from_cells(&cells_from_str("HELLO"));
    state.scrollback.push_from_cells(&cells_from_str("WORLD"));

    // Render without scrollback (offset = 0).
    let mut ctx_no_scroll = RenderCtx::new(cols, rows);
    ctx_no_scroll.render_with_viewport(&mut state, 0);
    let snapshot_no_scroll = ctx_no_scroll.buffer.clone();

    // Render with scrollback visible (offset = 1).
    let mut ctx_scroll = RenderCtx::new(cols, rows);
    ctx_scroll.render_with_viewport(&mut state, 1);
    let snapshot_scroll = ctx_scroll.buffer.clone();

    // The two renders must produce different framebuffers because one shows
    // scrollback content and the other does not.
    assert_ne!(
        snapshot_no_scroll, snapshot_scroll,
        "viewport_offset must shift visible content"
    );
}

// ===========================================================================
// Selection Rendering
// ===========================================================================

#[test]
fn selection_uses_theme_background_for_covered_blank_cells() {
    let cols: usize = 4;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Render without selection first.
    let mut ctx_no_sel = RenderCtx::new(cols, rows);
    ctx_no_sel.render_full(&mut state);
    let no_sel_cell1 = ctx_no_sel.cell_pixels(0, 1);

    // Render with selection covering cell (0, 1).
    // Flat index for (row=0, col=1) = 0 * 4 + 1 = 1.
    let mut state2 = state_with_default_scrollback(cols as u16, rows as u16);
    state2.cursor.visible = false;
    let mut ctx_sel = RenderCtx::new(cols, rows);
    ctx_sel.render_with_selection(&mut state2, 1, 1);
    let sel_cell1 = ctx_sel.cell_pixels(0, 1);
    let (_, selection_bg) = state2.selection_colors();

    assert_ne!(
        no_sel_cell1, sel_cell1,
        "selection theming must change pixels in the covered cell"
    );

    for (idx, &selected) in sel_cell1.iter().enumerate() {
        assert_eq!(
            selected, selection_bg,
            "pixel {idx} in selected cell must match the theme selection background"
        );
    }
}

#[test]
fn cursor_overlay_wins_over_selection_on_cursor_cell() {
    let cols: usize = 4;
    let rows: usize = 1;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.row = 0;
    state.cursor.col = 1;
    state.cursor.visible = true;

    // Render with selection covering the entire row including cursor cell.
    // Flat indices: 0..3 for row 0 of a 4-column grid.
    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_with_selection(&mut state, 0, 3);

    // Now render cursor-only (no selection) for comparison.
    let mut state_cursor_only = state_with_default_scrollback(cols as u16, rows as u16);
    state_cursor_only.cursor.row = 0;
    state_cursor_only.cursor.col = 1;
    state_cursor_only.cursor.visible = true;

    let mut ctx_cursor_only = RenderCtx::new(cols, rows);
    ctx_cursor_only.render_full(&mut state_cursor_only);

    let sel_cursor_cell = ctx.cell_pixels(0, 1);
    let only_cursor_cell = ctx_cursor_only.cell_pixels(0, 1);
    assert_eq!(
        sel_cursor_cell, only_cursor_cell,
        "cursor cell must look identical whether or not selection covers it, \
         because cursor theming should override the selection highlight"
    );
}

// ===========================================================================
// Stress
// ===========================================================================

#[test]
fn render_100_consecutive_full_frames_is_stable() {
    let cols: usize = 20;
    let rows: usize = 10;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    // Populate grid with some content.
    for col in 0..cols.min(state.grid.width() as usize) {
        state
            .grid
            .put_char(0, col as u16, 'A', Attrs::default())
            .expect("put char");
    }

    let mut ctx = RenderCtx::new(cols, rows);
    ctx.render_full(&mut state);
    let reference = ctx.buffer.clone();

    for iteration in 1..100 {
        ctx.buffer.fill(0);
        ctx.render_full(&mut state);
        assert_eq!(
            ctx.buffer, reference,
            "frame {iteration} must match the reference frame"
        );
    }
}

#[test]
fn rapid_dirty_clear_cycles_1000_iterations() {
    let cols: usize = 8;
    let rows: usize = 4;
    let mut state = state_with_default_scrollback(cols as u16, rows as u16);
    state.cursor.visible = false;

    let mut ctx = RenderCtx::new(cols, rows);

    for i in 0u16..1000 {
        let row = i % (rows as u16);
        let col = i % (cols as u16);
        let ch = (b'A' + (i % 26) as u8) as char;
        state
            .grid
            .put_char(row, col, ch, Attrs::default())
            .expect("put char in cycle");

        ctx.render_full(&mut state);

        // After render, dirty rows should be cleared.
        assert!(
            state.grid.dirty_rows().iter().all(|dirty| !*dirty),
            "dirty flags must be cleared after render at iteration {i}"
        );
    }
}
