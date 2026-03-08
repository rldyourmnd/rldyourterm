mod atlas;
mod cell_data;
mod pipeline_cache;
mod surface;

use bytemuck::{Pod, Zeroable};
use rldyourterm_font::GlyphCache;
use rldyourterm_services::render_mode::GpuFailureKind;
#[cfg(test)]
use rldyourterm_services::terminal::ANSI_PALETTE;
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Color, TerminalState, color_to_u32,
};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::path::Path;
use tracing::{debug, info};

use atlas::{ATLAS_GLYPH_COLS, ATLAS_GLYPH_ROWS, ATLAS_SLOTS, build_glyph_atlas};
#[cfg(test)]
use atlas::{ATLAS_GLYPH_HEIGHT, ATLAS_GLYPH_WIDTH, ATLAS_SIZE, write_glyph_to_atlas};
use cell_data::{
    create_cell_bind_group, initial_cell_buffer_capacity, prepare_and_upload_dirty_rows,
    reconcile_cell_buffer_capacity,
};
#[cfg(test)]
use cell_data::{next_cell_buffer_capacity, pack_cell_flags, shrink_cell_buffer_capacity};
#[cfg(test)]
use pipeline_cache::{
    MAX_PIPELINE_CACHE_BYTES, PipelineCacheReadError, read_pipeline_cache_with_limit,
};
use pipeline_cache::{load_pipeline_cache, save_pipeline_cache};

pub use surface::{
    DEFAULT_SURFACE_RECONFIGURE_RETRY_BUDGET, DEFAULT_SURFACE_RETRY_BUDGET,
    SurfaceConfigurationDecision, SurfaceConfigurationError, SurfaceErrorCategory,
    SurfaceErrorDecision, SurfaceRecoveryAction, SurfaceRecoveryPolicy, SurfaceRuntimeState,
    classify_surface_error, update_frame_latency_hint, update_surface_extent,
    validate_surface_configuration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuRenderError {
    DeviceLost,
    SurfaceAcquire(wgpu::SurfaceError),
    SubmitFailed,
    BackendUnavailable,
}

impl GpuRenderError {
    #[must_use]
    pub const fn failure_kind(&self) -> GpuFailureKind {
        match self {
            GpuRenderError::DeviceLost => GpuFailureKind::DeviceLost,
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Outdated)
            | GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Lost) => {
                GpuFailureKind::SwapchainOutOfDate
            }
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::OutOfMemory) => {
                GpuFailureKind::OutOfMemory
            }
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Timeout)
            | GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Other) => {
                GpuFailureKind::SurfaceError
            }
            GpuRenderError::SubmitFailed => GpuFailureKind::SubmitError,
            GpuRenderError::BackendUnavailable => GpuFailureKind::DeviceLost,
        }
    }
}

impl fmt::Display for GpuRenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceLost => f.write_str("GPU device lost during frame rendering"),
            Self::SurfaceAcquire(error) => {
                write!(
                    f,
                    "GPU surface acquire failed during frame rendering: {error:?}"
                )
            }
            Self::SubmitFailed => f.write_str("GPU command submission failed"),
            Self::BackendUnavailable => {
                f.write_str("GPU backend is unavailable in this renderer path")
            }
        }
    }
}

impl Error for GpuRenderError {}

// Default terminal colors (must match gui_runtime)
const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);

// Attribute flag bits packed in upper bits of CellInstance::atlas_and_flags.
// Lower 16 bits = atlas slot index (supports 65536 glyphs).
const ATTR_BOLD: u32 = 1 << 16;
const ATTR_ITALIC: u32 = 1 << 17;
const ATTR_UNDERLINE: u32 = 1 << 18;
const ATTR_STRIKETHROUGH: u32 = 1 << 19;
const ATTR_DIM: u32 = 1 << 20;
const ATTR_INVERSE: u32 = 1 << 21;

// Sentinel value indicating no active selection (u32::MAX).
const SELECTION_NONE: u32 = u32::MAX;
const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const INITIAL_CELL_BUFFER_CAPACITY: usize = 120 * 32;
const CELL_BUFFER_SHRINK_UTILIZATION_DIVISOR: usize = 4;
const CELL_BUFFER_SHRINK_FRAME_STREAK_THRESHOLD: u16 = 120;

/// Pack atlas slot index and text attribute flags into a single u32.
/// Lower 16 bits = atlas slot, upper bits = bold/italic/underline/strikethrough/dim/inverse.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GridUniforms {
    cell_width: f32,
    cell_height: f32,
    grid_cols: u32,
    grid_rows: u32,
    viewport_width: f32,
    viewport_height: f32,
    atlas_cols: u32,
    atlas_rows: u32,
    cursor_row: u32,
    cursor_col: u32,
    cursor_visible: u32,
    selection_start: u32,
    selection_end: u32,
    blink_visible: u32,
    _pad: [u32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CellInstance {
    pub atlas_and_flags: u32,
    pub fg_color: u32,
    pub bg_color: u32,
    pub _pad: u32,
}

struct GpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    pipeline_cache: Option<wgpu::PipelineCache>,
    adapter_info: wgpu::AdapterInfo,
    grid_uniform_buffer: wgpu::Buffer,
    grid_bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    cell_bind_group_layout: wgpu::BindGroupLayout,
    cell_bind_group: wgpu::BindGroup,
    cell_buffer: wgpu::Buffer,
    cell_buffer_back: wgpu::Buffer,
    cell_bind_group_back: wgpu::BindGroup,
    cell_buffer_capacity: usize,
    cell_instances: Vec<CellInstance>,
    glyph_cache: GlyphCache,
    char_to_slot: HashMap<char, u16>,
    next_atlas_slot: u16,
    atlas_full_warned: bool,
    surface_state: SurfaceRuntimeState,
    underutilized_frame_streak: u16,
}

impl GpuBackend {
    fn destroy_resources(&self) {
        self.grid_uniform_buffer.destroy();
        self.cell_buffer.destroy();
        self.cell_buffer_back.destroy();
        self.atlas_texture.destroy();
    }
}

pub struct GpuRenderer {
    policy: SurfaceRecoveryPolicy,
    backend: Option<GpuBackend>,
    last_cursor_row: u32,
    last_cursor_col: u32,
    last_cursor_visible: u32,
}

impl fmt::Debug for GpuRenderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuRenderer")
            .field("policy", &self.policy)
            .field("backend", &self.backend.as_ref().map(|_| "initialized"))
            .finish()
    }
}

impl GpuRenderer {
    #[must_use]
    pub fn new(policy: SurfaceRecoveryPolicy) -> Self {
        Self {
            policy,
            backend: None,
            last_cursor_row: u32::MAX,
            last_cursor_col: u32::MAX,
            last_cursor_visible: u32::MAX,
        }
    }

    #[must_use]
    pub fn recovery_policy(&self) -> SurfaceRecoveryPolicy {
        self.policy
    }

    pub fn is_initialized(&self) -> bool {
        self.backend.is_some()
    }

    /// Releases all GPU-side resources held by the renderer backend.
    ///
    /// Used when runtime deterministically falls back to CPU to promptly return
    /// VRAM/system allocations instead of keeping an idle GPU backend alive.
    pub fn release_backend(&mut self) {
        if let Some(backend) = self.backend.take() {
            backend.destroy_resources();
            info!("GPU backend released");
        }
        self.last_cursor_row = u32::MAX;
        self.last_cursor_col = u32::MAX;
        self.last_cursor_visible = u32::MAX;
    }

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

        // Request PIPELINE_CACHE feature when the adapter supports it (Vulkan).
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
            // Prefer faster memory allocation strategy; we intentionally trade RAM
            // footprint for responsiveness in the primary GPU runtime path.
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
            // Hint one frame in flight to minimize input-to-present latency.
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        // Build glyph atlas with essential ranges only (ASCII + Box Drawing + Block Elements).
        // Non-essential glyphs (Cyrillic, Latin Extended, Greek, etc.) are loaded on-demand
        // via ensure_glyph_in_atlas when first encountered in terminal output.
        let mut glyph_cache = GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16);
        let (atlas_texture, char_to_slot, next_atlas_slot) =
            build_glyph_atlas(&device, &queue, &mut glyph_cache);

        debug!(
            glyph_count = char_to_slot.len(),
            elapsed_ms = t0.elapsed().as_millis(),
            "gpu init: atlas built (deferred mode)"
        );

        let atlas_view = atlas_texture.create_view(&Default::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terminal-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("terminal.wgsl").into()),
        });

        // Pipeline cache: load from disk if available, create new otherwise.
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

            // SAFETY: data (if Some) was previously obtained from PipelineCache::get_data
            // for the same adapter (keyed by pipeline_cache_key). fallback: true ensures
            // a fresh empty cache is created if the data is corrupt or incompatible.
            let cache = unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("rldyourterm-pipeline-cache"),
                    data: cache_data.as_deref(),
                    fallback: true,
                })
            };
            Some(cache)
        } else {
            None
        };

        // Bind group layouts
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

        // Uniform buffer
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

        // Pre-size instance buffers to the initial viewport to avoid first-frame realloc spikes.
        let initial_capacity = initial_cell_buffer_capacity(config.width, config.height);
        let cell_buf_size = (initial_capacity * std::mem::size_of::<CellInstance>()) as u64;
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
            glyph_count = char_to_slot.len(),
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
                    _pad: 0
                };
                initial_capacity
            ],
            glyph_cache,
            char_to_slot,
            next_atlas_slot,
            atlas_full_warned: false,
            surface_state: SurfaceRuntimeState::default(),
            underutilized_frame_streak: 0,
        });

        Ok(())
    }

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
            let _ = update_surface_extent(&mut backend.config, width, height, max_dim);
            backend.surface.configure(&backend.device, &backend.config);
            // Reconfigure resets the surface — clear stale failure counters.
            self.policy
                .on_reconfigure_success(&mut backend.surface_state);
        }
    }

    /// Renders the terminal state to the GPU surface.
    ///
    /// `dirty_rows` indicates which grid rows changed since last render.
    /// `scroll_count` is lines scrolled since last frame (for GPU DMA scroll optimization).
    /// Only dirty rows are re-prepared on the CPU and uploaded to the GPU buffer.
    /// The GPU buffer retains previous frame data for clean rows.
    pub fn render_frame(
        &mut self,
        terminal: &TerminalState,
        dirty_rows: &[bool],
        scroll_count: usize,
    ) -> Result<(), GpuRenderError> {
        let backend = self
            .backend
            .as_mut()
            .ok_or(GpuRenderError::BackendUnavailable)?;

        let grid_cols = terminal.grid.width() as usize;
        let grid_rows = terminal.grid.height() as usize;
        let cell_count = grid_cols * grid_rows;

        if cell_count == 0 {
            return Ok(());
        }

        // Frame skip: if no content changed and cursor is identical, skip entirely.
        let cursor_row = terminal.cursor.row as u32;
        let cursor_col = terminal.cursor.col as u32;
        let cursor_visible = u32::from(terminal.cursor.visible);
        let content_dirty = dirty_rows.iter().any(|&d| d);
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

        let instance_size = std::mem::size_of::<CellInstance>();
        let row_byte_size = grid_cols * instance_size;

        // Scroll DMA parameters (used later in the unified encoder).
        // When set, the render encoder will include a copy_buffer_to_buffer before the pass.
        let mut scroll_dma: Option<(u64, u64)> = None; // (src_offset, copy_size)

        if force_full_upload {
            backend.prepare_all_rows(terminal);
            backend.queue.write_buffer(
                &backend.cell_buffer,
                0,
                bytemuck::cast_slice(&backend.cell_instances[..cell_count]),
            );
        }

        // Scroll-aware upload: use GPU DMA to shift existing data in back buffer,
        // then upload only the new rows. CPU shadow is updated first to preserve old data.
        if !force_full_upload && scroll_count > 0 && scroll_count < grid_rows {
            let copy_rows = grid_rows - scroll_count;
            let first_new_row = grid_rows - scroll_count;

            // CPU shadow: shift OLD data BEFORE writing new rows (must read old data first)
            let src_start = scroll_count * grid_cols;
            let src_end = grid_rows * grid_cols;
            backend.cell_instances.copy_within(src_start..src_end, 0);

            // Prepare NEW rows at the bottom
            for row in first_new_row..grid_rows {
                backend.write_row_instances(terminal, row, grid_cols);
            }

            // Stage new rows to back buffer via write_buffer (batched with next submit)
            let upload_offset = (first_new_row * row_byte_size) as u64;
            let instance_start = first_new_row * grid_cols;
            let instance_end = grid_rows * grid_cols;
            backend.queue.write_buffer(
                &backend.cell_buffer_back,
                upload_offset,
                bytemuck::cast_slice(&backend.cell_instances[instance_start..instance_end]),
            );

            // Record DMA parameters for the unified encoder
            let src_offset = (scroll_count * row_byte_size) as u64;
            let copy_size = (copy_rows * row_byte_size) as u64;
            scroll_dma = Some((src_offset, copy_size));

            // Swap front/back buffers so render pass reads from the assembled back buffer
            std::mem::swap(&mut backend.cell_buffer, &mut backend.cell_buffer_back);
            std::mem::swap(
                &mut backend.cell_bind_group,
                &mut backend.cell_bind_group_back,
            );
        } else if !force_full_upload {
            // Standard path: prepare dirty rows and upload coalesced ranges in one pass.
            prepare_and_upload_dirty_rows(backend, terminal, dirty_rows, grid_cols, row_byte_size);
        }

        // Update grid uniforms
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
            selection_start: SELECTION_NONE,
            selection_end: SELECTION_NONE,
            blink_visible: 1,
            _pad: [0; 2],
        };
        backend.queue.write_buffer(
            &backend.grid_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        // Acquire surface texture with inline recovery.
        // Outdated/Lost -> reconfigure + retry (no service-layer budget consumed).
        // Timeout -> signal retry to caller.
        // OOM/Other -> signal degrade to caller.
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
                                // Reset counters only after confirmed successful acquire.
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

        // Render pass
        let bg_r = DEFAULT_BG.0 as f64 / 255.0;
        let bg_g = DEFAULT_BG.1 as f64 / 255.0;
        let bg_b = DEFAULT_BG.2 as f64 / 255.0;

        let mut encoder = backend
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terminal-encoder"),
            });

        // Scroll DMA: copy shifted rows in the same encoder before the render pass.
        // After the buffer swap above, cell_buffer_back holds the OLD front buffer (source)
        // and cell_buffer holds the NEW front buffer (destination with staged new rows).
        // write_buffer staging + this copy write to non-overlapping regions of cell_buffer.
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

impl Default for GpuRenderer {
    fn default() -> Self {
        Self::new(SurfaceRecoveryPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_retryable() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Timeout),
            SurfaceErrorCategory::Retryable
        );
    }

    #[test]
    fn pipeline_cache_reader_accepts_payload_at_cap() {
        let payload = vec![0_u8; MAX_PIPELINE_CACHE_BYTES as usize];
        let mut cursor = std::io::Cursor::new(payload.clone());
        let loaded = read_pipeline_cache_with_limit(&mut cursor, payload.len())
            .expect("payload at cap must load");
        assert_eq!(loaded.len(), payload.len());
    }

    #[test]
    fn pipeline_cache_reader_rejects_payload_above_cap() {
        let mut cursor = std::io::Cursor::new(vec![0_u8; MAX_PIPELINE_CACHE_BYTES as usize + 1]);
        assert_eq!(
            read_pipeline_cache_with_limit(&mut cursor, 0),
            Err(PipelineCacheReadError::TooLarge)
        );
    }

    #[test]
    fn outdated_and_lost_require_reconfigure() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Outdated),
            SurfaceErrorCategory::ReconfigureRequired
        );
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::Lost),
            SurfaceErrorCategory::ReconfigureRequired
        );
    }

    #[test]
    fn oom_degrades_to_cpu() {
        assert_eq!(
            classify_surface_error(&wgpu::SurfaceError::OutOfMemory),
            SurfaceErrorCategory::DegradeRequired
        );
    }

    #[test]
    fn retryable_errors_degrade_when_budget_is_exhausted() {
        let policy = SurfaceRecoveryPolicy::new(1);
        let first = policy.classify(wgpu::SurfaceError::Timeout, SurfaceRuntimeState::new(0, 0));
        let second = policy.classify(wgpu::SurfaceError::Timeout, SurfaceRuntimeState::new(1, 0));
        assert_eq!(first.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
    }

    #[test]
    fn zero_size_surface_config_is_rejected() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        assert_eq!(validate_surface_configuration(&config), Ok(()));

        config.width = 0;
        assert_eq!(
            validate_surface_configuration(&config),
            Err(SurfaceConfigurationError::ZeroWidth)
        );

        config.width = 1;
        config.height = 0;
        assert_eq!(
            validate_surface_configuration(&config),
            Err(SurfaceConfigurationError::ZeroHeight)
        );
    }

    #[test]
    fn acquire_timeout_transitions_from_retry_to_degrade() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let first = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(first.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let second = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(second.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 2);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let third = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(third.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn acquire_outdated_uses_reconfigure_budget_before_degrade() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 1);
        let mut state = SurfaceRuntimeState::default();

        let first = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(first.action, SurfaceRecoveryAction::ReconfigureSurface);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let second = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Lost);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn successful_reconfigure_resets_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let _ = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        let _ = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        policy.on_reconfigure_success(&mut state);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let after_reset = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(
            after_reset.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
    }

    #[test]
    fn configuration_errors_reconfigure_then_degrade_when_budget_exhausted() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(1, 1);
        let mut state = SurfaceRuntimeState::default();

        let first =
            policy.on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroWidth);
        assert_eq!(first.action, SurfaceRecoveryAction::ReconfigureSurface);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let second = policy
            .on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroHeight);
        assert_eq!(second.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn acquire_success_resets_all_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::new(2, 1);

        policy.on_acquire_success(&mut state);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn retry_path_clears_reconfigure_failure_streak() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let reconfigure = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
        assert_eq!(
            reconfigure.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);

        let retry = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(retry.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn configuration_error_resets_acquire_failure_streak() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::default();

        let timeout = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
        assert_eq!(timeout.action, SurfaceRecoveryAction::RetryAcquire);
        assert_eq!(state.consecutive_acquire_failures(), 1);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);

        let config_error =
            policy.on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroWidth);
        assert_eq!(
            config_error.action,
            SurfaceRecoveryAction::ReconfigureSurface
        );
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 1);
    }

    #[test]
    fn oom_degrade_is_immediate_and_clears_failure_counters() {
        let policy = SurfaceRecoveryPolicy::with_reconfigure_retry_budget(2, 2);
        let mut state = SurfaceRuntimeState::new(2, 1);

        let decision = policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::OutOfMemory);
        assert_eq!(decision.action, SurfaceRecoveryAction::DegradeToCpu);
        assert_eq!(state.consecutive_acquire_failures(), 0);
        assert_eq!(state.consecutive_reconfigure_failures(), 0);
    }

    #[test]
    fn update_frame_latency_hint_is_explicit_and_monitor_driven() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        update_frame_latency_hint(&mut config, 4);
        assert_eq!(config.desired_maximum_frame_latency, 4);
    }

    #[test]
    fn update_surface_extent_clamps_requested_extent_to_device_limit() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        update_surface_extent(&mut config, 8192, 4096, 2048).expect("extent update");
        assert_eq!(config.width, 2048);
        assert_eq!(config.height, 2048);
    }

    #[test]
    fn update_surface_extent_rejects_zero_dimensions_before_clamping() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        assert_eq!(
            update_surface_extent(&mut config, 0, 16, 2048),
            Err(SurfaceConfigurationError::ZeroWidth)
        );
        assert_eq!(
            update_surface_extent(&mut config, 16, 0, 2048),
            Err(SurfaceConfigurationError::ZeroHeight)
        );
    }

    #[test]
    fn cell_buffer_capacity_growth_is_geometric() {
        assert_eq!(next_cell_buffer_capacity(3840, 3841), 7680);
        assert_eq!(next_cell_buffer_capacity(7680, 12000), 15360);
    }

    #[test]
    fn cell_buffer_capacity_growth_is_stable_when_current_capacity_is_sufficient() {
        assert_eq!(next_cell_buffer_capacity(4096, 4096), 4096);
        assert_eq!(next_cell_buffer_capacity(4096, 1024), 4096);
    }

    #[test]
    fn cell_buffer_capacity_shrink_is_triggered_only_when_underutilized() {
        assert_eq!(
            shrink_cell_buffer_capacity(16_384, 2_000, INITIAL_CELL_BUFFER_CAPACITY),
            Some(3840),
        );
        assert_eq!(
            shrink_cell_buffer_capacity(16_384, 5_000, INITIAL_CELL_BUFFER_CAPACITY),
            None,
        );
    }

    #[test]
    fn cell_buffer_capacity_shrink_never_drops_below_initial_capacity() {
        assert_eq!(
            shrink_cell_buffer_capacity(
                INITIAL_CELL_BUFFER_CAPACITY,
                1,
                INITIAL_CELL_BUFFER_CAPACITY
            ),
            None
        );
    }

    #[test]
    fn initial_capacity_scales_with_viewport() {
        assert_eq!(
            initial_cell_buffer_capacity(0, 0),
            INITIAL_CELL_BUFFER_CAPACITY
        );
        assert_eq!(
            initial_cell_buffer_capacity(3840, 2160),
            next_cell_buffer_capacity(
                INITIAL_CELL_BUFFER_CAPACITY,
                (3840usize / CELL_WIDTH) * (2160usize / CELL_HEIGHT),
            )
        );
    }

    #[test]
    fn render_frame_returns_backend_unavailable_when_uninitialized() {
        let mut renderer = GpuRenderer::default();
        let terminal = TerminalState::new(80, 24, 100);
        let dirty = vec![true; 24];
        assert_eq!(
            renderer.render_frame(&terminal, &dirty, 0),
            Err(GpuRenderError::BackendUnavailable)
        );
    }

    // --- F9: Glyph Atlas & Cell Data Tests ---

    #[test]
    fn glyph_cache_has_ascii() {
        let cache = GlyphCache::new(8, 16);
        for ch in b'A'..=b'Z' {
            assert!(
                cache.has_glyph(ch as char),
                "missing glyph for ASCII char '{}'",
                ch as char
            );
        }
    }

    #[test]
    fn glyph_cache_has_cyrillic() {
        let cache = GlyphCache::new(8, 16);
        // Cyrillic small letter de
        assert!(cache.has_glyph('\u{0434}'));
    }

    #[test]
    fn glyph_cache_has_box_drawing() {
        let cache = GlyphCache::new(8, 16);
        let box_chars = ['─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼'];
        for ch in box_chars {
            assert!(
                cache.has_glyph(ch),
                "missing glyph for box-drawing char '{ch}'"
            );
        }
    }

    #[test]
    fn glyph_cache_missing_for_unknown() {
        let cache = GlyphCache::new(8, 16);
        assert!(!cache.has_glyph('\u{FFFF}'));
    }

    #[test]
    fn color_to_u32_default_uses_fallback() {
        let result = color_to_u32(Color::Default, (0xFF, 0x80, 0x40));
        assert_eq!(result, 0xFF8040);
    }

    #[test]
    fn color_to_u32_rgb() {
        let result = color_to_u32(Color::Rgb(0x12, 0x34, 0x56), (0, 0, 0));
        assert_eq!(result, 0x123456);
    }

    #[test]
    fn color_to_u32_indexed() {
        // Index 1 = standard red = 0xCC0000 in ANSI palette
        let result = color_to_u32(Color::Indexed(1), (0, 0, 0));
        assert_eq!(result, ANSI_PALETTE[1]);
    }

    #[test]
    fn write_glyph_to_atlas_places_data_correctly() {
        let cw = ATLAS_GLYPH_WIDTH as usize;
        let ch = ATLAS_GLYPH_HEIGHT as usize;
        let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
        let mut cell_buf = vec![0u8; cw * ch];
        // Write a single pixel at top-left
        cell_buf[0] = 200;
        write_glyph_to_atlas(&mut atlas_data, 1, &cell_buf);
        // Slot 1 at col=1, row=0 -> x=8, y=0
        let expected_idx = cw;
        assert_eq!(atlas_data[expected_idx], 200);
    }

    #[test]
    fn atlas_constants_are_consistent() {
        assert_eq!(ATLAS_GLYPH_COLS * ATLAS_GLYPH_WIDTH, ATLAS_SIZE);
        assert_eq!(ATLAS_GLYPH_ROWS * ATLAS_GLYPH_HEIGHT, ATLAS_SIZE);
        assert_eq!(ATLAS_SLOTS, (ATLAS_GLYPH_COLS * ATLAS_GLYPH_ROWS) as usize);
    }

    #[test]
    fn cell_instance_bytemuck_pod_layout() {
        assert_eq!(std::mem::size_of::<CellInstance>(), 16);
        assert_eq!(std::mem::align_of::<CellInstance>(), 4);
    }

    #[test]
    fn grid_uniforms_bytemuck_pod_layout() {
        assert_eq!(std::mem::size_of::<GridUniforms>(), 64);
        assert_eq!(std::mem::align_of::<GridUniforms>(), 4);
    }

    #[test]
    fn attr_flags_do_not_overlap_atlas_index() {
        assert_eq!(ATTR_BOLD & 0xFFFF, 0);
        assert_eq!(ATTR_ITALIC & 0xFFFF, 0);
        assert_eq!(ATTR_UNDERLINE & 0xFFFF, 0);
        assert_eq!(ATTR_STRIKETHROUGH & 0xFFFF, 0);
        assert_eq!(ATTR_DIM & 0xFFFF, 0);
        assert_eq!(ATTR_INVERSE & 0xFFFF, 0);
        // All flags use distinct bits
        let all_flags =
            ATTR_BOLD | ATTR_ITALIC | ATTR_UNDERLINE | ATTR_STRIKETHROUGH | ATTR_DIM | ATTR_INVERSE;
        assert_eq!(all_flags.count_ones(), 6);
    }

    #[test]
    fn selection_none_sentinel_is_u32_max() {
        assert_eq!(SELECTION_NONE, u32::MAX);
    }

    #[test]
    fn gpu_renderer_default_is_not_initialized() {
        let renderer = GpuRenderer::default();
        assert!(!renderer.is_initialized());
    }

    #[test]
    fn gpu_render_error_mapping_is_deterministic() {
        assert_eq!(
            GpuRenderError::DeviceLost.failure_kind(),
            GpuFailureKind::DeviceLost
        );
        assert_eq!(
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Timeout).failure_kind(),
            GpuFailureKind::SurfaceError
        );
        assert_eq!(
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Outdated).failure_kind(),
            GpuFailureKind::SwapchainOutOfDate
        );
        assert_eq!(
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Lost).failure_kind(),
            GpuFailureKind::SwapchainOutOfDate
        );
        assert_eq!(
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::OutOfMemory).failure_kind(),
            GpuFailureKind::OutOfMemory
        );
        assert_eq!(
            GpuRenderError::SubmitFailed.failure_kind(),
            GpuFailureKind::SubmitError
        );
        assert_eq!(
            GpuRenderError::BackendUnavailable.failure_kind(),
            GpuFailureKind::DeviceLost
        );
    }

    // ── Stress tests ─────────────────────────────────────────────

    #[test]
    fn stress_pack_cell_flags_all_64_combinations() {
        for bits in 0..64u8 {
            let attrs = Attrs {
                bold: bits & 1 != 0,
                italic: bits & 2 != 0,
                underline: bits & 4 != 0,
                strikethrough: bits & 8 != 0,
                dim: bits & 16 != 0,
                inverse: bits & 32 != 0,
                ..Default::default()
            };
            let slot = 12345u16;
            let flags = pack_cell_flags(slot, &attrs);
            assert_eq!(
                flags & 0xFFFF,
                slot as u32,
                "atlas index corrupted at bits={bits}"
            );
            assert_eq!((flags & ATTR_BOLD != 0), attrs.bold);
            assert_eq!((flags & ATTR_ITALIC != 0), attrs.italic);
            assert_eq!((flags & ATTR_UNDERLINE != 0), attrs.underline);
            assert_eq!((flags & ATTR_STRIKETHROUGH != 0), attrs.strikethrough);
            assert_eq!((flags & ATTR_DIM != 0), attrs.dim);
            assert_eq!((flags & ATTR_INVERSE != 0), attrs.inverse);
        }
    }

    #[test]
    fn stress_pack_cell_flags_max_atlas_slot() {
        let attrs = Attrs {
            bold: true,
            italic: true,
            underline: true,
            strikethrough: true,
            dim: true,
            inverse: true,
            ..Default::default()
        };
        let slot = 0xFFFFu16; // max 16-bit atlas slot
        let flags = pack_cell_flags(slot, &attrs);
        assert_eq!(flags & 0xFFFF, 0xFFFF);
        assert!(flags & ATTR_BOLD != 0);
        assert!(flags & ATTR_INVERSE != 0);
    }

    #[test]
    fn stress_cell_instance_bulk_creation() {
        let attrs = Attrs::default();
        let mut instances = Vec::with_capacity(80 * 50);
        for slot in 0..4000u16 {
            instances.push(CellInstance {
                atlas_and_flags: pack_cell_flags(slot, &attrs),
                fg_color: 0xD8D8D8,
                bg_color: 0x141B1F,
                _pad: 0,
            });
        }
        assert_eq!(instances.len(), 4000);
        assert_eq!(instances[3999].atlas_and_flags & 0xFFFF, 3999);
    }

    #[test]
    fn stress_grid_uniforms_cursor_boundary_values() {
        let edge_cases: &[(u32, u32)] = &[
            (0, 0),
            (0, u32::MAX),
            (u32::MAX, 0),
            (u32::MAX, u32::MAX),
            (255, 79),
        ];
        for &(row, col) in edge_cases {
            let uniforms = GridUniforms {
                cell_width: 8.0,
                cell_height: 16.0,
                grid_cols: 80,
                grid_rows: 50,
                viewport_width: 640.0,
                viewport_height: 800.0,
                atlas_cols: ATLAS_GLYPH_COLS,
                atlas_rows: ATLAS_GLYPH_ROWS,
                cursor_row: row,
                cursor_col: col,
                cursor_visible: 1,
                selection_start: SELECTION_NONE,
                selection_end: SELECTION_NONE,
                blink_visible: 1,
                _pad: [0; 2],
            };
            let bytes = bytemuck::bytes_of(&uniforms);
            assert_eq!(bytes.len(), 64);
        }
    }
}
