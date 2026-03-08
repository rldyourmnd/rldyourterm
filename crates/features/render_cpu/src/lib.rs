mod rasterize;
mod renderer;

#[cfg(test)]
mod tests;

pub use rasterize::{
    DEFAULT_BG_U32, DEFAULT_FG_U32, render_terminal_buffer, resolve_cell_colors, rgb_to_u32,
};
pub use renderer::{
    CpuRenderFrame, CpuRenderFrameStats, CpuRenderRow, CpuRenderer, CpuRendererConfig,
};
pub use rldyourterm_services::terminal::{DEFAULT_BG, DEFAULT_FG};
