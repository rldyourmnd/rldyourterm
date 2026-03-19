// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::*;

impl GpuRenderer {
    /// Initializes the GPU backend with wgpu device, surface, pipeline, and glyph atlas.
    /// Must be called from the main thread (winit event loop) before any rendering.
    ///
    /// `cache_dir` — optional directory for persisting the wgpu pipeline cache across runs.
    /// When provided, compiled shader machine code is saved/loaded to eliminate cold-start
    /// compilation spikes (Vulkan only).
    pub fn initialize(
        &mut self,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        cache_dir: Option<&Path>,
    ) -> Result<(), GpuRenderError> {
        let t0 = std::time::Instant::now();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default().with_env());

        let surface = instance
            .create_surface(target)
            .map_err(|_| GpuRenderError::BackendUnavailable)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            // Prioritize low-latency interactive throughput for AI CLI workloads.
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|_| GpuRenderError::BackendUnavailable)?;

        let adapter_info = adapter.get_info();
        debug!(
            backend = ?adapter_info.backend,
            adapter = adapter_info.name,
            elapsed_ms = t0.elapsed().as_millis(),
            "gpu init: adapter acquired"
        );

        let pipeline_cache_supported = adapter.features().contains(wgpu::Features::PIPELINE_CACHE);
        let required_features = if pipeline_cache_supported {
            wgpu::Features::PIPELINE_CACHE
        } else {
            wgpu::Features::empty()
        };

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rldyourterm-gpu"),
            required_features,
            required_limits: wgpu::Limits::downlevel_defaults(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|_| GpuRenderError::DeviceLost)?;

        debug!(
            elapsed_ms = t0.elapsed().as_millis(),
            pipeline_cache_supported, "gpu init: device created"
        );

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .or_else(|| caps.formats.first().copied())
            .ok_or(GpuRenderError::BackendUnavailable)?;

        let max_dim = device.limits().max_texture_dimension_2d;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1).min(max_dim),
            height: height.max(1).min(max_dim),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let mut glyph_cache =
            GlyphCache::new_with_system_fallbacks(CELL_WIDTH as u16, CELL_HEIGHT as u16);
        let atlas::AtlasBuildResult {
            texture: atlas_texture,
            glyph_to_slot,
            slot_to_glyph,
            lru: atlas_lru,
            next_slot: next_atlas_slot,
        } = build_glyph_atlas(&device, &queue, &mut glyph_cache);

        debug!(
            glyph_count = glyph_to_slot.len(),
            elapsed_ms = t0.elapsed().as_millis(),
            "gpu init: atlas built (deferred mode)"
        );

        let atlas_view = atlas_texture.create_view(&Default::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terminal-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terminal.wgsl").into()),
        });

        let pipeline_cache = if pipeline_cache_supported {
            let cache_data = cache_dir.and_then(|dir| {
                let key = wgpu::util::pipeline_cache_key(&adapter_info)?;
                let path = dir.join(key);
                load_pipeline_cache(&path).inspect(|data| {
                    debug!(
                        bytes = data.len(),
                        path = %path.display(),
                        "gpu init: loaded pipeline cache from disk"
                    );
                })
            });

            // SAFETY: `fallback: true` ensures the driver discards corrupt or incompatible
            // cache data and falls back to normal compilation instead of producing undefined
            // behavior. The cache blob is loaded from a user-local directory that only this
            // process writes to, so third-party tampering is not a supported threat model.
            unsafe {
                Some(
                    device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                        label: Some("rldyourterm-pipeline-cache"),
                        data: cache_data.as_deref(),
                        fallback: true,
                    }),
                )
            }
        } else {
            None
        };

        let grid_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid-uniform-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let cell_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cell-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terminal-pipeline-layout"),
            bind_group_layouts: &[&grid_bgl, &atlas_bgl, &cell_bgl],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terminal-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: pipeline_cache.as_ref(),
        });

        debug!(
            elapsed_ms = t0.elapsed().as_millis(),
            "gpu init: pipeline compiled"
        );

        let grid_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid-uniforms"),
            size: std::mem::size_of::<GridUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let grid_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid-bg"),
            layout: &grid_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: grid_uniform_buffer.as_entire_binding(),
            }],
        });

        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas-bg"),
            layout: &atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let initial_capacity = initial_cell_buffer_capacity(config.width, config.height);
        let cell_buf_size = initial_capacity as u64 * std::mem::size_of::<CellInstance>() as u64;
        let cell_buf_usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC;
        let cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances"),
            size: cell_buf_size,
            usage: cell_buf_usage,
            mapped_at_creation: false,
        });
        let cell_bind_group = create_cell_bind_group(&device, &cell_bgl, &cell_buffer);

        let cell_buffer_back = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances-back"),
            size: cell_buf_size,
            usage: cell_buf_usage,
            mapped_at_creation: false,
        });
        let cell_bind_group_back = create_cell_bind_group(&device, &cell_bgl, &cell_buffer_back);

        info!(
            format = ?format,
            backend = ?adapter_info.backend,
            adapter_name = adapter_info.name,
            width,
            height,
            atlas_slots = ATLAS_SLOTS,
            glyph_count = glyph_to_slot.len(),
            pipeline_cache_supported,
            elapsed_ms = t0.elapsed().as_millis(),
            "GPU backend initialized"
        );

        self.backend = Some(GpuBackend {
            device,
            queue,
            surface,
            config,
            pipeline,
            pipeline_cache,
            adapter_info,
            grid_uniform_buffer,
            grid_bind_group,
            atlas_texture,
            atlas_bind_group,
            cell_bind_group_layout: cell_bgl,
            cell_bind_group,
            cell_buffer,
            cell_buffer_back,
            cell_bind_group_back,
            cell_buffer_capacity: initial_capacity,
            cell_instances: vec![
                CellInstance {
                    atlas_and_flags: 0,
                    fg_color: 0,
                    bg_color: 0,
                    underline_color: 0,
                };
                initial_capacity
            ],
            glyph_cache,
            glyph_to_slot,
            slot_to_glyph,
            atlas_lru,
            next_atlas_slot,
            surface_state: SurfaceRuntimeState::default(),
            underutilized_frame_streak: 0,
        });

        Ok(())
    }

    /// Persist the pipeline cache to disk for faster startup on subsequent runs.
    /// Writes atomically (temp file + rename) to avoid corrupt cache files.
    /// No-op if pipeline caching is unsupported or no cache directory was provided.
    pub fn save_pipeline_cache(&self, cache_dir: &Path) {
        let Some(backend) = &self.backend else {
            return;
        };
        let Some(cache) = &backend.pipeline_cache else {
            return;
        };
        save_pipeline_cache(cache_dir, &backend.adapter_info, cache);
    }
}
