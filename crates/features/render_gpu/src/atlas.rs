// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use rldyourterm_font::{GlyphCache, GlyphKey, rasterize_for_atlas};
use std::collections::HashMap;
use tracing::debug;

use crate::{CELL_HEIGHT, CELL_WIDTH};

pub(crate) const ATLAS_GLYPH_WIDTH: u32 = CELL_WIDTH as u32; // 8
pub(crate) const ATLAS_GLYPH_HEIGHT: u32 = CELL_HEIGHT as u32; // 16
pub(crate) const ATLAS_SIZE: u32 = 1024;
pub(crate) const ATLAS_GLYPH_COLS: u32 = ATLAS_SIZE / ATLAS_GLYPH_WIDTH; // 128
pub(crate) const ATLAS_GLYPH_ROWS: u32 = ATLAS_SIZE / ATLAS_GLYPH_HEIGHT; // 64
pub(crate) const ATLAS_SLOTS: usize = (ATLAS_GLYPH_COLS * ATLAS_GLYPH_ROWS) as usize; // 8192

pub(crate) fn write_glyph_to_atlas(atlas_data: &mut [u8], slot: u16, cell_buf: &[u8]) {
    let slot = slot as usize;
    let cw = ATLAS_GLYPH_WIDTH as usize;
    let ch = ATLAS_GLYPH_HEIGHT as usize;
    let cols = ATLAS_GLYPH_COLS as usize;
    let slot_x = (slot % cols) * cw;
    let slot_y = (slot / cols) * ch;

    for gy in 0..ch {
        for gx in 0..cw {
            let src_idx = gy * cw + gx;
            if src_idx >= cell_buf.len() {
                continue;
            }
            let coverage = cell_buf[src_idx];
            if coverage == 0 {
                continue;
            }
            let px = slot_x + gx;
            let py = slot_y + gy;
            atlas_data[py * ATLAS_SIZE as usize + px] = coverage;
        }
    }
}

/// Result of building the GPU glyph atlas texture.
pub(crate) struct AtlasBuildResult {
    pub texture: wgpu::Texture,
    pub glyph_to_slot: HashMap<GlyphKey, u16>,
    pub slot_to_glyph: Vec<Option<GlyphKey>>,
    pub lru: AtlasLru,
    pub next_slot: u16,
}

pub(crate) struct AtlasLru {
    head: Option<u16>,
    tail: Option<u16>,
    prev: Vec<Option<u16>>,
    next: Vec<Option<u16>>,
    linked: Vec<bool>,
}

impl AtlasLru {
    pub(crate) fn new(slot_count: usize) -> Self {
        Self {
            head: None,
            tail: None,
            prev: vec![None; slot_count],
            next: vec![None; slot_count],
            linked: vec![false; slot_count],
        }
    }

    pub(crate) fn touch(&mut self, slot: u16) {
        if slot == 0 {
            return;
        }

        if self.linked[slot as usize] {
            self.unlink(slot);
        }
        self.link_front(slot);
    }

    pub(crate) fn evict_lru(&mut self) -> Option<u16> {
        let slot = self.tail?;
        self.unlink(slot);
        Some(slot)
    }

    fn link_front(&mut self, slot: u16) {
        let slot_index = slot as usize;
        let old_head = self.head;

        self.prev[slot_index] = None;
        self.next[slot_index] = old_head;
        self.linked[slot_index] = true;

        if let Some(old_head) = old_head {
            self.prev[old_head as usize] = Some(slot);
        } else {
            self.tail = Some(slot);
        }

        self.head = Some(slot);
    }

    fn unlink(&mut self, slot: u16) {
        let slot_index = slot as usize;
        let prev = self.prev[slot_index];
        let next = self.next[slot_index];

        if let Some(prev) = prev {
            self.next[prev as usize] = next;
        } else {
            self.head = next;
        }

        if let Some(next) = next {
            self.prev[next as usize] = prev;
        } else {
            self.tail = prev;
        }

        self.prev[slot_index] = None;
        self.next[slot_index] = None;
        self.linked[slot_index] = false;
    }
}

pub(crate) fn build_glyph_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    glyph_cache: &mut GlyphCache,
) -> AtlasBuildResult {
    let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
    let mut glyph_to_slot: HashMap<GlyphKey, u16> = HashMap::new();
    let mut slot_to_glyph: Vec<Option<GlyphKey>> = vec![None; ATLAS_SLOTS];
    let mut lru = AtlasLru::new(ATLAS_SLOTS);

    glyph_to_slot.insert(GlyphKey::from(' '), 0);
    slot_to_glyph[0] = Some(GlyphKey::from(' '));
    let mut next_slot: u16 = 1;

    let ranges: &[(u32, u32)] = &[(0x0020, 0x007F), (0x2500, 0x257F), (0x2580, 0x259F)];

    for &(start, end) in ranges {
        for code_point in start..=end {
            if next_slot as usize >= ATLAS_SLOTS {
                break;
            }
            if let Some(ch) = char::from_u32(code_point) {
                let glyph_key = GlyphKey::from(ch);
                if glyph_key.is_blank_space() || glyph_to_slot.contains_key(&glyph_key) {
                    continue;
                }
                if !glyph_cache.has_glyph(ch) {
                    continue;
                }
                let cell_buf = rasterize_for_atlas(glyph_cache, glyph_key.clone());
                write_glyph_to_atlas(&mut atlas_data, next_slot, &cell_buf);
                glyph_to_slot.insert(glyph_key.clone(), next_slot);
                slot_to_glyph[next_slot as usize] = Some(glyph_key);
                lru.touch(next_slot);
                next_slot += 1;
            }
        }
    }

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glyph-atlas"),
        size: wgpu::Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(ATLAS_SIZE),
            rows_per_image: Some(ATLAS_SIZE),
        },
        wgpu::Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
    );

    AtlasBuildResult {
        texture,
        glyph_to_slot,
        slot_to_glyph,
        lru,
        next_slot,
    }
}

pub(crate) fn upload_glyph_to_atlas(
    queue: &wgpu::Queue,
    atlas_texture: &wgpu::Texture,
    slot: u16,
    cell_buf: &[u8],
) {
    let cw = ATLAS_GLYPH_WIDTH;
    let ch = ATLAS_GLYPH_HEIGHT;
    let cols = ATLAS_GLYPH_COLS;
    let slot_x = (slot as u32 % cols) * cw;
    let slot_y = (slot as u32 / cols) * ch;

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: slot_x,
                y: slot_y,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        cell_buf,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(cw),
            rows_per_image: Some(ch),
        },
        wgpu::Extent3d {
            width: cw,
            height: ch,
            depth_or_array_layers: 1,
        },
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ensure_glyph_in_atlas(
    glyph_key: GlyphKey,
    glyph_cache: &mut GlyphCache,
    glyph_to_slot: &mut HashMap<GlyphKey, u16>,
    slot_to_glyph: &mut [Option<GlyphKey>],
    lru: &mut AtlasLru,
    next_slot: &mut u16,
    atlas_texture: &wgpu::Texture,
    queue: &wgpu::Queue,
) -> u16 {
    if let Some(&slot) = glyph_to_slot.get(&glyph_key) {
        lru.touch(slot);
        return slot;
    }

    let slot = if (*next_slot as usize) < ATLAS_SLOTS {
        let s = *next_slot;
        *next_slot = s + 1;
        s
    } else {
        let evict_slot = lru.evict_lru().unwrap_or(1);

        if let Some(old_glyph_key) = slot_to_glyph[evict_slot as usize].take() {
            glyph_to_slot.remove(&old_glyph_key);
        }
        debug!(
            evicted_slot = evict_slot,
            new_glyph = ?glyph_key,
            "atlas LRU eviction"
        );
        evict_slot
    };

    let cell_buf = rasterize_for_atlas(glyph_cache, glyph_key.clone());
    upload_glyph_to_atlas(queue, atlas_texture, slot, &cell_buf);
    glyph_to_slot.insert(glyph_key.clone(), slot);
    slot_to_glyph[slot as usize] = Some(glyph_key);
    lru.touch(slot);
    slot
}

#[cfg(test)]
mod tests {
    use super::AtlasLru;

    fn lru_order(lru: &AtlasLru) -> Vec<u16> {
        let mut slots = Vec::new();
        let mut cursor = lru.head;
        while let Some(slot) = cursor {
            slots.push(slot);
            cursor = lru.next[slot as usize];
        }
        slots
    }

    #[test]
    fn touch_moves_slot_to_mru_head() {
        let mut lru = AtlasLru::new(8);
        lru.touch(1);
        lru.touch(2);
        lru.touch(3);

        lru.touch(1);

        assert_eq!(lru_order(&lru), vec![1, 3, 2]);
        assert_eq!(lru.evict_lru(), Some(2));
    }

    #[test]
    fn evict_lru_returns_oldest_non_blank_slot() {
        let mut lru = AtlasLru::new(8);
        lru.touch(1);
        lru.touch(2);
        lru.touch(3);

        assert_eq!(lru.evict_lru(), Some(1));
        assert_eq!(lru_order(&lru), vec![3, 2]);
    }

    #[test]
    fn reserved_blank_slot_is_never_linked_or_evicted() {
        let mut lru = AtlasLru::new(4);
        lru.touch(0);
        lru.touch(1);

        assert_eq!(lru_order(&lru), vec![1]);
        assert_eq!(lru.evict_lru(), Some(1));
        assert_eq!(lru.evict_lru(), None);
    }
}
