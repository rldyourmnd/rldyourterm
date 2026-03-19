// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;

impl GpuRenderer {
    /// Resize the GPU surface. Must be called when the window is resized.
    /// Zero-dimension requests are ignored (wgpu panics on zero-size configure).
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if let Some(backend) = self.backend.as_mut() {
            let max_dim = backend.device.limits().max_texture_dimension_2d;
            let clamped_width = width.min(max_dim.max(1));
            let clamped_height = height.min(max_dim.max(1));
            if backend.config.width == clamped_width && backend.config.height == clamped_height {
                return;
            }
            if let Err(err) = update_surface_extent(&mut backend.config, width, height, max_dim) {
                debug!(%err, width, height, max_dim, "surface extent rejected after zero-dimension guard");
                return;
            }
            backend.surface.configure(&backend.device, &backend.config);
            self.policy
                .on_reconfigure_success(&mut backend.surface_state);
        }
    }

    /// Renders the terminal state to the GPU surface.
    ///
    /// `dirty_rows` indicates which grid rows changed since last render.
    /// `scroll_count` is lines scrolled since last frame (for GPU DMA scroll optimization).
    /// `viewport_offset` is the number of scrollback lines to show at the top of the viewport
    /// (0 = live view, >0 = user scrolled back into history).
    /// Only dirty rows are re-prepared on the CPU and uploaded to the GPU buffer.
    /// The GPU buffer retains previous frame data for clean rows.
    #[allow(clippy::too_many_arguments)]
    pub fn render_frame(
        &mut self,
        terminal: &TerminalState,
        dirty_rows: &[bool],
        scroll_count: usize,
        blink_visible: bool,
        viewport_offset: usize,
        selection_start: u32,
        selection_end: u32,
    ) -> Result<(), GpuRenderError> {
        let backend = self
            .backend
            .as_mut()
            .ok_or(GpuRenderError::BackendUnavailable)?;

        backend.frame_counter = backend.frame_counter.wrapping_add(1);

        let grid_cols = terminal.grid.width() as usize;
        let grid_rows = terminal.grid.height() as usize;
        let cell_count = grid_cols * grid_rows;

        if cell_count == 0 {
            return Ok(());
        }

        // When viewing scrollback, cursor is hidden and all rows need full recompositing.
        let viewing_scrollback = viewport_offset > 0;
        let effective_offset = viewport_offset.min(terminal.scrollback.len());
        let sb_rows = if viewing_scrollback {
            effective_offset.min(grid_rows)
        } else {
            0
        };

        let cursor_row = terminal.cursor.row as u32;
        let cursor_col = terminal.cursor.col as u32;
        let cursor_visible = if viewing_scrollback {
            0
        } else {
            u32::from(terminal.cursor.visible)
        };
        let content_dirty = if viewing_scrollback {
            true
        } else {
            dirty_rows.iter().any(|&d| d)
        };
        let cursor_changed = cursor_row != self.last_cursor_row
            || cursor_col != self.last_cursor_col
            || cursor_visible != self.last_cursor_visible;

        if !content_dirty && !cursor_changed {
            return Ok(());
        }

        self.last_cursor_row = cursor_row;
        self.last_cursor_col = cursor_col;
        self.last_cursor_visible = cursor_visible;

        let force_full_upload = reconcile_cell_buffer_capacity(backend, cell_count);
        let row_byte_size = grid_cols * std::mem::size_of::<CellInstance>();
        let mut scroll_dma: Option<(u64, u64)> = None;

        if viewing_scrollback {
            // Scrollback view: compose scrollback lines at top, grid rows below.
            for display_row in 0..sb_rows {
                let sb_line_idx = terminal.scrollback.len() - effective_offset + display_row;
                if let Some(line) = terminal.scrollback.get(sb_line_idx) {
                    backend.write_scrollback_row_instances(terminal, line, display_row, grid_cols);
                } else {
                    let default_fg = color_to_u32(Color::Default, DEFAULT_FG);
                    let default_bg = color_to_u32(Color::Default, DEFAULT_BG);
                    let row_offset = display_row * grid_cols;
                    backend.cell_instances[row_offset..row_offset + grid_cols].fill(CellInstance {
                        atlas_and_flags: 0,
                        fg_color: default_fg,
                        bg_color: default_bg,
                        underline_color: 0,
                    });
                }
            }
            for grid_row in 0..(grid_rows - sb_rows) {
                let display_row = sb_rows + grid_row;
                backend.write_row_instances(terminal, grid_row, display_row, grid_cols);
            }
            backend.queue.write_buffer(
                &backend.cell_buffer,
                0,
                bytemuck::cast_slice(&backend.cell_instances[..cell_count]),
            );
        } else if force_full_upload {
            backend.prepare_all_rows(terminal);
            backend.queue.write_buffer(
                &backend.cell_buffer,
                0,
                bytemuck::cast_slice(&backend.cell_instances[..cell_count]),
            );
        }

        if !viewing_scrollback && !force_full_upload && scroll_count > 0 && scroll_count < grid_rows
        {
            let copy_rows = grid_rows - scroll_count;
            let first_new_row = grid_rows - scroll_count;
            let src_start = scroll_count * grid_cols;
            let src_end = grid_rows * grid_cols;
            backend.cell_instances.copy_within(src_start..src_end, 0);

            for row in first_new_row..grid_rows {
                backend.write_row_instances(terminal, row, row, grid_cols);
            }

            let upload_offset = first_new_row as u64 * row_byte_size as u64;
            let instance_start = first_new_row * grid_cols;
            let instance_end = grid_rows * grid_cols;
            backend.queue.write_buffer(
                &backend.cell_buffer_back,
                upload_offset,
                bytemuck::cast_slice(&backend.cell_instances[instance_start..instance_end]),
            );

            let src_offset = scroll_count as u64 * row_byte_size as u64;
            let copy_size = copy_rows as u64 * row_byte_size as u64;
            scroll_dma = Some((src_offset, copy_size));

            std::mem::swap(&mut backend.cell_buffer, &mut backend.cell_buffer_back);
            std::mem::swap(
                &mut backend.cell_bind_group,
                &mut backend.cell_bind_group_back,
            );
        } else if !viewing_scrollback && !force_full_upload {
            prepare_and_upload_dirty_rows(backend, terminal, dirty_rows, grid_cols, row_byte_size);
        }

        let uniforms = GridUniforms {
            cell_width: CELL_WIDTH as f32,
            cell_height: CELL_HEIGHT as f32,
            grid_cols: grid_cols as u32,
            grid_rows: grid_rows as u32,
            viewport_width: backend.config.width as f32,
            viewport_height: backend.config.height as f32,
            atlas_cols: ATLAS_GLYPH_COLS,
            atlas_rows: ATLAS_GLYPH_ROWS,
            cursor_row,
            cursor_col,
            cursor_visible,
            selection_start,
            selection_end,
            blink_visible: u32::from(blink_visible),
            cursor_shape: terminal.cursor_shape() as u32,
            _pad: 0,
        };
        backend.queue.write_buffer(
            &backend.grid_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        let frame = match backend.surface.get_current_texture() {
            Ok(frame) => {
                self.policy.on_acquire_success(&mut backend.surface_state);
                frame
            }
            Err(error) => {
                let decision = self
                    .policy
                    .on_surface_acquire_error(&mut backend.surface_state, error);
                match decision.action {
                    SurfaceRecoveryAction::RetryAcquire => {
                        return Err(GpuRenderError::SurfaceAcquire(decision.source));
                    }
                    SurfaceRecoveryAction::ReconfigureSurface => {
                        backend.surface.configure(&backend.device, &backend.config);
                        match backend.surface.get_current_texture() {
                            Ok(frame) => {
                                self.policy
                                    .on_reconfigure_success(&mut backend.surface_state);
                                frame
                            }
                            Err(retry_error) => {
                                return Err(GpuRenderError::SurfaceAcquire(retry_error));
                            }
                        }
                    }
                    SurfaceRecoveryAction::DegradeToCpu => {
                        return Err(GpuRenderError::SurfaceAcquire(decision.source));
                    }
                }
            }
        };
        let view = frame.texture.create_view(&Default::default());

        let bg_r = DEFAULT_BG.0 as f64 / 255.0;
        let bg_g = DEFAULT_BG.1 as f64 / 255.0;
        let bg_b = DEFAULT_BG.2 as f64 / 255.0;

        let mut encoder = backend
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terminal-encoder"),
            });

        if let Some((src_offset, copy_size)) = scroll_dma {
            encoder.copy_buffer_to_buffer(
                &backend.cell_buffer_back,
                src_offset,
                &backend.cell_buffer,
                0,
                copy_size,
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terminal-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg_r,
                            g: bg_g,
                            b: bg_b,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&backend.pipeline);
            pass.set_bind_group(0, &backend.grid_bind_group, &[]);
            pass.set_bind_group(1, &backend.atlas_bind_group, &[]);
            pass.set_bind_group(2, &backend.cell_bind_group, &[]);
            pass.draw(0..6, 0..cell_count as u32);
        }

        backend.queue.submit(Some(encoder.finish()));
        frame.present();

        Ok(())
    }
}
