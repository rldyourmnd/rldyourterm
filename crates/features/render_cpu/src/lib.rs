// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod rasterize;
mod renderer;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_stress;

pub use rasterize::{
    DEFAULT_BG_U32, DEFAULT_FG_U32, render_terminal_buffer, resolve_cell_colors, rgb_to_u32,
};
pub use renderer::{
    CpuRenderFrame, CpuRenderFrameStats, CpuRenderRow, CpuRenderer, CpuRendererConfig,
};
pub use rldyourterm_services::terminal::{DEFAULT_BG, DEFAULT_FG};
