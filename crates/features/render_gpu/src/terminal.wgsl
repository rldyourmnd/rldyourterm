// Terminal cell renderer — instanced quads with glyph atlas sampling.
// Each instance is a grid cell; 6 vertices per quad (2 triangles).
// Supports text attributes (bold, italic, underline, strikethrough, dim, inverse),
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
    _pad0: u32,
    _pad1: u32,
};

struct CellInstance {
    atlas_and_flags: u32,
    fg_color: u32,
    bg_color: u32,
    _pad: u32,
};

// Attribute flag bits in upper bits of atlas_and_flags.
const ATLAS_MASK: u32      = 0xFFFFu;
const BOLD_BIT: u32        = 0x10000u;
const ITALIC_BIT: u32      = 0x20000u;
const UNDERLINE_BIT: u32   = 0x40000u;
const STRIKE_BIT: u32      = 0x80000u;
const DIM_BIT: u32         = 0x100000u;
const INVERSE_BIT: u32     = 0x200000u;

// Selection sentinel: no active selection.
const SEL_NONE: u32 = 0xFFFFFFFFu;

// Atlas texture size (pixels) for bold pixel offset calculation.
const ATLAS_TEX_SIZE: f32 = 1024.0;

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

    var px = (f32(col) + corner.x) * grid.cell_width;
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
    return out;
}

fn unpack_rgb(packed: u32) -> vec3<f32> {
    return vec3<f32>(
        f32((packed >> 16u) & 0xFFu) / 255.0,
        f32((packed >> 8u) & 0xFFu) / 255.0,
        f32(packed & 0xFFu) / 255.0,
    );
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = in.atlas_and_flags;

    var fg = unpack_rgb(in.fg_color);
    var bg = unpack_rgb(in.bg_color);

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

    // Selection highlight: invert colors for selected range
    if grid.selection_start != SEL_NONE {
        let sel_lo = min(grid.selection_start, grid.selection_end);
        let sel_hi = max(grid.selection_start, grid.selection_end);
        if in.instance >= sel_lo && in.instance <= sel_hi {
            let tmp = fg;
            fg = bg;
            bg = tmp;
        }
    }

    // Cursor: invert at cursor position (flat index comparison)
    let cursor_index = grid.cursor_row * grid.grid_cols + grid.cursor_col;
    if grid.cursor_visible != 0u && in.instance == cursor_index {
        let tmp = fg;
        fg = bg;
        bg = tmp;
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

    // Underline: 1px solid line at bottom of cell (SGR 4)
    if (flags & UNDERLINE_BIT) != 0u && in.cell_pos.y > 0.9375 {
        glyph_alpha = 1.0;
    }

    // Strikethrough: 1px line at middle of cell (SGR 9)
    if (flags & STRIKE_BIT) != 0u && in.cell_pos.y > 0.46875 && in.cell_pos.y < 0.53125 {
        glyph_alpha = 1.0;
    }

    // Blink: hide glyph when blink timer is off (SGR 5)
    // Blink flag would be bit 22 if set — for now blink_visible controls
    // global blink state for future per-cell blink support.

    // Composite foreground glyph over background
    let color = mix(bg, fg, glyph_alpha);
    return vec4<f32>(color, 1.0);
}
