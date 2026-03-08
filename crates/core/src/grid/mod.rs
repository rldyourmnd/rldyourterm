mod operations;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_stress;

pub const BLANK_CHAR: char = ' ';
pub const CELL_WIDTH: usize = 8;
pub const CELL_HEIGHT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub attrs: Attrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: BLANK_CHAR,
            attrs: Attrs::default(),
        }
    }
}

#[rustfmt::skip]
pub static ANSI_PALETTE: [u32; 256] = {
    let mut palette = [0u32; 256];

    // Standard 16 colors (0-15)
    palette[0]  = 0x00_000000; // black
    palette[1]  = 0x00_aa0000; // red
    palette[2]  = 0x00_00aa00; // green
    palette[3]  = 0x00_aa5500; // yellow
    palette[4]  = 0x00_0000aa; // blue
    palette[5]  = 0x00_aa00aa; // magenta
    palette[6]  = 0x00_00aaaa; // cyan
    palette[7]  = 0x00_aaaaaa; // white
    palette[8]  = 0x00_555555; // bright black
    palette[9]  = 0x00_ff5555; // bright red
    palette[10] = 0x00_55ff55; // bright green
    palette[11] = 0x00_ffff55; // bright yellow
    palette[12] = 0x00_5555ff; // bright blue
    palette[13] = 0x00_ff55ff; // bright magenta
    palette[14] = 0x00_55ffff; // bright cyan
    palette[15] = 0x00_ffffff; // bright white

    // 216 color cube (16-231): 6x6x6 with levels [0, 95, 135, 175, 215, 255]
    let levels: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let mut i = 16usize;
    let mut ri = 0usize;
    while ri < 6 {
        let mut gi = 0usize;
        while gi < 6 {
            let mut bi = 0usize;
            while bi < 6 {
                palette[i] = ((levels[ri] as u32) << 16)
                    | ((levels[gi] as u32) << 8)
                    | (levels[bi] as u32);
                i += 1;
                bi += 1;
            }
            gi += 1;
        }
        ri += 1;
    }

    // 24-step grayscale ramp (232-255): 8, 18, 28, ..., 238
    let mut g = 0usize;
    while g < 24 {
        let level = (8 + 10 * g) as u32;
        palette[232 + g] = (level << 16) | (level << 8) | level;
        g += 1;
    }

    palette
};

pub const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
pub const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);

/// Converts a terminal `Color` to a packed RGB `u32` (`0x00RRGGBB`).
/// `default` is used when the color is `Color::Default`.
#[must_use]
pub fn color_to_u32(color: Color, default: (u8, u8, u8)) -> u32 {
    match color {
        Color::Default => {
            ((default.0 as u32) << 16) | ((default.1 as u32) << 8) | (default.2 as u32)
        }
        Color::Indexed(idx) => ANSI_PALETTE[idx as usize],
        Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) cells: Vec<Cell>,
    pub(super) dirty_rows: Vec<bool>,
    pub(super) scroll_count: usize,
}
