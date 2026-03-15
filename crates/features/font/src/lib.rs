// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use font8x8::{BASIC_FONTS, BLOCK_FONTS, BOX_FONTS, UnicodeFonts};
use fontdue::{Font, FontSettings};
use std::collections::{HashMap, VecDeque};

/// Bundled JetBrains Mono Nerd Font Mono (SIL OFL 1.1).
/// Covers ASCII, Latin Extended, Cyrillic, Greek, Powerline, Nerd Font icons,
/// Box Drawing, and Block Elements.
static FONT_DATA: &[u8] =
    include_bytes!("../../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");

const DEFAULT_MAX_GLYPH_CACHE_ENTRIES: usize = 8_192;
const FALLBACK_GLYPH_CHAR: char = '?';

/// A single font in the fallback chain, calibrated to a specific cell size.
struct FontEntry {
    font: Font,
    ascent_px: i32,
}

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
/// and caches the results for the lifetime of the cache with an explicit upper
/// bound to prevent unbounded growth in long-running sessions.
///
/// Supports an ordered fallback chain: primary font is tried first, then
/// each fallback font in insertion order. ASCII glyphs hit the primary font
/// on the first check with zero fallback overhead.
pub struct GlyphCache {
    fonts: Vec<FontEntry>,
    px_size: f32,
    cell_width: u16,
    cell_height: u16,
    cache: HashMap<char, GlyphBitmap>,
    eviction_queue: VecDeque<char>,
    max_entries: usize,
}

impl GlyphCache {
    /// Create a new cache calibrated for the given terminal cell dimensions.
    ///
    /// Computes `px_size` so that the font's monospace advance closely matches
    /// `cell_width`, ensuring glyphs fill each cell without overflow.
    #[must_use]
    pub fn new(cell_width: u16, cell_height: u16) -> Self {
        Self::new_with_max_entries(cell_width, cell_height, DEFAULT_MAX_GLYPH_CACHE_ENTRIES)
    }

    /// Create a bounded cache with an explicit max entry limit.
    #[must_use]
    pub fn new_with_max_entries(cell_width: u16, cell_height: u16, max_entries: usize) -> Self {
        let initial_px = cell_height as f32;
        let primary = match Font::from_bytes(FONT_DATA, FontSettings::default()) {
            Ok(font) => {
                // Calibrate px_size: find the size where advance_width of 'M' matches cell_width.
                // Start with cell_height as initial guess (typical for monospace fonts).
                let metrics = font.metrics('M', initial_px);
                let px_size = if metrics.advance_width > 0.0 {
                    (cell_width as f32 / metrics.advance_width) * initial_px
                } else {
                    initial_px
                };
                let ascent_px = Self::compute_ascent(&font, px_size, cell_height);
                Some((FontEntry { font, ascent_px }, px_size))
            }
            // Preserve shell continuity if the bundled font asset is corrupted.
            Err(_) => None,
        };
        let (fonts, px_size) = match primary {
            Some((font_entry, px_size)) => (vec![font_entry], px_size),
            None => (Vec::new(), initial_px),
        };

        let max_entries = max_entries.max(1);
        let mut result = Self {
            fonts,
            px_size,
            cell_width,
            cell_height,
            cache: HashMap::with_capacity(max_entries.min(1024)),
            eviction_queue: VecDeque::new(),
            max_entries,
        };
        let fallback_bitmap = result.rasterize_into_cell(FALLBACK_GLYPH_CHAR);
        result.cache.insert(FALLBACK_GLYPH_CHAR, fallback_bitmap);
        result
    }

    /// Add a fallback font to the chain. Fonts are tried in insertion order
    /// after the primary (bundled) font. The font data must be valid TTF/OTF.
    ///
    /// Returns `true` if the font was loaded, `false` if parsing failed.
    pub fn add_fallback_font(&mut self, font_data: &[u8]) -> bool {
        let font = match Font::from_bytes(font_data, FontSettings::default()) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let ascent_px = Self::compute_ascent(&font, self.px_size, self.cell_height);
        self.fonts.push(FontEntry { font, ascent_px });
        true
    }

    /// Compute baseline ascent for a font at the given px_size.
    fn compute_ascent(font: &Font, px_size: f32, cell_height: u16) -> i32 {
        font.horizontal_line_metrics(px_size)
            .map(|lm| lm.ascent.round() as i32)
            .unwrap_or(cell_height as i32 - 2)
    }

    /// Get or rasterize a glyph for `ch`. Returns a reference to the cached bitmap.
    pub fn get(&mut self, ch: char) -> &GlyphBitmap {
        if !self.cache.contains_key(&ch) {
            if self.cache.len() >= self.max_entries {
                while let Some(candidate) = self.eviction_queue.pop_front() {
                    if self.cache.remove(&candidate).is_some() {
                        break;
                    }
                }
                if self.cache.len() >= self.max_entries {
                    let fallback_bitmap = self.rasterize_into_cell(FALLBACK_GLYPH_CHAR);
                    return self
                        .cache
                        .entry(FALLBACK_GLYPH_CHAR)
                        .or_insert(fallback_bitmap);
                }
            }

            let bitmap = self.rasterize_into_cell(ch);
            if ch != FALLBACK_GLYPH_CHAR {
                self.eviction_queue.push_back(ch);
            }
            self.cache.entry(ch).or_insert(bitmap);
        }

        self.cache
            .get(&ch)
            .unwrap_or_else(|| &self.cache[&FALLBACK_GLYPH_CHAR])
    }

    /// Check if any font in the chain contains a real glyph for `ch`.
    #[must_use]
    pub fn has_glyph(&self, ch: char) -> bool {
        if self.try_font8x8_box_block(ch).is_some() {
            return true;
        }
        if self.fonts.is_empty() {
            return self.try_font8x8_basic(ch).is_some();
        }

        self.fonts
            .iter()
            .any(|e| e.font.lookup_glyph_index(ch) != 0)
    }

    /// Rasterize a single character into a cell-sized coordinate space.
    ///
    /// For Box Drawing (U+2500-U+257F) and Block Elements (U+2580-U+259F) at 8px
    /// cell width, uses font8x8 pixel-perfect bitmaps scaled 2x vertically to fill
    /// 8x16 cells. All other characters use fontdue rasterization with fallback chain.
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

        if self.fonts.is_empty() {
            return self
                .try_font8x8_basic(ch)
                .or_else(|| self.try_font8x8_basic(FALLBACK_GLYPH_CHAR))
                .unwrap_or_else(Self::empty_glyph);
        }

        // Try each font in the chain until one has the glyph.
        for entry in &self.fonts {
            if entry.font.lookup_glyph_index(ch) == 0 {
                continue;
            }
            return Self::rasterize_with_font(entry, ch, self.px_size);
        }

        // No font has the glyph - rasterize with primary font (produces .notdef).
        Self::rasterize_with_font(&self.fonts[0], ch, self.px_size)
    }

    fn empty_glyph() -> GlyphBitmap {
        GlyphBitmap {
            data: Vec::new(),
            x_offset: 0,
            y_offset: 0,
            glyph_width: 0,
            glyph_height: 0,
        }
    }

    /// Rasterize `ch` using a specific font entry.
    fn rasterize_with_font(entry: &FontEntry, ch: char, px_size: f32) -> GlyphBitmap {
        let (metrics, bitmap) = entry.font.rasterize(ch, px_size);

        if bitmap.is_empty() || metrics.width == 0 || metrics.height == 0 {
            return Self::empty_glyph();
        }

        let y_offset = entry.ascent_px - (metrics.ymin + metrics.height as i32);
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
        if self.cell_width != 8 || self.cell_height != 16 {
            return None;
        }

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

    fn try_font8x8_basic(&self, ch: char) -> Option<GlyphBitmap> {
        let raw = BASIC_FONTS.get(ch)?;
        Some(self.scale_font8x8_bitmap(raw))
    }

    fn scale_font8x8_bitmap(&self, raw: [u8; 8]) -> GlyphBitmap {
        let width = self.cell_width as usize;
        let height = self.cell_height as usize;
        let mut data = vec![0u8; width * height];

        for py in 0..height {
            let src_y = py * 8 / height.max(1);
            let row_bits = raw[src_y];
            for px in 0..width {
                let src_x = px * 8 / width.max(1);
                if (row_bits >> src_x) & 1 != 0 {
                    data[py * width + px] = 255;
                }
            }
        }

        GlyphBitmap {
            data,
            x_offset: 0,
            y_offset: 0,
            glyph_width: width,
            glyph_height: height,
        }
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

    #[test]
    fn cache_size_is_bounded_and_evicts_old_entries() {
        let mut cache = GlyphCache::new_with_max_entries(8, 16, 2);
        let _ = cache.get('A');
        let _ = cache.get('Ж');

        assert_eq!(cache.cache.len(), 2);
        assert!(!cache.cache.contains_key(&'A'));
        assert!(cache.cache.contains_key(&'Ж'));
    }

    #[test]
    fn fallback_is_used_when_cache_cannot_evict_more_entries() {
        let mut cache = GlyphCache::new_with_max_entries(8, 16, 1);
        let fallback = cache.get(FALLBACK_GLYPH_CHAR).data.clone();
        let glyph = cache.get('Ж').data.clone();
        assert_eq!(glyph, fallback);
        assert_eq!(cache.cache.len(), 1);
        assert!(cache.cache.contains_key(&FALLBACK_GLYPH_CHAR));
    }

    #[test]
    fn add_fallback_font_accepts_valid_data() {
        let mut cache = GlyphCache::new(8, 16);
        // Re-adding the same font as fallback should succeed
        let result = cache.add_fallback_font(FONT_DATA);
        assert!(result);
        assert_eq!(cache.fonts.len(), 2);
    }

    #[test]
    fn add_fallback_font_rejects_invalid_data() {
        let mut cache = GlyphCache::new(8, 16);
        let result = cache.add_fallback_font(b"not a font");
        assert!(!result);
        assert_eq!(cache.fonts.len(), 1);
    }

    #[test]
    fn has_glyph_checks_all_fonts_in_chain() {
        let mut cache = GlyphCache::new(8, 16);
        // Primary font has 'A'
        assert!(cache.has_glyph('A'));
        // Adding same font as fallback doesn't change result
        cache.add_fallback_font(FONT_DATA);
        assert!(cache.has_glyph('A'));
        // Missing glyph still returns false
        assert!(!cache.has_glyph('\u{FFFF}'));
    }

    #[test]
    fn fallback_chain_rasterizes_from_first_matching_font() {
        let mut cache = GlyphCache::new(8, 16);
        cache.add_fallback_font(FONT_DATA);
        // 'A' exists in primary - should get valid glyph
        let glyph = cache.get('A');
        assert!(glyph.glyph_width > 0);
        assert!(!glyph.data.is_empty());
    }

    #[test]
    fn degraded_cache_uses_font8x8_ascii_without_primary_font() {
        let mut cache = GlyphCache {
            fonts: Vec::new(),
            px_size: 16.0,
            cell_width: 8,
            cell_height: 16,
            cache: HashMap::new(),
            eviction_queue: VecDeque::new(),
            max_entries: 16,
        };

        assert!(cache.has_glyph('A'));
        assert!(!cache.has_glyph('\u{FFFF}'));
        let glyph = cache.get('A');
        assert_eq!(glyph.glyph_width, 8);
        assert_eq!(glyph.glyph_height, 16);
        assert!(glyph.data.iter().any(|&pixel| pixel > 0));
    }

    #[test]
    fn degraded_cache_reports_box_drawing_support_without_primary_font() {
        let mut cache = GlyphCache {
            fonts: Vec::new(),
            px_size: 16.0,
            cell_width: 8,
            cell_height: 16,
            cache: HashMap::new(),
            eviction_queue: VecDeque::new(),
            max_entries: 16,
        };

        assert!(cache.has_glyph('─'));
        let glyph = cache.get('─');
        assert_eq!(glyph.glyph_width, 8);
        assert_eq!(glyph.glyph_height, 16);
        assert!(glyph.data.iter().any(|&pixel| pixel > 0));
    }
}
