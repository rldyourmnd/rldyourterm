// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

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
#[repr(u16)]
pub enum UnderlineStyle {
    #[default]
    None = 0,
    Single = 1,
    Double = 2,
    Curly = 3,
    Dotted = 4,
    Dashed = 5,
}

const ATTR_BOLD: u16 = 1 << 0;
const ATTR_DIM: u16 = 1 << 1;
const ATTR_ITALIC: u16 = 1 << 2;
const ATTR_UNDERLINE_STYLE_SHIFT: u16 = 3;
const ATTR_UNDERLINE_STYLE_MASK: u16 = 0b111 << ATTR_UNDERLINE_STYLE_SHIFT;
const ATTR_OVERLINE: u16 = 1 << 6;
const ATTR_INVERSE: u16 = 1 << 7;
const ATTR_HIDDEN: u16 = 1 << 8;
const ATTR_BLINK: u16 = 1 << 9;
const ATTR_STRIKETHROUGH: u16 = 1 << 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    pub underline_color: Color,
    flags: u16,
}

impl Attrs {
    fn get(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    fn set(&mut self, flag: u16, value: bool) {
        if value {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    pub fn bold(&self) -> bool {
        self.get(ATTR_BOLD)
    }
    pub fn set_bold(&mut self, v: bool) {
        self.set(ATTR_BOLD, v);
    }
    pub fn dim(&self) -> bool {
        self.get(ATTR_DIM)
    }
    pub fn set_dim(&mut self, v: bool) {
        self.set(ATTR_DIM, v);
    }
    pub fn italic(&self) -> bool {
        self.get(ATTR_ITALIC)
    }
    pub fn set_italic(&mut self, v: bool) {
        self.set(ATTR_ITALIC, v);
    }
    pub fn underline_style(&self) -> UnderlineStyle {
        match (self.flags & ATTR_UNDERLINE_STYLE_MASK) >> ATTR_UNDERLINE_STYLE_SHIFT {
            1 => UnderlineStyle::Single,
            2 => UnderlineStyle::Double,
            3 => UnderlineStyle::Curly,
            4 => UnderlineStyle::Dotted,
            5 => UnderlineStyle::Dashed,
            _ => UnderlineStyle::None,
        }
    }
    pub fn set_underline_style(&mut self, style: UnderlineStyle) {
        self.flags &= !ATTR_UNDERLINE_STYLE_MASK;
        self.flags |= (style as u16) << ATTR_UNDERLINE_STYLE_SHIFT;
    }
    pub fn has_underline(&self) -> bool {
        self.underline_style() != UnderlineStyle::None
    }
    pub fn underline(&self) -> bool {
        self.underline_style() == UnderlineStyle::Single
    }
    pub fn set_underline(&mut self, v: bool) {
        self.set_underline_style(if v {
            UnderlineStyle::Single
        } else {
            UnderlineStyle::None
        });
    }
    pub fn double_underline(&self) -> bool {
        self.underline_style() == UnderlineStyle::Double
    }
    pub fn set_double_underline(&mut self, v: bool) {
        self.set_underline_style(if v {
            UnderlineStyle::Double
        } else {
            UnderlineStyle::None
        });
    }
    pub fn curly_underline(&self) -> bool {
        self.underline_style() == UnderlineStyle::Curly
    }
    pub fn dotted_underline(&self) -> bool {
        self.underline_style() == UnderlineStyle::Dotted
    }
    pub fn dashed_underline(&self) -> bool {
        self.underline_style() == UnderlineStyle::Dashed
    }
    pub fn overline(&self) -> bool {
        self.get(ATTR_OVERLINE)
    }
    pub fn set_overline(&mut self, v: bool) {
        self.set(ATTR_OVERLINE, v);
    }
    pub fn inverse(&self) -> bool {
        self.get(ATTR_INVERSE)
    }
    pub fn set_inverse(&mut self, v: bool) {
        self.set(ATTR_INVERSE, v);
    }
    pub fn hidden(&self) -> bool {
        self.get(ATTR_HIDDEN)
    }
    pub fn set_hidden(&mut self, v: bool) {
        self.set(ATTR_HIDDEN, v);
    }
    pub fn blink(&self) -> bool {
        self.get(ATTR_BLINK)
    }
    pub fn set_blink(&mut self, v: bool) {
        self.set(ATTR_BLINK, v);
    }
    pub fn strikethrough(&self) -> bool {
        self.get(ATTR_STRIKETHROUGH)
    }
    pub fn set_strikethrough(&mut self, v: bool) {
        self.set(ATTR_STRIKETHROUGH, v);
    }

    #[must_use]
    pub fn with_fg(mut self, color: Color) -> Self {
        self.fg = color;
        self
    }
    #[must_use]
    pub fn with_bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }
    #[must_use]
    pub fn with_underline_color_value(mut self, color: Color) -> Self {
        self.underline_color = color;
        self
    }

    #[must_use]
    pub fn with_bold(mut self) -> Self {
        self.set_bold(true);
        self
    }
    #[must_use]
    pub fn with_dim(mut self) -> Self {
        self.set_dim(true);
        self
    }
    #[must_use]
    pub fn with_italic(mut self) -> Self {
        self.set_italic(true);
        self
    }
    #[must_use]
    pub fn with_underline(mut self) -> Self {
        self.set_underline(true);
        self
    }
    #[must_use]
    pub fn with_double_underline(mut self) -> Self {
        self.set_double_underline(true);
        self
    }
    #[must_use]
    pub fn with_curly_underline(mut self) -> Self {
        self.set_underline_style(UnderlineStyle::Curly);
        self
    }
    #[must_use]
    pub fn with_dotted_underline(mut self) -> Self {
        self.set_underline_style(UnderlineStyle::Dotted);
        self
    }
    #[must_use]
    pub fn with_dashed_underline(mut self) -> Self {
        self.set_underline_style(UnderlineStyle::Dashed);
        self
    }
    #[must_use]
    pub fn with_overline(mut self) -> Self {
        self.set_overline(true);
        self
    }
    #[must_use]
    pub fn with_inverse(mut self) -> Self {
        self.set_inverse(true);
        self
    }
    #[must_use]
    pub fn with_hidden(mut self) -> Self {
        self.set_hidden(true);
        self
    }
    #[must_use]
    pub fn with_blink(mut self) -> Self {
        self.set_blink(true);
        self
    }
    #[must_use]
    pub fn with_strikethrough(mut self) -> Self {
        self.set_strikethrough(true);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub attrs: Attrs,
    pub width: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellText<'a> {
    Char(char),
    Text(&'a str),
}

impl CellText<'_> {
    #[must_use]
    pub fn is_blank_space(self) -> bool {
        match self {
            Self::Char(ch) => ch == BLANK_CHAR,
            Self::Text(text) => text == " ",
        }
    }

    pub fn append_to(self, out: &mut String) {
        match self {
            Self::Char(ch) => out.push(ch),
            Self::Text(text) => out.push_str(text),
        }
    }
}

impl Cell {
    #[must_use]
    pub fn blank_with_bg(bg: Color) -> Self {
        Self {
            ch: BLANK_CHAR,
            attrs: Attrs::default().with_bg(bg),
            width: 1,
        }
    }

    #[must_use]
    pub fn text(&self) -> CellText<'_> {
        CellText::Char(self.ch)
    }

    #[must_use]
    pub fn is_blank_space(&self) -> bool {
        self.text().is_blank_space()
    }

    pub fn append_text_to(&self, out: &mut String) {
        self.text().append_to(out);
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::blank_with_bg(Color::Default)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    base_colors: [u32; 256],
    colors: [u32; 256],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalTheme {
    pub default_fg: (u8, u8, u8),
    pub default_bg: (u8, u8, u8),
    pub cursor_fg: (u8, u8, u8),
    pub cursor_bg: (u8, u8, u8),
    pub selection_fg: (u8, u8, u8),
    pub selection_bg: (u8, u8, u8),
    pub palette: [u32; 256],
}

impl Palette {
    #[must_use]
    pub fn get(&self, index: u8) -> u32 {
        self.colors[index as usize]
    }

    pub fn set_rgb(&mut self, index: u8, rgb: (u8, u8, u8)) {
        self.colors[index as usize] = pack_rgb(rgb.0, rgb.1, rgb.2);
    }

    pub fn set_base_colors(&mut self, base_colors: [u32; 256]) {
        let previous_base_colors = self.base_colors;
        for (index, color) in self.colors.iter_mut().enumerate() {
            if *color == previous_base_colors[index] {
                *color = base_colors[index];
            }
        }
        self.base_colors = base_colors;
    }

    pub fn reset_color(&mut self, index: u8) {
        self.colors[index as usize] = self.base_colors[index as usize];
    }

    pub fn reset_all(&mut self) {
        self.colors = self.base_colors;
    }

    #[must_use]
    pub fn resolve_color(&self, color: Color, default: (u8, u8, u8)) -> u32 {
        match color {
            Color::Default => pack_rgb(default.0, default.1, default.2),
            Color::Indexed(idx) => self.get(idx),
            Color::Rgb(r, g, b) => pack_rgb(r, g, b),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            base_colors: ANSI_PALETTE,
            colors: ANSI_PALETTE,
        }
    }
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self {
            default_fg: DEFAULT_FG,
            default_bg: DEFAULT_BG,
            cursor_fg: DEFAULT_BG,
            cursor_bg: DEFAULT_FG,
            selection_fg: DEFAULT_FG,
            selection_bg: (0x3e, 0x4b, 0x53),
            palette: ANSI_PALETTE,
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

const fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Converts a terminal `Color` to a packed RGB `u32` (`0x00RRGGBB`).
/// `default` is used when the color is `Color::Default`.
#[must_use]
pub fn color_to_u32(color: Color, default: (u8, u8, u8)) -> u32 {
    match color {
        Color::Default => pack_rgb(default.0, default.1, default.2),
        Color::Indexed(idx) => ANSI_PALETTE[idx as usize],
        Color::Rgb(r, g, b) => pack_rgb(r, g, b),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) cells: Vec<Cell>,
    pub(super) dirty_rows: Vec<bool>,
    /// Per-row soft-wrap flag. `wrapped[r] == true` means row `r` is a continuation
    /// of the previous row's content (caused by auto-wrap, not an explicit newline).
    /// Used by `resize_with_reflow` to merge logical lines during width changes.
    pub(super) wrapped: Vec<bool>,
    pub(super) scroll_count: usize,
}
