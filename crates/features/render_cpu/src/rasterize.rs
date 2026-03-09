use rldyourterm_font::{GlyphBitmap, GlyphCache};
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, DEFAULT_BG, DEFAULT_FG, TerminalState, color_to_u32,
};

pub const DEFAULT_BG_U32: u32 = rgb_to_u32(DEFAULT_BG.0, DEFAULT_BG.1, DEFAULT_BG.2);
pub const DEFAULT_FG_U32: u32 = rgb_to_u32(DEFAULT_FG.0, DEFAULT_FG.1, DEFAULT_FG.2);

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
