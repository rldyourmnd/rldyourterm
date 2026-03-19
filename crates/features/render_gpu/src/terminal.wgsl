// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

// Terminal cell renderer — instanced quads with glyph atlas sampling.
// Each instance is a grid cell; 6 vertices per quad (2 triangles).
// Supports text attributes (bold, italic, underline, double underline, overline,
// strikethrough, dim, inverse, blink, hidden, wide chars, custom underline color),
// selection highlighting via uniforms, and blink via timer uniform.

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
    overlay_row: u32,
    cursor_fg_color: u32,
    cursor_bg_color: u32,
    selection_fg_color: u32,
    selection_bg_color: u32,
};

struct CellInstance {
    atlas_and_flags: u32,
    fg_color: u32,
    bg_color: u32,
    underline_color: u32,
};

// Attribute flag bits in upper bits of atlas_and_flags.
const ATLAS_MASK: u32      = 0xFFFFu;
const BOLD_BIT: u32        = 0x10000u;   // bit 16
const ITALIC_BIT: u32      = 0x20000u;   // bit 17
const UNDERLINE_BIT: u32   = 0x40000u;   // bit 18
const STRIKE_BIT: u32      = 0x80000u;   // bit 19
const DIM_BIT: u32         = 0x100000u;  // bit 20
const INVERSE_BIT: u32     = 0x200000u;  // bit 21
const BLINK_BIT: u32       = 0x400000u;  // bit 22
const HIDDEN_BIT: u32      = 0x800000u;  // bit 23
const WIDE_BIT: u32        = 0x1000000u; // bit 24
const CONT_BIT: u32        = 0x2000000u; // bit 25
const DBL_UL_BIT: u32      = 0x4000000u; // bit 26
const OVERLINE_BIT: u32    = 0x8000000u; // bit 27
const CURLY_UL_BIT: u32    = 0x10000000u; // bit 28
const DOTTED_UL_BIT: u32   = 0x20000000u; // bit 29
const DASHED_UL_BIT: u32   = 0x40000000u; // bit 30
const SEARCH_HIT_BIT: u32  = 0x80000000u; // bit 31

// Selection sentinel: no active selection.
const SEL_NONE: u32 = 0xFFFFFFFFu;
const OVERLAY_NONE: u32 = 0xFFFFFFFFu;

// Atlas texture size (pixels) for bold pixel offset calculation.
const ATLAS_TEX_SIZE: f32 = 1024.0;
const CURLY_WAVE: array<u32, 8> = array<u32, 8>(2u, 1u, 0u, 1u, 2u, 1u, 0u, 1u);

@group(0) @binding(0) var<uniform> grid: GridUniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_samp: sampler;
@group(2) @binding(0) var<storage, read> cells: array<CellInstance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
    @location(2) cell_pos: vec2<f32>,
    @location(3) @interpolate(flat) atlas_and_flags: u32,
    @location(4) @interpolate(flat) fg_color: u32,
    @location(5) @interpolate(flat) bg_color: u32,
    @location(6) @interpolate(flat) underline_color: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vertex_index];

    let col = instance_index % grid.grid_cols;
    let row = instance_index / grid.grid_cols;

    let cell = cells[instance_index];

    // Wide character: double quad width to span 2 grid columns (CJK/emoji).
    var cell_span = grid.cell_width;
    if (cell.atlas_and_flags & WIDE_BIT) != 0u {
        cell_span = grid.cell_width * 2.0;
    }
    var px = f32(col) * grid.cell_width + corner.x * cell_span;
    let py = (f32(row) + corner.y) * grid.cell_height;

    // Italic: screen-space shear — shift top vertices rightward (SGR 3).
    // Applied here to avoid atlas UV bleeding from adjacent glyph slots.
    if (cell.atlas_and_flags & ITALIC_BIT) != 0u {
        px = px + (1.0 - corner.y) * 0.15 * grid.cell_width;
    }

    let ndc_x = (px / grid.viewport_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / grid.viewport_height) * 2.0;

    // Atlas UV from lower 16 bits of atlas_and_flags
    let atlas_index = cell.atlas_and_flags & ATLAS_MASK;
    let glyph_col = atlas_index % grid.atlas_cols;
    let glyph_row = atlas_index / grid.atlas_cols;
    let u = (f32(glyph_col) + corner.x) / f32(grid.atlas_cols);
    let v = (f32(glyph_row) + corner.y) / f32(grid.atlas_rows);

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    out.instance = instance_index;
    out.cell_pos = corner;
    out.atlas_and_flags = cell.atlas_and_flags;
    out.fg_color = cell.fg_color;
    out.bg_color = cell.bg_color;
    out.underline_color = cell.underline_color;
    return out;
}

fn unpack_rgb(packed: u32) -> vec3<f32> {
    return vec3<f32>(
        f32((packed >> 16u) & 0xFFu) / 255.0,
        f32((packed >> 8u) & 0xFFu) / 255.0,
        f32(packed & 0xFFu) / 255.0,
    );
}

fn underline_decoration_alpha(flags: u32, cell_pos: vec2<f32>) -> f32 {
    let cell_width_px = max(grid.cell_width, 1.0);
    let span_px = select(cell_width_px, cell_width_px * 2.0, (flags & WIDE_BIT) != 0u);
    let x_px = u32(floor(cell_pos.x * span_px));
    let cell_height_px = max(u32(grid.cell_height), 1u);
    let y_px = min(u32(floor(cell_pos.y * grid.cell_height)), cell_height_px - 1u);
    let bottom_row = cell_height_px - 1u;
    let upper_row = max(cell_height_px, 3u) - 3u;

    var alpha: f32 = 0.0;

    if (flags & UNDERLINE_BIT) != 0u && y_px == bottom_row {
        alpha = 1.0;
    }

    if (flags & DBL_UL_BIT) != 0u && (y_px == upper_row || y_px == bottom_row) {
        alpha = 1.0;
    }

    if (flags & DOTTED_UL_BIT) != 0u && y_px == bottom_row && (x_px % 2u == 0u) {
        alpha = 1.0;
    }

    if (flags & DASHED_UL_BIT) != 0u && y_px == bottom_row && (x_px % 4u < 3u) {
        alpha = 1.0;
    }

    if (flags & CURLY_UL_BIT) != 0u {
        let target_row = upper_row + CURLY_WAVE[x_px % 8u];
        if y_px == target_row {
            alpha = 1.0;
        }
    }

    return alpha;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = in.atlas_and_flags;

    // Continuation cell: discard to preserve wide cell's glyph underneath.
    if (flags & CONT_BIT) != 0u {
        discard;
    }

    var fg = unpack_rgb(in.fg_color);
    var bg = unpack_rgb(in.bg_color);
    let cell_row = in.instance / grid.grid_cols;
    let is_overlay_cell = grid.overlay_row != OVERLAY_NONE && cell_row == grid.overlay_row;

    // Dim: halve foreground brightness (SGR 2)
    if (flags & DIM_BIT) != 0u {
        fg = fg * 0.5;
    }

    // Inverse: swap fg/bg (SGR 7)
    if (flags & INVERSE_BIT) != 0u {
        let tmp = fg;
        fg = bg;
        bg = tmp;
    }

    // Hidden: render as invisible - fg becomes bg (SGR 8)
    // Applied before selection/cursor so the cursor remains visible on hidden cells
    if (flags & HIDDEN_BIT) != 0u {
        fg = bg;
    }

    if (flags & SEARCH_HIT_BIT) != 0u {
        bg = mix(bg, fg, 0.1875);
    }

    let cursor_index = grid.cursor_row * grid.grid_cols + grid.cursor_col;
    let is_cursor_cell = !is_overlay_cell && grid.cursor_visible != 0u && in.instance == cursor_index;
    if grid.selection_start != SEL_NONE && !is_overlay_cell {
        let sel_lo = min(grid.selection_start, grid.selection_end);
        let sel_hi = max(grid.selection_start, grid.selection_end);
        if in.instance >= sel_lo && in.instance <= sel_hi {
            fg = unpack_rgb(grid.selection_fg_color);
            bg = unpack_rgb(grid.selection_bg_color);
        }
    }

    // Cursor: shape-aware rendering (DECSCUSR)
    // Shapes: 0/1=blinking block, 2=steady block, 3=blinking underline,
    // 4=steady underline, 5=blinking bar, 6=steady bar.
    if is_cursor_cell {
        let shape = grid.cursor_shape;
        let is_blinking = (shape == 0u || (shape & 1u) != 0u);
        let cursor_on = !is_blinking || grid.blink_visible != 0u;

        if cursor_on {
            let is_underline = (shape == 3u || shape == 4u);
            let is_bar = (shape == 5u || shape == 6u);

            var in_cursor = true;
            if is_underline {
                in_cursor = in.cell_pos.y > 0.875;
            } else if is_bar {
                in_cursor = in.cell_pos.x < 0.25;
            }

            if in_cursor {
                fg = unpack_rgb(grid.cursor_fg_color);
                bg = unpack_rgb(grid.cursor_bg_color);
            }
        }
    }

    // Glyph coverage from R8Unorm atlas
    let uv = in.uv;
    var glyph_alpha = textureSample(atlas_tex, atlas_samp, uv).r;

    // Bold: double-strike — sample 1px to the right and merge (SGR 1)
    if (flags & BOLD_BIT) != 0u {
        let bold_offset = vec2<f32>(1.0 / ATLAS_TEX_SIZE, 0.0);
        let bold_alpha = textureSample(atlas_tex, atlas_samp, uv + bold_offset).r;
        glyph_alpha = max(glyph_alpha, bold_alpha);
    }

    // Blink: hide glyph when blink timer is off (SGR 5/6)
    if (flags & BLINK_BIT) != 0u && grid.blink_visible == 0u {
        glyph_alpha = 0.0;
    }

    // Decoration color for underline/double underline (SGR 58 or fallback to fg).
    let decoration_color = select(fg, unpack_rgb(in.underline_color), in.underline_color != 0u);

    let decoration_alpha = underline_decoration_alpha(flags, in.cell_pos);

    // Overline: 1px line at top of cell (SGR 53) — uses fg color
    if (flags & OVERLINE_BIT) != 0u && in.cell_pos.y < 0.0625 {
        glyph_alpha = 1.0;
    }

    // Strikethrough: 1px line at middle of cell (SGR 9) — uses fg color
    if (flags & STRIKE_BIT) != 0u && in.cell_pos.y > 0.46875 && in.cell_pos.y < 0.53125 {
        glyph_alpha = 1.0;
    }

    // Composite: glyph over background, then underline decoration over result.
    var color = mix(bg, fg, glyph_alpha);
    color = mix(color, decoration_color, decoration_alpha);
    return vec4<f32>(color, 1.0);
}
