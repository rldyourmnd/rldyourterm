// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::{
    ATTR_BLINK, ATTR_BOLD, ATTR_CONTINUATION, ATTR_CURLY_UNDERLINE, ATTR_DASHED_UNDERLINE,
    ATTR_DIM, ATTR_DOTTED_UNDERLINE, ATTR_DOUBLE_UNDERLINE, ATTR_HIDDEN, ATTR_INVERSE, ATTR_ITALIC,
    ATTR_OVERLINE, ATTR_STRIKETHROUGH, ATTR_UNDERLINE, ATTR_WIDE, Attrs,
    CELL_BUFFER_SHRINK_FRAME_STREAK_THRESHOLD, CELL_BUFFER_SHRINK_UTILIZATION_DIVISOR, CELL_HEIGHT,
    CELL_WIDTH, Cell, CellInstance, Color, DEFAULT_FG, GpuBackend, INITIAL_CELL_BUFFER_CAPACITY,
    TerminalState, UnderlineStyle,
};
use crate::atlas::ensure_glyph_in_atlas;
use rldyourterm_font::GlyphKey;
use rldyourterm_services::terminal::CellText;
use tracing::{debug, info};

#[inline]
pub(super) fn pack_cell_flags(slot: u16, attrs: &super::Attrs) -> u32 {
    let mut flags = slot as u32;
    if attrs.bold() {
        flags |= ATTR_BOLD;
    }
    if attrs.italic() {
        flags |= ATTR_ITALIC;
    }
    if attrs.strikethrough() {
        flags |= ATTR_STRIKETHROUGH;
    }
    if attrs.dim() {
        flags |= ATTR_DIM;
    }
    if attrs.inverse() {
        flags |= ATTR_INVERSE;
    }
    if attrs.blink() {
        flags |= ATTR_BLINK;
    }
    if attrs.hidden() {
        flags |= ATTR_HIDDEN;
    }
    match attrs.underline_style() {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => flags |= ATTR_UNDERLINE,
        UnderlineStyle::Double => flags |= ATTR_DOUBLE_UNDERLINE,
        UnderlineStyle::Curly => flags |= ATTR_CURLY_UNDERLINE,
        UnderlineStyle::Dotted => flags |= ATTR_DOTTED_UNDERLINE,
        UnderlineStyle::Dashed => flags |= ATTR_DASHED_UNDERLINE,
    }
    if attrs.overline() {
        flags |= ATTR_OVERLINE;
    }
    flags
}

fn glyph_key_for_cell(cell: &Cell) -> Option<GlyphKey> {
    match cell.text() {
        CellText::Char(ch) if ch == ' ' => None,
        CellText::Char(ch) => Some(GlyphKey::from(ch)),
        CellText::Text(text) if text == " " => None,
        CellText::Text(text) => Some(GlyphKey::from(text)),
    }
}

impl GpuBackend {
    pub(super) fn resize_cell_buffers(&mut self, new_capacity: usize) {
        if new_capacity == self.cell_buffer_capacity {
            return;
        }

        let buf_size = new_capacity as u64 * std::mem::size_of::<CellInstance>() as u64;
        let buf_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;

        let next_front = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances"),
            size: buf_size,
            usage: buf_usage,
            mapped_at_creation: false,
        });
        let next_front_bg =
            create_cell_bind_group(&self.device, &self.cell_bind_group_layout, &next_front);

        let next_back = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances-back"),
            size: buf_size,
            usage: buf_usage,
            mapped_at_creation: false,
        });
        let next_back_bg =
            create_cell_bind_group(&self.device, &self.cell_bind_group_layout, &next_back);

        let old_front = std::mem::replace(&mut self.cell_buffer, next_front);
        let old_back = std::mem::replace(&mut self.cell_buffer_back, next_back);
        self.cell_bind_group = next_front_bg;
        self.cell_bind_group_back = next_back_bg;
        old_front.destroy();
        old_back.destroy();

        self.cell_buffer_capacity = new_capacity;
        self.cell_instances.resize(
            new_capacity,
            CellInstance {
                atlas_and_flags: 0,
                fg_color: 0,
                bg_color: 0,
                underline_color: 0,
            },
        );
        self.underutilized_frame_streak = 0;
    }

    pub(super) fn prepare_all_rows(&mut self, terminal: &TerminalState) {
        let rows = terminal.grid.height() as usize;
        let cols = terminal.grid.width() as usize;
        for row in 0..rows {
            self.write_row_instances(terminal, row, row, cols);
        }
    }

    /// Write grid row `grid_row` into `cell_instances` at display position `display_row`.
    /// When `grid_row == display_row`, behavior is identical to a single-row prepare.
    /// During scrollback viewing, `display_row` shifts grid rows down to make room for
    /// scrollback content at the top of the viewport.
    pub(super) fn write_row_instances(
        &mut self,
        terminal: &TerminalState,
        grid_row: usize,
        display_row: usize,
        cols: usize,
    ) {
        let row_offset = display_row * cols;
        if let Ok(row_cells) = terminal.grid.row_cells(grid_row as u16) {
            for (col, cell) in row_cells.iter().take(cols).enumerate() {
                let attrs = &cell.attrs;
                let (fg, bg) = terminal.resolve_cell_colors(attrs);

                // Continuation cells (width=0) are discarded in the shader;
                // the owning wide cell's 2x quad covers their screen area.
                if cell.width == 0 {
                    self.cell_instances[row_offset + col] = CellInstance {
                        atlas_and_flags: ATTR_CONTINUATION,
                        fg_color: fg,
                        bg_color: bg,
                        underline_color: 0,
                    };
                    continue;
                }

                let slot = glyph_key_for_cell(cell).map_or(0, |glyph_key| {
                    ensure_glyph_in_atlas(
                        glyph_key,
                        &mut self.glyph_cache,
                        &mut self.glyph_to_slot,
                        &mut self.slot_to_glyph,
                        &mut self.slot_last_used,
                        self.frame_counter,
                        &mut self.next_atlas_slot,
                        &self.atlas_texture,
                        &self.queue,
                    )
                });

                let mut packed = pack_cell_flags(slot, attrs);
                if cell.width == 2 {
                    packed |= ATTR_WIDE;
                }

                // Resolve underline decoration color for shader (SGR 58).
                let ul_color = if attrs.has_underline() {
                    if attrs.underline_color == Color::Default {
                        fg
                    } else {
                        terminal.resolve_color(attrs.underline_color, DEFAULT_FG)
                    }
                } else {
                    0
                };

                self.cell_instances[row_offset + col] = CellInstance {
                    atlas_and_flags: packed,
                    fg_color: fg,
                    bg_color: bg,
                    underline_color: ul_color,
                };
            }
        } else {
            let (default_fg, default_bg) = terminal.resolve_cell_colors(&Attrs::default());
            self.cell_instances[row_offset..row_offset + cols].fill(CellInstance {
                atlas_and_flags: 0,
                fg_color: default_fg,
                bg_color: default_bg,
                underline_color: 0,
            });
        }
    }

    /// Write a scrollback cell row into `cell_instances` at `display_row`,
    /// preserving full visual attributes (colors, bold, italic, etc.).
    pub(super) fn write_scrollback_row_instances(
        &mut self,
        terminal: &TerminalState,
        cells: &[Cell],
        display_row: usize,
        cols: usize,
    ) {
        let row_offset = display_row * cols;
        let (default_fg, default_bg) = terminal.resolve_cell_colors(&Attrs::default());
        let blank = CellInstance {
            atlas_and_flags: 0,
            fg_color: default_fg,
            bg_color: default_bg,
            underline_color: 0,
        };
        self.cell_instances[row_offset..row_offset + cols].fill(blank);

        for (col, cell) in cells.iter().take(cols).enumerate() {
            if cell.is_blank_space() && cell.attrs == Attrs::default() {
                continue;
            }
            let slot = glyph_key_for_cell(cell).map_or(0, |glyph_key| {
                ensure_glyph_in_atlas(
                    glyph_key,
                    &mut self.glyph_cache,
                    &mut self.glyph_to_slot,
                    &mut self.slot_to_glyph,
                    &mut self.slot_last_used,
                    self.frame_counter,
                    &mut self.next_atlas_slot,
                    &self.atlas_texture,
                    &self.queue,
                )
            });
            let flags = pack_cell_flags(slot, &cell.attrs);
            let (fg, bg) = terminal.resolve_cell_colors(&cell.attrs);
            let ul = if cell.attrs.has_underline() {
                if cell.attrs.underline_color == Color::Default {
                    fg
                } else {
                    terminal.resolve_color(cell.attrs.underline_color, DEFAULT_FG)
                }
            } else {
                0
            };
            self.cell_instances[row_offset + col] = CellInstance {
                atlas_and_flags: flags,
                fg_color: fg,
                bg_color: bg,
                underline_color: ul,
            };
        }
    }
}

pub(super) fn create_cell_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cell-bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

pub(super) fn next_cell_buffer_capacity(
    current_capacity: usize,
    required_capacity: usize,
) -> usize {
    if required_capacity <= current_capacity {
        return current_capacity;
    }

    let mut capacity = current_capacity.max(1);
    while capacity < required_capacity {
        let doubled = capacity.saturating_mul(2);
        if doubled == capacity {
            return required_capacity;
        }
        capacity = doubled;
    }

    capacity
}

pub(super) fn shrink_cell_buffer_capacity(
    current_capacity: usize,
    required_capacity: usize,
    initial_capacity: usize,
) -> Option<usize> {
    if current_capacity <= initial_capacity {
        return None;
    }

    let threshold = (current_capacity / CELL_BUFFER_SHRINK_UTILIZATION_DIVISOR).max(1);
    if required_capacity > threshold {
        return None;
    }

    let target = next_cell_buffer_capacity(initial_capacity, required_capacity.max(1));
    (target < current_capacity).then_some(target)
}

pub(super) fn initial_cell_buffer_capacity(width: u32, height: u32) -> usize {
    let cols = ((width as usize) / CELL_WIDTH).max(1);
    let rows = ((height as usize) / CELL_HEIGHT).max(1);
    let required = cols.saturating_mul(rows);
    next_cell_buffer_capacity(INITIAL_CELL_BUFFER_CAPACITY, required)
}

pub(super) fn reconcile_cell_buffer_capacity(backend: &mut GpuBackend, cell_count: usize) -> bool {
    let mut force_full_upload = false;
    let current_capacity = backend.cell_buffer_capacity;
    let initial_capacity =
        initial_cell_buffer_capacity(backend.config.width, backend.config.height);
    if let Some(shrink_target) =
        shrink_cell_buffer_capacity(current_capacity, cell_count, initial_capacity)
    {
        backend.underutilized_frame_streak = backend.underutilized_frame_streak.saturating_add(1);
        if backend.underutilized_frame_streak >= CELL_BUFFER_SHRINK_FRAME_STREAK_THRESHOLD {
            info!(
                from_capacity = current_capacity,
                to_capacity = shrink_target,
                required_capacity = cell_count,
                sustained_frames = backend.underutilized_frame_streak,
                "gpu cell buffers shrunk after sustained underutilization"
            );
            backend.resize_cell_buffers(shrink_target);
            force_full_upload = true;
        }
    } else {
        backend.underutilized_frame_streak = 0;
    }

    let next_capacity = next_cell_buffer_capacity(backend.cell_buffer_capacity, cell_count);
    if next_capacity != backend.cell_buffer_capacity {
        debug!(
            from_capacity = backend.cell_buffer_capacity,
            to_capacity = next_capacity,
            required_capacity = cell_count,
            "gpu cell buffers grown to satisfy viewport demand"
        );
        backend.resize_cell_buffers(next_capacity);
        force_full_upload = true;
    }

    force_full_upload
}

pub(super) fn prepare_and_upload_dirty_rows(
    backend: &mut GpuBackend,
    terminal: &TerminalState,
    dirty_rows: &[bool],
    grid_cols: usize,
    row_byte_size: usize,
) {
    let flush = |backend: &GpuBackend, start: usize, end: usize| {
        let byte_offset = start as u64 * row_byte_size as u64;
        let instance_start = start * grid_cols;
        let instance_end = end * grid_cols;
        backend.queue.write_buffer(
            &backend.cell_buffer,
            byte_offset,
            bytemuck::cast_slice(&backend.cell_instances[instance_start..instance_end]),
        );
    };

    let mut range_start: Option<usize> = None;

    for (row, &dirty) in dirty_rows.iter().enumerate() {
        if dirty {
            if range_start.is_none() {
                range_start = Some(row);
            }
            backend.write_row_instances(terminal, row, row, grid_cols);
            continue;
        }

        if let Some(start) = range_start.take() {
            flush(backend, start, row);
        }
    }

    if let Some(start) = range_start {
        flush(backend, start, dirty_rows.len());
    }
}
