use super::{
    ATTR_BOLD, ATTR_DIM, ATTR_INVERSE, ATTR_ITALIC, ATTR_STRIKETHROUGH, ATTR_UNDERLINE,
    CELL_BUFFER_SHRINK_FRAME_STREAK_THRESHOLD, CELL_BUFFER_SHRINK_UTILIZATION_DIVISOR, CELL_HEIGHT,
    CELL_WIDTH, CellInstance, Color, DEFAULT_BG, DEFAULT_FG, GpuBackend,
    INITIAL_CELL_BUFFER_CAPACITY, TerminalState, color_to_u32,
};
use crate::atlas::ensure_glyph_in_atlas;
use tracing::{debug, info};

#[inline]
pub(super) fn pack_cell_flags(slot: u16, attrs: &super::Attrs) -> u32 {
    let mut flags = slot as u32;
    if attrs.bold {
        flags |= ATTR_BOLD;
    }
    if attrs.italic {
        flags |= ATTR_ITALIC;
    }
    if attrs.underline {
        flags |= ATTR_UNDERLINE;
    }
    if attrs.strikethrough {
        flags |= ATTR_STRIKETHROUGH;
    }
    if attrs.dim {
        flags |= ATTR_DIM;
    }
    if attrs.inverse {
        flags |= ATTR_INVERSE;
    }
    flags
}

impl GpuBackend {
    pub(super) fn resize_cell_buffers(&mut self, new_capacity: usize) {
        if new_capacity == self.cell_buffer_capacity {
            return;
        }

        let buf_size = (new_capacity * std::mem::size_of::<CellInstance>()) as u64;
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
                _pad: 0,
            },
        );
        self.underutilized_frame_streak = 0;
    }

    pub(super) fn prepare_all_rows(&mut self, terminal: &TerminalState) {
        let rows = terminal.grid.height() as usize;
        let cols = terminal.grid.width() as usize;
        for row in 0..rows {
            self.write_row_instances(terminal, row, cols);
        }
    }

    pub(super) fn write_row_instances(
        &mut self,
        terminal: &TerminalState,
        row: usize,
        cols: usize,
    ) {
        let row_offset = row * cols;
        if let Ok(row_cells) = terminal.grid.row_cells(row as u16) {
            for (col, cell) in row_cells.iter().take(cols).enumerate() {
                let attrs = &cell.attrs;
                let fg = color_to_u32(attrs.fg, DEFAULT_FG);
                let bg = color_to_u32(attrs.bg, DEFAULT_BG);

                let slot = if cell.ch == ' ' {
                    0u16
                } else {
                    ensure_glyph_in_atlas(
                        cell.ch,
                        &mut self.glyph_cache,
                        &mut self.char_to_slot,
                        &mut self.next_atlas_slot,
                        &mut self.atlas_full_warned,
                        &self.atlas_texture,
                        &self.queue,
                    )
                };

                self.cell_instances[row_offset + col] = CellInstance {
                    atlas_and_flags: pack_cell_flags(slot, attrs),
                    fg_color: fg,
                    bg_color: bg,
                    _pad: 0,
                };
            }
        } else {
            let default_fg = color_to_u32(Color::Default, DEFAULT_FG);
            let default_bg = color_to_u32(Color::Default, DEFAULT_BG);
            for col in 0..cols {
                self.cell_instances[row_offset + col] = CellInstance {
                    atlas_and_flags: 0,
                    fg_color: default_fg,
                    bg_color: default_bg,
                    _pad: 0,
                };
            }
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
        let byte_offset = (start * row_byte_size) as u64;
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
            backend.write_row_instances(terminal, row, grid_cols);
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
