use super::{
    CpuRenderer, CpuRendererConfig, DEFAULT_BG_U32, DEFAULT_FG_U32, render_terminal_buffer,
    resolve_cell_colors, rgb_to_u32,
};
use rldyourterm_font::GlyphCache;
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Color, DEFAULT_SCROLLBACK_CAP, TerminalState,
};

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
        true,
        0,
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

#[test]
fn resolve_cell_colors_default_attrs_produce_default_colors() {
    let (fg, bg) = resolve_cell_colors(&Attrs::default());
    assert_eq!(fg, DEFAULT_FG_U32);
    assert_eq!(bg, DEFAULT_BG_U32);
}

#[test]
fn resolve_cell_colors_hidden_makes_fg_equal_bg() {
    let attrs = Attrs {
        hidden: true,
        ..Attrs::default()
    };
    let (fg, bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, bg, "hidden text: fg must equal bg");
}

#[test]
fn resolve_cell_colors_dim_halves_fg_brightness() {
    let attrs = Attrs {
        fg: Color::Rgb(200, 100, 50),
        dim: true,
        ..Attrs::default()
    };
    let (fg, _bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, rgb_to_u32(100, 50, 25));
}

#[test]
fn resolve_cell_colors_inverse_swaps_fg_bg() {
    let attrs = Attrs {
        fg: Color::Rgb(255, 0, 0),
        bg: Color::Rgb(0, 0, 255),
        inverse: true,
        ..Attrs::default()
    };
    let (fg, bg) = resolve_cell_colors(&attrs);
    assert_eq!(fg, rgb_to_u32(0, 0, 255), "fg should be original bg");
    assert_eq!(bg, rgb_to_u32(255, 0, 0), "bg should be original fg");
}
