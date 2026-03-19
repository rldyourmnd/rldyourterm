// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_font::{GlyphBitmap, GlyphCache, GlyphKey};
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Cell, CellText, Color, DEFAULT_BG, DEFAULT_FG, TerminalState,
    UnderlineStyle, color_to_u32,
};

pub const DEFAULT_BG_U32: u32 = rgb_to_u32(DEFAULT_BG.0, DEFAULT_BG.1, DEFAULT_BG.2);
pub const DEFAULT_FG_U32: u32 = rgb_to_u32(DEFAULT_FG.0, DEFAULT_FG.1, DEFAULT_FG.2);
const SEARCH_HIT_TINT_NUMERATOR: u32 = 3;
const SEARCH_HIT_TINT_DENOMINATOR: u32 = 16;

fn is_default_blank_cell(cell: &Cell) -> bool {
    cell.is_blank_space() && cell.width == 1 && cell.attrs == Attrs::default()
}

fn glyph_key_for_cell(cell: &Cell) -> Option<GlyphKey> {
    match cell.text() {
        CellText::Char(ch) if ch == ' ' => None,
        CellText::Char(ch) => Some(GlyphKey::from(ch)),
        CellText::Text(text) if text == " " => None,
        CellText::Text(text) => Some(GlyphKey::from(text)),
    }
}

fn row_requires_redraw(
    terminal: &TerminalState,
    row: usize,
    prev_cursor_row: Option<u16>,
    viewport_offset: usize,
) -> bool {
    let row_u16 = row as u16;
    terminal
        .grid
        .dirty_rows()
        .get(row)
        .copied()
        .unwrap_or(false)
        || (viewport_offset == 0
            && (row_u16 == terminal.cursor.row || prev_cursor_row == Some(row_u16)))
}

fn collect_current_damage_rows(
    terminal: &TerminalState,
    visible_rows: usize,
    prev_cursor_row: Option<u16>,
    viewport_offset: usize,
    current_damage_rows_scratch: &mut Vec<u16>,
) {
    let mut current_damage_rows = std::mem::take(current_damage_rows_scratch);
    current_damage_rows.clear();
    if current_damage_rows.capacity() < (visible_rows / 4 + 2) {
        current_damage_rows.reserve((visible_rows / 4 + 2) - current_damage_rows.capacity());
    }

    for row in 0..visible_rows {
        if row_requires_redraw(terminal, row, prev_cursor_row, viewport_offset) {
            current_damage_rows.push(row as u16);
        }
    }

    *current_damage_rows_scratch = current_damage_rows;
}

fn build_rows_to_repaint(
    visible_rows: usize,
    framebuffer_age: u8,
    previous_damage_rows: &[u16],
    current_damage_rows: &[u16],
    repaint_rows_scratch: &mut Vec<u16>,
) {
    let mut repaint_rows = std::mem::take(repaint_rows_scratch);
    repaint_rows.clear();
    if repaint_rows.capacity() < (visible_rows / 4 + 2) {
        repaint_rows.reserve((visible_rows / 4 + 2) - repaint_rows.capacity());
    }

    match framebuffer_age {
        1 => repaint_rows.extend(current_damage_rows.iter().copied()),
        2 => {
            let mut previous_iter = previous_damage_rows
                .iter()
                .copied()
                .take_while(|&row| (row as usize) < visible_rows)
                .peekable();
            let mut current_iter = current_damage_rows.iter().copied().peekable();

            loop {
                match (previous_iter.peek().copied(), current_iter.peek().copied()) {
                    (Some(previous_row), Some(current_row)) if previous_row < current_row => {
                        repaint_rows.push(previous_row);
                        previous_iter.next();
                    }
                    (Some(previous_row), Some(current_row)) if previous_row > current_row => {
                        repaint_rows.push(current_row);
                        current_iter.next();
                    }
                    (Some(previous_row), Some(_current_row)) => {
                        repaint_rows.push(previous_row);
                        previous_iter.next();
                        current_iter.next();
                    }
                    (Some(previous_row), None) => {
                        repaint_rows.push(previous_row);
                        previous_iter.next();
                    }
                    (None, Some(current_row)) => {
                        repaint_rows.push(current_row);
                        current_iter.next();
                    }
                    (None, None) => break,
                }
            }
        }
        _ => repaint_rows.extend((0..visible_rows).map(|row| row as u16)),
    }

    *repaint_rows_scratch = repaint_rows;
}

fn persist_repaint_history(repaint_rows: &[u16], persisted_damage_rows_scratch: &mut Vec<u16>) {
    let mut persisted_damage_rows = std::mem::take(persisted_damage_rows_scratch);
    persisted_damage_rows.clear();
    if persisted_damage_rows.capacity() < repaint_rows.len() {
        persisted_damage_rows.reserve(repaint_rows.len() - persisted_damage_rows.capacity());
    }
    persisted_damage_rows.extend_from_slice(repaint_rows);
    *persisted_damage_rows_scratch = persisted_damage_rows;
}

#[allow(clippy::too_many_arguments)]
pub fn render_terminal_buffer(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    terminal: &mut TerminalState,
    glyph_cache: &mut GlyphCache,
    framebuffer_age: u8,
    previous_damage_rows: &[u16],
    prev_cursor_row: Option<u16>,
    current_damage_rows_scratch: &mut Vec<u16>,
    repaint_rows_scratch: &mut Vec<u16>,
    persisted_damage_rows_scratch: &mut Vec<u16>,
    blink_visible: bool,
    viewport_offset: usize,
    selection_start: u32,
    selection_end: u32,
    search_hit_ranges: &[(u32, u32)],
    search_overlay_row: &[Cell],
) {
    if width == 0 || height == 0 {
        return;
    }

    let grid_rows = terminal.grid.height() as usize;
    let grid_cols = terminal.grid.width() as usize;
    let visible_rows = (height / CELL_HEIGHT).max(1).min(grid_rows);
    let visible_cols = (width / CELL_WIDTH).max(1).min(grid_cols);
    let (_, default_bg) = terminal.resolve_cell_colors(&Attrs::default());
    let overlay_active = !search_overlay_row.is_empty();
    let overlay_row = visible_rows.saturating_sub(1);
    let content_rows = if overlay_active {
        visible_rows.saturating_sub(1)
    } else {
        visible_rows
    };

    let effective_offset = viewport_offset.min(terminal.scrollback.len());
    let sb_rows_on_screen = effective_offset.min(content_rows);

    collect_current_damage_rows(
        terminal,
        visible_rows,
        prev_cursor_row,
        viewport_offset,
        current_damage_rows_scratch,
    );
    build_rows_to_repaint(
        visible_rows,
        framebuffer_age,
        previous_damage_rows,
        current_damage_rows_scratch,
        repaint_rows_scratch,
    );
    let repaint_rows = repaint_rows_scratch.as_slice();
    terminal.grid.clear_dirty_rows();

    if repaint_rows.is_empty() {
        persist_repaint_history(repaint_rows, persisted_damage_rows_scratch);
        return;
    }

    for &row in repaint_rows {
        let row_idx = row as usize;
        if row_idx >= visible_rows {
            continue;
        }
        let base_y = row_idx * CELL_HEIGHT;
        let clear_end_y = (base_y + CELL_HEIGHT).min(height);

        for py in base_y..clear_end_y {
            let start = py * width;
            buffer[start..start + width].fill(default_bg);
        }

        if overlay_active && row_idx == overlay_row {
            render_row_cells(
                buffer,
                width,
                height,
                base_y,
                terminal,
                search_overlay_row,
                visible_cols,
                row_idx * grid_cols,
                &[],
                glyph_cache,
                true,
            );
            continue;
        }

        if row_idx < sb_rows_on_screen {
            let sb_line_idx = terminal.scrollback.len() - effective_offset + row_idx;
            if let Some(cells) = terminal.scrollback.get(sb_line_idx) {
                render_row_cells(
                    buffer,
                    width,
                    height,
                    base_y,
                    terminal,
                    cells,
                    visible_cols,
                    row_idx * grid_cols,
                    search_hit_ranges,
                    glyph_cache,
                    true,
                );
            }
        } else {
            let grid_row = (row_idx - sb_rows_on_screen) as u16;
            if let Ok(cells) = terminal.grid.row_cells(grid_row) {
                let row_flat_start = row_idx * grid_cols;
                if cells.iter().take(visible_cols).all(is_default_blank_cell)
                    && !row_intersects_search_hit_ranges(
                        search_hit_ranges,
                        row_flat_start,
                        grid_cols,
                    )
                {
                    continue;
                }
                render_row_cells(
                    buffer,
                    width,
                    height,
                    base_y,
                    terminal,
                    cells,
                    visible_cols,
                    row_flat_start,
                    search_hit_ranges,
                    glyph_cache,
                    blink_visible,
                );
            }
        }
    }

    let grid_pixel_height = visible_rows * CELL_HEIGHT;
    if grid_pixel_height < height {
        let any_bottom_dirty = repaint_rows
            .iter()
            .any(|&row| (row as usize) + 1 >= visible_rows);
        if any_bottom_dirty {
            for py in grid_pixel_height..height {
                let start = py * width;
                buffer[start..start + width].fill(default_bg);
            }
        }
    }

    // Selection highlight: invert colors for selected cells on repainted rows only.
    // When the framebuffer age is 2, repaint_rows already includes the previous
    // frame's damage rows, so the buffer is brought forward to the current frame
    // before selection XOR is applied. Re-inverting untouched rows would still
    // toggle the highlight off because XOR is its own inverse.
    // Skip the cursor cell to avoid double-XOR with the cursor pass below
    // (two XOR operations cancel out, making the cursor invisible on selection).
    let cursor_flat = if viewport_offset == 0
        && terminal.cursor.visible
        && (terminal.cursor.row as usize) < content_rows
    {
        (terminal.cursor.row as usize) * grid_cols + terminal.cursor.col as usize
    } else {
        usize::MAX
    };
    if selection_start != u32::MAX {
        let sel_lo = selection_start.min(selection_end) as usize;
        let sel_hi = selection_start.max(selection_end) as usize;
        for &row in repaint_rows {
            let row_usize = row as usize;
            if overlay_active && row_usize == overlay_row {
                continue;
            }
            let row_flat_start = row_usize * grid_cols;
            let row_flat_end = row_flat_start + grid_cols - 1;
            if row_flat_end < sel_lo || row_flat_start > sel_hi {
                continue;
            }
            let start_col = sel_lo.saturating_sub(row_flat_start);
            let end_col = (sel_hi - row_flat_start).min(grid_cols - 1);
            for col in start_col..=end_col.min(visible_cols - 1) {
                if row_flat_start + col == cursor_flat {
                    continue;
                }
                draw_cell_invert(
                    buffer,
                    width,
                    height,
                    col * CELL_WIDTH,
                    row_usize * CELL_HEIGHT,
                );
            }
        }
    }

    if viewport_offset == 0 && terminal.cursor.visible {
        let cursor_row = terminal.cursor.row as usize;
        let cursor_col = terminal.cursor.col as usize;
        if cursor_row < content_rows && cursor_col < visible_cols {
            draw_cursor(
                buffer,
                width,
                height,
                cursor_col * CELL_WIDTH,
                cursor_row * CELL_HEIGHT,
                terminal.cursor_shape(),
                blink_visible,
            );
        }
    }

    persist_repaint_history(repaint_rows, persisted_damage_rows_scratch);
}

#[allow(clippy::too_many_arguments)]
fn render_row_cells(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    base_y: usize,
    terminal: &TerminalState,
    cells: &[Cell],
    visible_cols: usize,
    row_flat_start: usize,
    search_hit_ranges: &[(u32, u32)],
    glyph_cache: &mut GlyphCache,
    blink_visible: bool,
) {
    let (_, default_bg) = terminal.resolve_cell_colors(&Attrs::default());

    for (col, cell) in cells.iter().take(visible_cols).enumerate() {
        if cell.width == 0 {
            continue;
        }
        let cell_search_hit =
            cell_intersects_search_hit_ranges(search_hit_ranges, row_flat_start + col, cell.width);
        if is_default_blank_cell(cell) && !cell_search_hit {
            continue;
        }

        let x = col * CELL_WIDTH;
        let (fg, mut bg) = resolve_cell_colors_for_terminal(terminal, &cell.attrs);
        if cell_search_hit {
            bg = tint_search_hit_background(fg, bg);
        }

        let cell_pixel_width = if cell.width == 2 {
            CELL_WIDTH * 2
        } else {
            CELL_WIDTH
        };
        if bg != default_bg {
            draw_cell_bg(buffer, width, height, x, base_y, bg);
            if cell.width == 2 && col + 1 < visible_cols {
                draw_cell_bg(buffer, width, height, x + CELL_WIDTH, base_y, bg);
            }
        }

        let glyph_hidden_by_blink = cell.attrs.blink() && !blink_visible;
        if !glyph_hidden_by_blink && let Some(glyph_key) = glyph_key_for_cell(cell) {
            let glyph = glyph_cache.get(glyph_key);
            draw_glyph_blended(
                buffer,
                width,
                height,
                x,
                base_y,
                glyph,
                fg,
                cell.attrs.bold(),
            );
        }

        // Resolve underline decoration color (SGR 58 or fallback to fg).
        let underline_style = cell.attrs.underline_style();
        if underline_style != UnderlineStyle::None {
            let ul_color = if cell.attrs.underline_color == Color::Default {
                fg
            } else {
                terminal.resolve_color(cell.attrs.underline_color, DEFAULT_FG)
            };
            draw_underline_decoration(buffer, width, height, x, base_y, underline_style, ul_color);
            if cell_pixel_width > CELL_WIDTH && col + 1 < visible_cols {
                draw_underline_decoration(
                    buffer,
                    width,
                    height,
                    x + CELL_WIDTH,
                    base_y,
                    underline_style,
                    ul_color,
                );
            }
        }

        if cell.attrs.strikethrough() {
            draw_strikethrough(buffer, width, height, x, base_y, fg);
            if cell_pixel_width > CELL_WIDTH && col + 1 < visible_cols {
                draw_strikethrough(buffer, width, height, x + CELL_WIDTH, base_y, fg);
            }
        }

        if cell.attrs.overline() {
            draw_overline(buffer, width, height, x, base_y, fg);
            if cell_pixel_width > CELL_WIDTH && col + 1 < visible_cols {
                draw_overline(buffer, width, height, x + CELL_WIDTH, base_y, fg);
            }
        }
    }
}

fn resolve_cell_colors_for_terminal(terminal: &TerminalState, attrs: &Attrs) -> (u32, u32) {
    terminal.resolve_cell_colors(attrs)
}

fn row_intersects_search_hit_ranges(
    search_hit_ranges: &[(u32, u32)],
    row_flat_start: usize,
    row_width: usize,
) -> bool {
    if row_width == 0 || search_hit_ranges.is_empty() {
        return false;
    }
    let row_start = row_flat_start as u32;
    let row_end = row_start.saturating_add(row_width.saturating_sub(1) as u32);
    let first_candidate = search_hit_ranges.partition_point(|&(_, end)| end < row_start);
    search_hit_ranges
        .get(first_candidate)
        .is_some_and(|&(start, _)| start <= row_end)
}

fn cell_intersects_search_hit_ranges(
    search_hit_ranges: &[(u32, u32)],
    flat_start: usize,
    cell_width: u8,
) -> bool {
    if cell_width == 0 || search_hit_ranges.is_empty() {
        return false;
    }
    let start = flat_start as u32;
    let end = start.saturating_add(u32::from(cell_width.saturating_sub(1)));
    let first_candidate = search_hit_ranges.partition_point(|&(_, range_end)| range_end < start);
    search_hit_ranges
        .get(first_candidate)
        .is_some_and(|&(range_start, _)| range_start <= end)
}

fn tint_search_hit_background(fg: u32, bg: u32) -> u32 {
    let (fg_r, fg_g, fg_b) = u32_to_rgb(fg);
    let (bg_r, bg_g, bg_b) = u32_to_rgb(bg);
    rgb_to_u32(
        mix_search_hit_channel(bg_r, fg_r),
        mix_search_hit_channel(bg_g, fg_g),
        mix_search_hit_channel(bg_b, fg_b),
    )
}

fn mix_search_hit_channel(bg: u8, fg: u8) -> u8 {
    let bg = u32::from(bg);
    let fg = u32::from(fg);
    if fg >= bg {
        let delta = (fg - bg) * SEARCH_HIT_TINT_NUMERATOR / SEARCH_HIT_TINT_DENOMINATOR;
        bg.saturating_add(delta).min(u32::from(u8::MAX)) as u8
    } else {
        let delta = (bg - fg) * SEARCH_HIT_TINT_NUMERATOR / SEARCH_HIT_TINT_DENOMINATOR;
        bg.saturating_sub(delta) as u8
    }
}

pub fn resolve_cell_colors(attrs: &Attrs) -> (u32, u32) {
    let mut fg = color_to_u32(attrs.fg, DEFAULT_FG);
    let mut bg = color_to_u32(attrs.bg, DEFAULT_BG);

    if attrs.dim() {
        let (r, g, b) = u32_to_rgb(fg);
        fg = rgb_to_u32(r / 2, g / 2, b / 2);
    }

    if attrs.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }

    if attrs.hidden() {
        fg = bg;
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
    let end_x = (x + CELL_WIDTH).min(width);
    for py in y..(y + CELL_HEIGHT).min(height) {
        let row_start = py * width;
        buffer[row_start + x..row_start + end_x].fill(bg);
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
    let end_x = (x + CELL_WIDTH).min(width);
    buffer[row_start + x..row_start + end_x].fill(fg);
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
    let end_x = (x + CELL_WIDTH).min(width);
    buffer[row_start + x..row_start + end_x].fill(fg);
}

fn draw_underline_decoration(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    style: UnderlineStyle,
    fg: u32,
) {
    match style {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => draw_underline(buffer, width, height, x, y, fg),
        UnderlineStyle::Double => draw_double_underline(buffer, width, height, x, y, fg),
        UnderlineStyle::Curly => draw_curly_underline(buffer, width, height, x, y, fg),
        UnderlineStyle::Dotted => draw_patterned_underline(buffer, width, height, x, y, fg, 2, 1),
        UnderlineStyle::Dashed => draw_patterned_underline(buffer, width, height, x, y, fg, 4, 3),
    }
}

fn draw_patterned_underline(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    fg: u32,
    cycle: usize,
    on_pixels: usize,
) {
    let line_y = y + CELL_HEIGHT - 1;
    if line_y >= height || cycle == 0 {
        return;
    }
    let row_start = line_y * width;
    let end_x = (x + CELL_WIDTH).min(width);
    for px in x..end_x {
        if px % cycle < on_pixels {
            buffer[row_start + px] = fg;
        }
    }
}

fn draw_curly_underline(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    fg: u32,
) {
    const WAVE_OFFSETS: [usize; CELL_WIDTH] = [2, 1, 0, 1, 2, 1, 0, 1];

    for (offset_x, offset_y) in WAVE_OFFSETS.iter().copied().enumerate() {
        let px = x + offset_x;
        let py = y + CELL_HEIGHT - 3 + offset_y;
        if px < width && py < height {
            buffer[py * width + px] = fg;
        }
    }
}

fn draw_double_underline(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    fg: u32,
) {
    let end_x = (x + CELL_WIDTH).min(width);
    // Line 1: 2px above cell bottom
    let line_y1 = y + CELL_HEIGHT - 3;
    if line_y1 < height {
        let row_start = line_y1 * width;
        buffer[row_start + x..row_start + end_x].fill(fg);
    }
    // Line 2: at cell bottom
    let line_y2 = y + CELL_HEIGHT - 1;
    if line_y2 < height {
        let row_start = line_y2 * width;
        buffer[row_start + x..row_start + end_x].fill(fg);
    }
}

fn draw_overline(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize, fg: u32) {
    if y >= height {
        return;
    }
    let row_start = y * width;
    let end_x = (x + CELL_WIDTH).min(width);
    buffer[row_start + x..row_start + end_x].fill(fg);
}

/// Invert a full cell for selection highlighting.
fn draw_cell_invert(buffer: &mut [u32], width: usize, height: usize, x: usize, y: usize) {
    let end_x = (x + CELL_WIDTH).min(width);
    for glyph_y in 0..CELL_HEIGHT {
        let pixel_y = y + glyph_y;
        if pixel_y >= height {
            break;
        }
        let row_start = pixel_y * width;
        for pixel in &mut buffer[row_start + x..row_start + end_x] {
            *pixel ^= 0x00FF_FFFF;
        }
    }
}

/// Draw cursor with DECSCUSR shape support.
/// Shapes: 0/1=blinking block, 2=steady block, 3=blinking underline,
/// 4=steady underline, 5=blinking bar, 6=steady bar.
fn draw_cursor(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    cursor_shape: u8,
    blink_visible: bool,
) {
    // Blinking: shapes 0,1,3,5 use blink timer; 2,4,6 are steady.
    let is_blinking = cursor_shape == 0 || cursor_shape % 2 == 1;
    if is_blinking && !blink_visible {
        return;
    }

    let (y_start, y_end, x_start, x_end) = match cursor_shape {
        3 | 4 => {
            // Underline: bottom 2px of cell.
            let ul_start = CELL_HEIGHT.saturating_sub(2);
            (ul_start, CELL_HEIGHT, x, (x + CELL_WIDTH).min(width))
        }
        5 | 6 => {
            // Bar: left 2px of cell.
            (0, CELL_HEIGHT, x, (x + 2).min(width))
        }
        _ => {
            // Block (0/1/2 and unknown): full cell.
            (0, CELL_HEIGHT, x, (x + CELL_WIDTH).min(width))
        }
    };

    for glyph_y in y_start..y_end {
        let pixel_y = y + glyph_y;
        if pixel_y >= height {
            break;
        }
        let row_start = pixel_y * width;
        for pixel in &mut buffer[row_start + x_start..row_start + x_end] {
            *pixel ^= 0x00FF_FFFF;
        }
    }
}
