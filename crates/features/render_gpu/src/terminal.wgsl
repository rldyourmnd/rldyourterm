// Terminal cell renderer — instanced quads with glyph atlas sampling.
// Each instance is a grid cell; 6 vertices per quad (2 triangles).

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
};

struct CellInstance {
    atlas_and_flags: u32,
    fg_color: u32,
    bg_color: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> grid: GridUniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_samp: sampler;
@group(2) @binding(0) var<storage, read> cells: array<CellInstance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    // Two-triangle quad: (0,0),(1,0),(0,1) + (1,0),(1,1),(0,1)
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vertex_index];

    // Cell position from instance index (row-major storage order)
    let col = instance_index % grid.grid_cols;
    let row = instance_index / grid.grid_cols;

    // Pixel coordinates of this vertex
    let px = (f32(col) + corner.x) * grid.cell_width;
    let py = (f32(row) + corner.y) * grid.cell_height;

    // Pixel to NDC [-1, 1]
    let ndc_x = (px / grid.viewport_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / grid.viewport_height) * 2.0;

    // Atlas UV — glyph slot maps to a tile in the atlas grid
    let cell = cells[instance_index];
    let glyph_col = cell.atlas_and_flags % grid.atlas_cols;
    let glyph_row = cell.atlas_and_flags / grid.atlas_cols;
    let u = (f32(glyph_col) + corner.x) / f32(grid.atlas_cols);
    let v = (f32(glyph_row) + corner.y) / f32(grid.atlas_rows);

    var out: VertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    out.instance = instance_index;
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
    let cell = cells[in.instance];
    var fg = unpack_rgb(cell.fg_color);
    var bg = unpack_rgb(cell.bg_color);

    // Invert colors at cursor position
    let col = in.instance % grid.grid_cols;
    let row = in.instance / grid.grid_cols;
    if grid.cursor_visible != 0u && row == grid.cursor_row && col == grid.cursor_col {
        let tmp = fg;
        fg = bg;
        bg = tmp;
    }

    // Glyph alpha from R8Unorm atlas (coverage in .r channel)
    let glyph_alpha = textureSample(atlas_tex, atlas_samp, in.uv).r;

    // Composite foreground glyph over background
    let color = mix(bg, fg, glyph_alpha);
    return vec4<f32>(color, 1.0);
}
