// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use rldyourterm_font::{GlyphCache, rasterize_for_atlas};
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
    pub char_to_slot: HashMap<char, u16>,
    pub slot_to_char: Vec<Option<char>>,
    pub slot_last_used: Vec<u64>,
    pub next_slot: u16,
}

pub(crate) fn build_glyph_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    glyph_cache: &mut GlyphCache,
) -> AtlasBuildResult {
    let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
    let mut char_to_slot: HashMap<char, u16> = HashMap::new();
    let mut slot_to_char: Vec<Option<char>> = vec![None; ATLAS_SLOTS];
    let slot_last_used: Vec<u64> = vec![0; ATLAS_SLOTS];

    char_to_slot.insert(' ', 0);
    slot_to_char[0] = Some(' ');
    let mut next_slot: u16 = 1;

    let ranges: &[(u32, u32)] = &[(0x0020, 0x007F), (0x2500, 0x257F), (0x2580, 0x259F)];

    for &(start, end) in ranges {
        for code_point in start..=end {
            if next_slot as usize >= ATLAS_SLOTS {
                break;
            }
            if let Some(ch) = char::from_u32(code_point) {
                if ch == ' ' || char_to_slot.contains_key(&ch) {
                    continue;
                }
                if !glyph_cache.has_glyph(ch) {
                    continue;
                }
                let cell_buf = rasterize_for_atlas(glyph_cache, ch);
                write_glyph_to_atlas(&mut atlas_data, next_slot, &cell_buf);
                char_to_slot.insert(ch, next_slot);
                slot_to_char[next_slot as usize] = Some(ch);
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
        char_to_slot,
        slot_to_char,
        slot_last_used,
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
    ch: char,
    glyph_cache: &mut GlyphCache,
    char_to_slot: &mut HashMap<char, u16>,
    slot_to_char: &mut [Option<char>],
    slot_last_used: &mut [u64],
    frame_counter: u64,
    next_slot: &mut u16,
    atlas_texture: &wgpu::Texture,
    queue: &wgpu::Queue,
) -> u16 {
    if let Some(&slot) = char_to_slot.get(&ch) {
        slot_last_used[slot as usize] = frame_counter;
        return slot;
    }

    let slot = if (*next_slot as usize) < ATLAS_SLOTS {
        let s = *next_slot;
        *next_slot = s + 1;
        s
    } else {
        // LRU eviction: find the slot with the smallest last_used (skip slot 0 = blank)
        let evict_slot = (1..ATLAS_SLOTS)
            .min_by_key(|&i| slot_last_used[i])
            .unwrap_or(1) as u16;

        if let Some(old_ch) = slot_to_char[evict_slot as usize].take() {
            char_to_slot.remove(&old_ch);
        }
        debug!(
            evicted_slot = evict_slot,
            new_char = ?ch,
            "atlas LRU eviction"
        );
        evict_slot
    };

    let cell_buf = rasterize_for_atlas(glyph_cache, ch);
    upload_glyph_to_atlas(queue, atlas_texture, slot, &cell_buf);
    char_to_slot.insert(ch, slot);
    slot_to_char[slot as usize] = Some(ch);
    slot_last_used[slot as usize] = frame_counter;
    slot
}
