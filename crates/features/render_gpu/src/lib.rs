use bytemuck::{Pod, Zeroable};
use rldyourterm_core::grid::{self, CELL_HEIGHT, CELL_WIDTH, Color};
use rldyourterm_core::state::TerminalState;
use rldyourterm_font::{GlyphCache, rasterize_for_atlas};
use rldyourterm_foundation::error::GpuFailureKind;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use tracing::info;

pub const DEFAULT_SURFACE_RETRY_BUDGET: u8 = 3;
pub const DEFAULT_SURFACE_RECONFIGURE_RETRY_BUDGET: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceErrorCategory {
    Retryable,
    ReconfigureRequired,
    DegradeRequired,
}

impl SurfaceErrorCategory {
    #[must_use]
    pub const fn default_action(self) -> SurfaceRecoveryAction {
        match self {
            Self::Retryable => SurfaceRecoveryAction::RetryAcquire,
            Self::ReconfigureRequired => SurfaceRecoveryAction::ReconfigureSurface,
            Self::DegradeRequired => SurfaceRecoveryAction::DegradeToCpu,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRecoveryAction {
    RetryAcquire,
    ReconfigureSurface,
    DegradeToCpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceErrorDecision {
    pub source: wgpu::SurfaceError,
    pub category: SurfaceErrorCategory,
    pub action: SurfaceRecoveryAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRecoveryPolicy {
    acquire_retry_budget: u8,
    reconfigure_retry_budget: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceRuntimeState {
    consecutive_acquire_failures: u8,
    consecutive_reconfigure_failures: u8,
}

impl SurfaceRuntimeState {
    #[must_use]
    pub const fn new(
        consecutive_acquire_failures: u8,
        consecutive_reconfigure_failures: u8,
    ) -> Self {
        Self {
            consecutive_acquire_failures,
            consecutive_reconfigure_failures,
        }
    }

    #[must_use]
    pub const fn consecutive_acquire_failures(self) -> u8 {
        self.consecutive_acquire_failures
    }

    #[must_use]
    pub const fn consecutive_reconfigure_failures(self) -> u8 {
        self.consecutive_reconfigure_failures
    }

    fn clear_acquire_failures(&mut self) {
        self.consecutive_acquire_failures = 0;
    }

    fn clear_reconfigure_failures(&mut self) {
        self.consecutive_reconfigure_failures = 0;
    }

    fn reset_failures(&mut self) {
        self.clear_acquire_failures();
        self.clear_reconfigure_failures();
    }

    fn mark_retry_acquire(&mut self) {
        self.consecutive_acquire_failures = self.consecutive_acquire_failures.saturating_add(1);
        self.clear_reconfigure_failures();
    }

    fn mark_reconfigure_attempt(&mut self) {
        self.clear_acquire_failures();
        self.consecutive_reconfigure_failures =
            self.consecutive_reconfigure_failures.saturating_add(1);
    }

    fn mark_degrade(&mut self) {
        self.reset_failures();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceConfigurationDecision {
    pub source: SurfaceConfigurationError,
    pub action: SurfaceRecoveryAction,
}

impl SurfaceRecoveryPolicy {
    #[must_use]
    pub const fn new(retry_budget: u8) -> Self {
        Self::with_reconfigure_retry_budget(retry_budget, retry_budget)
    }

    #[must_use]
    pub const fn with_reconfigure_retry_budget(
        acquire_retry_budget: u8,
        reconfigure_retry_budget: u8,
    ) -> Self {
        Self {
            acquire_retry_budget,
            reconfigure_retry_budget,
        }
    }

    #[must_use]
    pub const fn retry_budget(self) -> u8 {
        self.acquire_retry_budget
    }

    #[must_use]
    pub const fn acquire_retry_budget(self) -> u8 {
        self.acquire_retry_budget
    }

    #[must_use]
    pub const fn reconfigure_retry_budget(self) -> u8 {
        self.reconfigure_retry_budget
    }

    #[must_use]
    pub fn classify(
        self,
        error: wgpu::SurfaceError,
        state: SurfaceRuntimeState,
    ) -> SurfaceErrorDecision {
        let category = classify_surface_error(&error);
        let action = match category {
            SurfaceErrorCategory::Retryable
                if state.consecutive_acquire_failures < self.acquire_retry_budget =>
            {
                SurfaceRecoveryAction::RetryAcquire
            }
            SurfaceErrorCategory::Retryable => SurfaceRecoveryAction::DegradeToCpu,
            SurfaceErrorCategory::ReconfigureRequired
                if state.consecutive_reconfigure_failures < self.reconfigure_retry_budget =>
            {
                SurfaceRecoveryAction::ReconfigureSurface
            }
            SurfaceErrorCategory::ReconfigureRequired => SurfaceRecoveryAction::DegradeToCpu,
            SurfaceErrorCategory::DegradeRequired => SurfaceRecoveryAction::DegradeToCpu,
        };

        SurfaceErrorDecision {
            source: error,
            category,
            action,
        }
    }

    pub fn on_acquire_success(self, state: &mut SurfaceRuntimeState) {
        state.reset_failures();
    }

    pub fn on_reconfigure_success(self, state: &mut SurfaceRuntimeState) {
        state.reset_failures();
    }

    #[must_use]
    pub fn on_surface_acquire_error(
        self,
        state: &mut SurfaceRuntimeState,
        error: wgpu::SurfaceError,
    ) -> SurfaceErrorDecision {
        let decision = self.classify(error, *state);
        match decision.action {
            SurfaceRecoveryAction::RetryAcquire => state.mark_retry_acquire(),
            SurfaceRecoveryAction::ReconfigureSurface => state.mark_reconfigure_attempt(),
            SurfaceRecoveryAction::DegradeToCpu => state.mark_degrade(),
        }
        decision
    }

    #[must_use]
    pub fn on_surface_configuration_error(
        self,
        state: &mut SurfaceRuntimeState,
        error: SurfaceConfigurationError,
    ) -> SurfaceConfigurationDecision {
        let action = if state.consecutive_reconfigure_failures < self.reconfigure_retry_budget {
            state.mark_reconfigure_attempt();
            SurfaceRecoveryAction::ReconfigureSurface
        } else {
            state.mark_degrade();
            SurfaceRecoveryAction::DegradeToCpu
        };

        SurfaceConfigurationDecision {
            source: error,
            action,
        }
    }
}

impl Default for SurfaceRecoveryPolicy {
    fn default() -> Self {
        Self::with_reconfigure_retry_budget(
            DEFAULT_SURFACE_RETRY_BUDGET,
            DEFAULT_SURFACE_RECONFIGURE_RETRY_BUDGET,
        )
    }
}

#[must_use]
pub fn classify_surface_error(error: &wgpu::SurfaceError) -> SurfaceErrorCategory {
    // Mirrors wgpu `Surface::get_current_texture` error semantics:
    // timeout -> retry acquire, outdated/lost -> reconfigure swapchain, OOM -> degrade.
    match error {
        wgpu::SurfaceError::Timeout => SurfaceErrorCategory::Retryable,
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
            SurfaceErrorCategory::ReconfigureRequired
        }
        wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other => {
            SurfaceErrorCategory::DegradeRequired
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceConfigurationError {
    ZeroWidth,
    ZeroHeight,
}

impl fmt::Display for SurfaceConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => f.write_str("surface configuration width must be non-zero"),
            Self::ZeroHeight => f.write_str("surface configuration height must be non-zero"),
        }
    }
}

impl Error for SurfaceConfigurationError {}

pub fn validate_surface_configuration(
    config: &wgpu::SurfaceConfiguration,
) -> Result<(), SurfaceConfigurationError> {
    // `wgpu::Surface::configure` panics when width/height are zero; fail fast here.
    if config.width == 0 {
        return Err(SurfaceConfigurationError::ZeroWidth);
    }
    if config.height == 0 {
        return Err(SurfaceConfigurationError::ZeroHeight);
    }
    Ok(())
}

pub fn update_surface_extent(
    config: &mut wgpu::SurfaceConfiguration,
    width: u32,
    height: u32,
    max_texture_dimension_2d: u32,
) -> Result<(), SurfaceConfigurationError> {
    if width == 0 {
        return Err(SurfaceConfigurationError::ZeroWidth);
    }
    if height == 0 {
        return Err(SurfaceConfigurationError::ZeroHeight);
    }

    let max_texture_dimension_2d = max_texture_dimension_2d.max(1);
    config.width = width.min(max_texture_dimension_2d);
    config.height = height.min(max_texture_dimension_2d);
    Ok(())
}

pub fn update_frame_latency_hint(
    config: &mut wgpu::SurfaceConfiguration,
    desired_maximum_frame_latency: u32,
) {
    // Callers provide monitor-driven pacing inputs; renderer keeps this as an explicit hint.
    config.desired_maximum_frame_latency = desired_maximum_frame_latency;
}

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
            GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::Timeout)
            | GpuRenderError::SurfaceAcquire(wgpu::SurfaceError::OutOfMemory)
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

// Glyph atlas constants: 1024x1024 texture with 8x16 slots (128 cols x 64 rows = 8192 slots).
// Large enough to pre-populate ASCII, Latin, Cyrillic, Greek, Box Drawing, Block, Powerline
// and still have room for runtime-discovered Nerd Font glyphs.
const ATLAS_GLYPH_WIDTH: u32 = CELL_WIDTH as u32; // 8
const ATLAS_GLYPH_HEIGHT: u32 = CELL_HEIGHT as u32; // 16
const ATLAS_SIZE: u32 = 1024;
const ATLAS_GLYPH_COLS: u32 = ATLAS_SIZE / ATLAS_GLYPH_WIDTH; // 128
const ATLAS_GLYPH_ROWS: u32 = ATLAS_SIZE / ATLAS_GLYPH_HEIGHT; // 64
const ATLAS_SLOTS: usize = (ATLAS_GLYPH_COLS * ATLAS_GLYPH_ROWS) as usize; // 8192

// Default terminal colors (must match gui_runtime)
const DEFAULT_BG: (u8, u8, u8) = (0x14, 0x1b, 0x1f);
const DEFAULT_FG: (u8, u8, u8) = (0xd8, 0xd8, 0xd8);
const INITIAL_CELL_BUFFER_CAPACITY: usize = 120 * 32;

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
    _pad: u32,
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
    grid_uniform_buffer: wgpu::Buffer,
    grid_bind_group: wgpu::BindGroup,
    atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    cell_bind_group_layout: wgpu::BindGroupLayout,
    cell_bind_group: wgpu::BindGroup,
    cell_buffer: wgpu::Buffer,
    cell_buffer_capacity: usize,
    glyph_cache: GlyphCache,
    char_to_slot: HashMap<char, u16>,
    next_atlas_slot: u16,
    surface_state: SurfaceRuntimeState,
}

pub struct GpuRenderer {
    policy: SurfaceRecoveryPolicy,
    backend: Option<GpuBackend>,
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
        }
    }

    #[must_use]
    pub fn recovery_policy(&self) -> SurfaceRecoveryPolicy {
        self.policy
    }

    pub fn is_initialized(&self) -> bool {
        self.backend.is_some()
    }

    /// Initializes the GPU backend with wgpu device, surface, pipeline, and glyph atlas.
    /// Must be called from the main thread (winit event loop) before any rendering.
    pub fn initialize(
        &mut self,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<(), GpuRenderError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default().with_env());

        let surface = instance
            .create_surface(target)
            .map_err(|_| GpuRenderError::BackendUnavailable)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|_| GpuRenderError::BackendUnavailable)?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rldyourterm-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|_| GpuRenderError::DeviceLost)?;

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
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Build glyph atlas using fontdue rasterization
        let mut glyph_cache = GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16);
        let (atlas_texture, char_to_slot, next_atlas_slot) =
            build_glyph_atlas(&device, &queue, &mut glyph_cache);
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
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
            cache: None,
        });

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

        // Cell instance buffer (initial capacity for 120x32 terminal)
        let initial_capacity = INITIAL_CELL_BUFFER_CAPACITY;
        let cell_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell-instances"),
            size: (initial_capacity * std::mem::size_of::<CellInstance>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cell_bind_group = create_cell_bind_group(&device, &cell_bgl, &cell_buffer);

        info!(
            format = ?format,
            backend = ?adapter.get_info().backend,
            adapter_name = adapter.get_info().name,
            width,
            height,
            atlas_slots = ATLAS_SLOTS,
            glyph_count = char_to_slot.len(),
            "GPU backend initialized"
        );

        self.backend = Some(GpuBackend {
            device,
            queue,
            surface,
            config,
            pipeline,
            grid_uniform_buffer,
            grid_bind_group,
            atlas_texture,
            atlas_bind_group,
            cell_bind_group_layout: cell_bgl,
            cell_bind_group,
            cell_buffer,
            cell_buffer_capacity: initial_capacity,
            glyph_cache,
            char_to_slot,
            next_atlas_slot,
            surface_state: SurfaceRuntimeState::default(),
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
            let _ = update_surface_extent(&mut backend.config, width, height, max_dim);
            backend.surface.configure(&backend.device, &backend.config);
            // Reconfigure resets the surface — clear stale failure counters.
            self.policy
                .on_reconfigure_success(&mut backend.surface_state);
        }
    }

    /// Renders the terminal state to the GPU surface.
    pub fn render_frame(&mut self, terminal: &TerminalState) -> Result<(), GpuRenderError> {
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

        // Prepare cell instance data, dynamically adding missing glyphs to atlas
        let cells = prepare_cell_data(
            terminal,
            &mut backend.glyph_cache,
            &mut backend.char_to_slot,
            &mut backend.next_atlas_slot,
            &backend.atlas_texture,
            &backend.queue,
        );

        // Grow cell buffer if needed
        let next_capacity = next_cell_buffer_capacity(backend.cell_buffer_capacity, cell_count);
        if next_capacity != backend.cell_buffer_capacity {
            backend.cell_buffer = backend.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cell-instances"),
                size: (next_capacity * std::mem::size_of::<CellInstance>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            backend.cell_bind_group = create_cell_bind_group(
                &backend.device,
                &backend.cell_bind_group_layout,
                &backend.cell_buffer,
            );
            backend.cell_buffer_capacity = next_capacity;
        }

        // Upload cell data
        backend
            .queue
            .write_buffer(&backend.cell_buffer, 0, bytemuck::cast_slice(&cells));

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
            cursor_row: terminal.cursor.row as u32,
            cursor_col: terminal.cursor.col as u32,
            cursor_visible: u32::from(terminal.cursor.visible),
            _pad: 0,
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

impl Default for GpuRenderer {
    fn default() -> Self {
        Self::new(SurfaceRecoveryPolicy::default())
    }
}

fn create_cell_bind_group(
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

fn next_cell_buffer_capacity(current_capacity: usize, required_capacity: usize) -> usize {
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

// --- Glyph Atlas ---

/// Write a cell-sized glyph bitmap into the atlas data buffer at the given slot.
fn write_glyph_to_atlas(atlas_data: &mut [u8], slot: u16, cell_buf: &[u8]) {
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

fn build_glyph_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    glyph_cache: &mut GlyphCache,
) -> (wgpu::Texture, HashMap<char, u16>, u16) {
    let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
    let mut char_to_slot: HashMap<char, u16> = HashMap::new();

    // Slot 0 = blank (space character)
    char_to_slot.insert(' ', 0);
    let mut next_slot: u16 = 1;

    // Pre-populate common Unicode ranges using fontdue rasterization
    let ranges: &[(u32, u32)] = &[
        (0x0020, 0x007F), // ASCII
        (0x00A0, 0x00FF), // Latin-1 Supplement
        (0x0100, 0x017F), // Latin Extended-A
        (0x0400, 0x04FF), // Cyrillic
        (0x0370, 0x03FF), // Greek
        (0x2500, 0x257F), // Box Drawing
        (0x2580, 0x259F), // Block Elements
        (0x3040, 0x309F), // Hiragana
        (0x2600, 0x26FF), // Miscellaneous Symbols
        (0xE0A0, 0xE0D4), // Powerline
    ];

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

    (texture, char_to_slot, next_slot)
}

// --- Cell Data Preparation ---

/// Upload a single glyph to the atlas texture at the given slot via partial write.
fn upload_glyph_to_atlas(
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

/// Ensure a character has a slot in the atlas. If missing, rasterize and upload it.
/// Returns the atlas slot index (0 = space/blank for unknown chars when atlas is full).
fn ensure_glyph_in_atlas(
    ch: char,
    glyph_cache: &mut GlyphCache,
    char_to_slot: &mut HashMap<char, u16>,
    next_slot: &mut u16,
    atlas_texture: &wgpu::Texture,
    queue: &wgpu::Queue,
) -> u16 {
    if let Some(&slot) = char_to_slot.get(&ch) {
        return slot;
    }
    if (*next_slot as usize) >= ATLAS_SLOTS {
        return 0; // Atlas full - render as blank
    }
    let cell_buf = rasterize_for_atlas(glyph_cache, ch);
    let slot = *next_slot;
    upload_glyph_to_atlas(queue, atlas_texture, slot, &cell_buf);
    char_to_slot.insert(ch, slot);
    *next_slot = slot + 1;
    slot
}

fn prepare_cell_data(
    terminal: &TerminalState,
    glyph_cache: &mut GlyphCache,
    char_to_slot: &mut HashMap<char, u16>,
    next_slot: &mut u16,
    atlas_texture: &wgpu::Texture,
    queue: &wgpu::Queue,
) -> Vec<CellInstance> {
    let cols = terminal.grid.width() as usize;
    let rows = terminal.grid.height() as usize;
    let mut cells = Vec::with_capacity(cols * rows);

    for row in 0..rows {
        if let Ok(row_cells) = terminal.grid.row_cells(row as u16) {
            for cell in row_cells.iter().take(cols) {
                let attrs = &cell.attrs;
                let mut fg = grid::color_to_u32(attrs.fg, DEFAULT_FG);
                let mut bg = grid::color_to_u32(attrs.bg, DEFAULT_BG);

                if attrs.dim {
                    let r = (fg >> 16) & 0xFF;
                    let g = (fg >> 8) & 0xFF;
                    let b = fg & 0xFF;
                    fg = ((r / 2) << 16) | ((g / 2) << 8) | (b / 2);
                }

                if attrs.inverse {
                    std::mem::swap(&mut fg, &mut bg);
                }

                let slot = if cell.ch == ' ' {
                    0
                } else {
                    ensure_glyph_in_atlas(
                        cell.ch,
                        glyph_cache,
                        char_to_slot,
                        next_slot,
                        atlas_texture,
                        queue,
                    )
                };

                cells.push(CellInstance {
                    atlas_and_flags: slot as u32,
                    fg_color: fg,
                    bg_color: bg,
                    _pad: 0,
                });
            }
        } else {
            // Row read failed - fill with blank cells
            for _ in 0..cols {
                cells.push(CellInstance {
                    atlas_and_flags: 0,
                    fg_color: grid::color_to_u32(Color::Default, DEFAULT_FG),
                    bg_color: grid::color_to_u32(Color::Default, DEFAULT_BG),
                    _pad: 0,
                });
            }
        }
    }

    cells
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
    fn render_frame_returns_backend_unavailable_when_uninitialized() {
        let mut renderer = GpuRenderer::default();
        let terminal = TerminalState::new(80, 24, 100);
        assert_eq!(
            renderer.render_frame(&terminal),
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
        let result = grid::color_to_u32(Color::Default, (0xFF, 0x80, 0x40));
        assert_eq!(result, 0xFF8040);
    }

    #[test]
    fn color_to_u32_rgb() {
        let result = grid::color_to_u32(Color::Rgb(0x12, 0x34, 0x56), (0, 0, 0));
        assert_eq!(result, 0x123456);
    }

    #[test]
    fn color_to_u32_indexed() {
        // Index 1 = standard red = 0xCC0000 in ANSI palette
        let result = grid::color_to_u32(Color::Indexed(1), (0, 0, 0));
        assert_eq!(result, rldyourterm_core::grid::ANSI_PALETTE[1]);
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
        let expected_idx = 0 * ATLAS_SIZE as usize + cw;
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
        assert_eq!(std::mem::size_of::<GridUniforms>(), 48);
        assert_eq!(std::mem::align_of::<GridUniforms>(), 4);
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
            GpuFailureKind::SurfaceError
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
}
