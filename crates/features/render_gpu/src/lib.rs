// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod atlas;
mod bootstrap;
mod cell_data;
mod frame;
mod pipeline_cache;
mod surface;

use bytemuck::{Pod, Zeroable};
use rldyourterm_font::GlyphCache;
use rldyourterm_services::render_mode::GpuFailureKind;
#[cfg(test)]
use rldyourterm_services::terminal::ANSI_PALETTE;
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Cell, Color, DEFAULT_BG, DEFAULT_FG, TerminalState,
    color_to_u32,
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
            GpuRenderError::BackendUnavailable => GpuFailureKind::BackendUnavailable,
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

// Attribute flag bits packed in upper bits of CellInstance::atlas_and_flags.
// Lower 16 bits = atlas slot index (supports 65536 glyphs).
const ATTR_BOLD: u32 = 1 << 16;
const ATTR_ITALIC: u32 = 1 << 17;
const ATTR_UNDERLINE: u32 = 1 << 18;
const ATTR_STRIKETHROUGH: u32 = 1 << 19;
const ATTR_DIM: u32 = 1 << 20;
const ATTR_INVERSE: u32 = 1 << 21;
const ATTR_BLINK: u32 = 1 << 22;
const ATTR_HIDDEN: u32 = 1 << 23;
const ATTR_WIDE: u32 = 1 << 24;
const ATTR_CONTINUATION: u32 = 1 << 25;
const ATTR_DOUBLE_UNDERLINE: u32 = 1 << 26;
const ATTR_OVERLINE: u32 = 1 << 27;

/// Sentinel value indicating no active selection (u32::MAX).
pub const SELECTION_NONE: u32 = u32::MAX;
// Matches ui::DEFAULT_TERMINAL_COLS * ui::DEFAULT_TERMINAL_ROWS for initial buffer sizing.
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
    cursor_shape: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct CellInstance {
    pub atlas_and_flags: u32,
    pub fg_color: u32,
    pub bg_color: u32,
    pub underline_color: u32,
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
    slot_to_char: Vec<Option<char>>,
    slot_last_used: Vec<u64>,
    frame_counter: u64,
    next_atlas_slot: u16,
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
}

impl Default for GpuRenderer {
    fn default() -> Self {
        Self::new(SurfaceRecoveryPolicy::default())
    }
}

#[cfg(test)]
mod tests;
