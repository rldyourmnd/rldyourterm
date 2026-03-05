use font8x8::{BLOCK_FONTS, BOX_FONTS, UnicodeFonts};
use fontdue::{Font, FontSettings};
use std::collections::HashMap;

/// Bundled JetBrains Mono Nerd Font Mono (SIL OFL 1.1).
/// Covers ASCII, Latin Extended, Cyrillic, Greek, Powerline, Nerd Font icons,
/// Box Drawing, and Block Elements.
static FONT_DATA: &[u8] =
    include_bytes!("../../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");

/// Pre-rasterized glyph bitmap sized to fit a terminal cell.
pub struct GlyphBitmap {
    /// Grayscale coverage (0-255), row-major, `glyph_width * glyph_height` elements.
    pub data: Vec<u8>,
    /// Horizontal offset from cell left edge to glyph start (pixels).
    pub x_offset: i32,
    /// Vertical offset from cell top edge to glyph start (pixels).
    pub y_offset: i32,
    /// Rasterized glyph width (may be smaller than cell_width).
    pub glyph_width: usize,
    /// Rasterized glyph height (may be smaller than cell_height).
    pub glyph_height: usize,
}

/// Glyph rasterization cache backed by fontdue + bundled Nerd Font.
///
/// Loads the embedded TTF at construction time. Rasterizes glyphs on demand
/// and caches the results for the lifetime of the cache.
pub struct GlyphCache {
    font: Font,
    px_size: f32,
    cell_width: u16,
    cell_height: u16,
    ascent_px: i32,
    cache: HashMap<char, GlyphBitmap>,
}

impl GlyphCache {
    /// Create a new cache calibrated for the given terminal cell dimensions.
    ///
    /// Computes `px_size` so that the font's monospace advance closely matches
    /// `cell_width`, ensuring glyphs fill each cell without overflow.
    #[must_use]
    pub fn new(cell_width: u16, cell_height: u16) -> Self {
        let font = Font::from_bytes(FONT_DATA, FontSettings::default())
            .expect("embedded font data must be valid");

        // Calibrate px_size: find the size where advance_width of 'M' matches cell_width.
        // Start with cell_height as initial guess (typical for monospace fonts).
        let initial_px = cell_height as f32;
        let metrics = font.metrics('M', initial_px);
        let px_size = if metrics.advance_width > 0.0 {
            (cell_width as f32 / metrics.advance_width) * initial_px
        } else {
            initial_px
        };

        // Compute baseline position from font metrics at calibrated size.
        // fontdue horizontal_line_metrics gives ascent/descent in font units scaled to px_size.
        let ascent_px = font
            .horizontal_line_metrics(px_size)
            .map(|lm| lm.ascent.round() as i32)
            .unwrap_or(cell_height as i32 - 2);

        Self {
            font,
            px_size,
            cell_width,
            cell_height,
            ascent_px,
            cache: HashMap::new(),
        }
    }

    /// Get or rasterize a glyph for `ch`. Returns a reference to the cached bitmap.
    pub fn get(&mut self, ch: char) -> &GlyphBitmap {
        if !self.cache.contains_key(&ch) {
            let bitmap = self.rasterize_into_cell(ch);
            self.cache.insert(ch, bitmap);
        }
        &self.cache[&ch]
    }

    /// Check if the font contains a real glyph for `ch` (not just .notdef / tofu).
    #[must_use]
    pub fn has_glyph(&self, ch: char) -> bool {
        self.font.lookup_glyph_index(ch) != 0
    }

    /// Rasterize a single character into a cell-sized coordinate space.
    ///
    /// For Box Drawing (U+2500-U+257F) and Block Elements (U+2580-U+259F) at 8px
    /// cell width, uses font8x8 pixel-perfect bitmaps scaled 2x vertically to fill
    /// 8x16 cells. All other characters use fontdue rasterization.
    fn rasterize_into_cell(&self, ch: char) -> GlyphBitmap {
        let cw = self.cell_width as usize;
        let ch_height = self.cell_height as usize;

        // Font8x8 path for pixel-perfect box drawing and block elements at 8px width.
        if cw == 8
            && ch_height == 16
            && let Some(bitmap) = self.try_font8x8_box_block(ch)
        {
            return bitmap;
        }

        // fontdue rasterization path
        let (metrics, bitmap) = self.font.rasterize(ch, self.px_size);

        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            return GlyphBitmap {
                data: Vec::new(),
                x_offset: 0,
                y_offset: 0,
                glyph_width: 0,
                glyph_height: 0,
            };
        }

        // Place glyph within cell using font metrics.
        // fontdue metrics.ymin = distance from baseline to bottom of glyph (positive = above baseline).
        // y_offset = distance from cell top to glyph top row.
        let y_offset = self.ascent_px - (metrics.ymin + metrics.height as i32);
        let x_offset = metrics.xmin;

        GlyphBitmap {
            data: bitmap,
            x_offset,
            y_offset,
            glyph_width: metrics.width,
            glyph_height: metrics.height,
        }
    }

    /// Try to render a Box Drawing or Block Element character using font8x8.
    /// Returns `None` if `ch` is not in these ranges.
    fn try_font8x8_box_block(&self, ch: char) -> Option<GlyphBitmap> {
        let code = ch as u32;
        let is_box = (0x2500..=0x257F).contains(&code);
        let is_block = (0x2580..=0x259F).contains(&code);

        if !is_box && !is_block {
            return None;
        }

        let raw: [u8; 8] = if is_box {
            BOX_FONTS.get(ch)?
        } else {
            BLOCK_FONTS.get(ch)?
        };

        // Scale 8x8 bitmap to 8x16 by doubling each row vertically.
        let mut data = vec![0u8; 8 * 16];
        for (gy, &row_bits) in raw.iter().enumerate() {
            for gx in 0..8usize {
                if (row_bits >> gx) & 1 != 0 {
                    let py0 = gy * 2;
                    let py1 = py0 + 1;
                    data[py0 * 8 + gx] = 255;
                    data[py1 * 8 + gx] = 255;
                }
            }
        }

        Some(GlyphBitmap {
            data,
            x_offset: 0,
            y_offset: 0,
            glyph_width: 8,
            glyph_height: 16,
        })
    }
}

/// Rasterize a glyph into a cell-sized RGBA buffer for the GPU atlas.
///
/// Returns a `cell_width * cell_height` grayscale buffer with the glyph
/// placed at the correct position within the cell. Caller owns the buffer.
pub fn rasterize_for_atlas(cache: &mut GlyphCache, ch: char) -> Vec<u8> {
    let cw = cache.cell_width as usize;
    let ch_val = cache.cell_height as usize;
    let glyph = cache.get(ch);

    let mut cell_buf = vec![0u8; cw * ch_val];

    for gy in 0..glyph.glyph_height {
        for gx in 0..glyph.glyph_width {
            let coverage = glyph.data[gy * glyph.glyph_width + gx];
            if coverage == 0 {
                continue;
            }
            let px = gx as i32 + glyph.x_offset;
            let py = gy as i32 + glyph.y_offset;
            if px < 0 || py < 0 {
                continue;
            }
            let px = px as usize;
            let py = py as usize;
            if px >= cw || py >= ch_val {
                continue;
            }
            cell_buf[py * cw + px] = coverage;
        }
    }

    cell_buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_rasterizes_ascii() {
        let mut cache = GlyphCache::new(8, 16);
        let glyph = cache.get('A');
        assert!(glyph.glyph_width > 0);
        assert!(glyph.glyph_height > 0);
        assert!(!glyph.data.is_empty());
    }

    #[test]
    fn cache_rasterizes_cyrillic() {
        let mut cache = GlyphCache::new(8, 16);
        assert!(cache.has_glyph('\u{0434}')); // Cyrillic small letter de
        let glyph = cache.get('\u{0434}');
        assert!(glyph.glyph_width > 0);
        assert!(!glyph.data.is_empty());
    }

    #[test]
    fn box_drawing_uses_font8x8() {
        let mut cache = GlyphCache::new(8, 16);
        let glyph = cache.get('\u{2500}'); // BOX DRAWINGS LIGHT HORIZONTAL
        // font8x8 path produces exact 8x16 bitmap
        assert_eq!(glyph.glyph_width, 8);
        assert_eq!(glyph.glyph_height, 16);
        assert_eq!(glyph.x_offset, 0);
        assert_eq!(glyph.y_offset, 0);
    }

    #[test]
    fn space_produces_empty_glyph() {
        let mut cache = GlyphCache::new(8, 16);
        let glyph = cache.get(' ');
        // Space has zero-size rasterization
        assert_eq!(glyph.glyph_width, 0);
    }

    #[test]
    fn rasterize_for_atlas_produces_cell_sized_buffer() {
        let mut cache = GlyphCache::new(8, 16);
        let buf = rasterize_for_atlas(&mut cache, 'A');
        assert_eq!(buf.len(), 8 * 16);
        // Buffer should have some non-zero pixels for 'A'
        assert!(buf.iter().any(|&b| b > 0));
    }

    #[test]
    fn has_glyph_returns_false_for_missing() {
        let cache = GlyphCache::new(8, 16);
        // Private Use Area char unlikely to be in font
        assert!(!cache.has_glyph('\u{FFFF}'));
    }
}
