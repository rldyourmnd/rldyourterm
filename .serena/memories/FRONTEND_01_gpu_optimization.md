<!-- Memory Metadata
Last updated: 2026-03-09
Last commit: 7ea60ad chore: remove dead orchestrator infrastructure and align VSA ordering
Scope: crates/features/render_gpu/src/, crates/core/src/grid/, crates/app/src/gui_runtime*.rs
Area: FRONTEND
-->

# GPU Rendering Optimization Architecture

## Overview
Seven-phase GPU optimization plan. Phases 1, 2, 3, 4, 6, and 7 (partial) are complete as of 0043f11. Phase 5 (batch atlas hydration) is planned but not yet implemented. The design reduces idle CPU/GPU work to zero and minimizes PCIe transfer volume on partial screen updates.

Architecture plan stored at: `.serena/plans/gpu-optimization-architecture.md`

## Phase 1: Dirty-Row Tracking + Partial Upload (COMPLETE)

### Core layer - Grid dirty tracking
`Grid` holds `dirty_rows: Vec<bool>` (one slot per row, all initialized `true` on construction and resize). Every mutating Grid method marks affected rows dirty automatically - callers never invoke dirty tracking directly.

Dirty API:
- `dirty_rows() -> &[bool]` - borrow flags for the renderer (read-only, does not clear)
- `take_dirty_rows() -> Vec<u16>` - returns dirty row indices AND clears all flags atomically
- `has_dirty_rows() -> bool` - quick check before render
- `mark_all_dirty()` - called internally on clear, scroll, resize

### render_gpu layer - Persistent cell buffer + partial upload
`GpuBackend` holds `cell_instances: Vec<CellInstance>` that persists across frames. On each render cycle, only dirty rows are recomputed and uploaded:

1. `prepare_dirty_rows(terminal, dirty_rows: &[bool])` - iterates only rows where `dirty_rows[row]` is true; updates `cell_instances` in-place; clean rows retain their previous frame data untouched
2. `upload_dirty_ranges(queue, cell_buffer, cell_instances, dirty_rows, grid_cols, row_byte_size)` - coalesces adjacent dirty rows into contiguous spans; each span generates one `write_buffer` call with a byte offset; minimizes PCIe transactions

`render_frame` signature: `render_frame(&mut self, terminal: &TerminalState, dirty_rows: &[bool]) -> Result<(), GpuRenderError>`

### app layer - Frame skip
`GpuRenderer` tracks `last_cursor_row`, `last_cursor_col`, `last_cursor_visible` (all initialized to `u32::MAX`). At the start of `render_frame`:
- If `dirty_rows.iter().all(|&d| !d)` AND cursor position/visibility unchanged: return `Ok(())` immediately with zero GPU work
- On return, the caller (`gui_runtime`) calls `terminal.grid.take_dirty_rows()` ONLY after a successful present; dirty flags are preserved through failed surface acquisitions to prevent lost updates

### Performance
| Scenario | Before | After |
|---|---|---|
| Idle terminal | 128 KB upload, 8K cells CPU | 0 bytes, 0 CPU (frame skip) |
| Single char edit | 128 KB, 8K cells | ~2.5 KB, ~160 cells |
| Full screen update | 128 KB, 8K cells | 128 KB, 8K cells (unchanged) |

## Phase 2: GPU-Side Cursor Rendering (COMPLETE)

### Shader cursor overlay
Cursor is rendered purely in the fragment shader (`terminal.wgsl`). The fragment shader reads `grid.cursor_row`, `grid.cursor_col`, `grid.cursor_visible` from uniforms and inverts fg/bg at the cursor cell position. No cursor data is embedded in `CellInstance`.

```wgsl
let cursor_index = grid.cursor_row * grid.grid_cols + grid.cursor_col;
if grid.cursor_visible != 0u && in.instance == cursor_index {
    let tmp = fg; fg = bg; bg = tmp;
}
```

Cursor state changes (movement) only require a `write_buffer` call to the 16-byte `GridUniforms` buffer. No cell instance data is re-prepared or uploaded when only the cursor moves. The frame-skip logic in `render_frame` uses `last_cursor_*` fields to detect cursor changes and trigger the uniform update.

Note: cursor blink is not yet implemented (no blink timer exists). When implemented, it will update only the cursor_visible uniform without touching cell data.

## Phase 3: Text Attributes in Shader (COMPLETE)

### Attribute flag constants
Six attribute flags occupy the upper 16 bits of `CellInstance::atlas_and_flags`. The lower 16 bits remain the atlas slot index (up to 65536 glyphs).

| Constant | Value | SGR | Shader effect |
|---|---|---|---|
| `ATTR_BOLD` | `1 << 16` | SGR 1 | Double-strike: samples 1px right and merges via `max` |
| `ATTR_ITALIC` | `1 << 17` | SGR 3 | UV skew: shifts U coordinate by `(1 - cell_pos.y) * 0.15 / atlas_cols` |
| `ATTR_UNDERLINE` | `1 << 18` | SGR 4 | 1px solid line at bottom of cell (`cell_pos.y > 0.9375`) |
| `ATTR_STRIKETHROUGH` | `1 << 19` | SGR 9 | 1px line at cell midpoint (`cell_pos.y in [0.46875, 0.53125]`) |
| `ATTR_DIM` | `1 << 20` | SGR 2 | Halve foreground brightness (`fg * 0.5`) |
| `ATTR_INVERSE` | `1 << 21` | SGR 7 | Swap fg/bg colors |

### Shader pipeline order for color resolution
1. Unpack fg and bg from packed RGB u32 fields in `CellInstance`
2. Apply `DIM_BIT` (halve fg brightness)
3. Apply `INVERSE_BIT` (swap fg/bg)
4. Apply selection highlight inversion (if `selection_start != SEL_NONE` and cell is in range)
5. Apply cursor inversion at flat cursor index
6. Apply `ITALIC_BIT` UV skew before atlas sample
7. Sample atlas; apply `BOLD_BIT` double-strike sample offset
8. Apply `UNDERLINE_BIT` and `STRIKE_BIT` as pixel coverage overrides
9. Composite `mix(bg, fg, glyph_alpha)` to output

`cell_pos` (a `vec2<f32>` varying carrying the corner UV `[0,0]`-`[1,1]`) is emitted from the vertex shader and used in italic skew and line rendering.

## Phase 4: Selection Rendering (COMPLETE)

`selection_start` and `selection_end` are flat cell indices added to `GridUniforms`. The shader treats them as an inclusive range and inverts fg/bg for any cell `in.instance` that falls within `[min(sel_start, sel_end), max(sel_start, sel_end)]`. Both are set to `SELECTION_NONE` (`u32::MAX`) when no selection is active.

`GridUniforms` layout after Phase 4 (64 bytes, fits one cache line):

| Field | Type | Offset |
|---|---|---|
| cell_width | f32 | 0 |
| cell_height | f32 | 4 |
| grid_cols | u32 | 8 |
| grid_rows | u32 | 12 |
| viewport_width | f32 | 16 |
| viewport_height | f32 | 20 |
| atlas_cols | u32 | 24 |
| atlas_rows | u32 | 28 |
| cursor_row | u32 | 32 |
| cursor_col | u32 | 36 |
| cursor_visible | u32 | 40 |
| selection_start | u32 | 44 |
| selection_end | u32 | 48 |
| blink_visible | u32 | 52 |
| _pad | [u32; 2] | 56 |

Note: `selection_start`/`selection_end` are always written as `SELECTION_NONE` in `render_frame`; live selection state is not yet wired from the UI layer.

## Phase 5: Batch Atlas Hydration (PLANNED - NOT YET IMPLEMENTED)

No code exists for batch atlas hydration. The current path calls `ensure_glyph_in_atlas` per cell during `prepare_dirty_rows` and the scroll path. Atlas overflow is logged via `warn!` (added c887e95).

## Phase 6: Scroll-Aware GPU Buffer Management (COMPLETE)

### Dual cell buffers (ping-pong)
`GpuBackend` holds two GPU cell buffers (`cell_buffer` and `cell_buffer_back`), both created with `STORAGE | COPY_DST | COPY_SRC` usage flags. Corresponding bind groups (`cell_bind_group`, `cell_bind_group_back`) track which buffer the render pass reads.

### Scroll DMA path in `render_frame`
When `scroll_count > 0 && scroll_count < grid_rows`:

1. Issue `copy_buffer_to_buffer` from `cell_buffer` (offset `scroll_count * row_byte_size`) to `cell_buffer_back` (offset 0) for `(grid_rows - scroll_count)` rows - GPU DMA shift
2. On CPU, prepare `cell_instances` for only the new rows at the bottom (`first_new_row = grid_rows - scroll_count`)
3. Upload those new rows to `cell_buffer_back` at the correct byte offset
4. Shift the CPU-side `cell_instances` shadow array to match (via `copy_within`)
5. `std::mem::swap` both the GPU buffers and their bind groups so `cell_buffer` always points to the current front

When `scroll_count == 0` or `scroll_count >= grid_rows`, the standard dirty-row path runs instead.

### Grid scroll tracking
`Grid.scroll_count: usize` is incremented by `scroll_up` (only that method - other scroll methods call `mark_all_dirty` instead). It is reset to 0 by `take_dirty_rows`, `clear`, and `resize`. `scroll_count()` is a read-only accessor used by `gui_runtime.rs` before calling `render_frame`.

`render_frame` signature updated: `render_frame(&mut self, terminal: &TerminalState, dirty_rows: &[bool], scroll_count: usize)`

## Phase 7: Blink Timer Uniform (PARTIAL)

`blink_visible: u32` is present in `GridUniforms` and declared in the WGSL `GridUniforms` struct. It is always written as `1` in `render_frame` (blink always on). No blink timer or per-cell `BLINK_BIT` flag exists yet. The shader has a comment noting future per-cell blink support at bit 22. This phase is structurally scaffolded but not driven by a real timer.

## Shader Cursor Check Optimization (c887e95)

The cursor position test was changed from two integer divisions per fragment:
```wgsl
// old: two divisions per pixel per cell
let col = in.instance % grid.grid_cols;
let row = in.instance / grid.grid_cols;
if grid.cursor_visible != 0u && row == grid.cursor_row && col == grid.cursor_col { ... }
```
to a single flat index comparison:
```wgsl
let cursor_index = grid.cursor_row * grid.grid_cols + grid.cursor_col;
if grid.cursor_visible != 0u && in.instance == cursor_index { ... }
```
This eliminates approximately 1M unnecessary ALU operations per frame on a typical 80x24 terminal.

## GPU-to-CPU Fallback Full Redraw Fix (0043f11)

When the GPU path fails mid-session and falls back to CPU softbuffer, the CPU renderer has no valid prior content. The dirty-row flags were already cleared by `take_dirty_rows` on the last successful GPU present, leaving most rows appearing clean. Fix: `mark_all_dirty()` is called on the terminal grid inside the `FallbackToCpu` handler in `gui_runtime.rs` before the CPU render path executes, guaranteeing the first CPU frame is complete.

See `.serena/plans/gpu-optimization-architecture.md` for full design including anti-patterns and VSA compliance notes.

## Pipeline Cache (Vulkan only)
`GpuRenderer::initialize(target, width, height, cache_dir: Option<&Path>)` loads compiled pipeline data from disk when available. On shutdown, `save_pipeline_cache(cache_dir)` writes atomically (temp + rename). Cache key is adapter-specific via `wgpu::util::pipeline_cache_key`. No-op on Metal/DX12/GL.

Cache directory resolution: `resolve_gpu_cache_dir()` in `gui_runtime.rs` - macOS: `~/Library/Caches/rldyourterm`; Linux: `$XDG_CACHE_HOME/rldyourterm` with `~/.cache/rldyourterm` fallback.

## Deferred Atlas Loading
`build_glyph_atlas` at startup loads only three Unicode ranges: ASCII (0x0020-0x007F), Box Drawing (0x2500-0x257F), Block Elements (0x2580-0x259F) - approximately 255 glyphs total. All other ranges (Cyrillic, Latin Extended, Greek, Nerd Font icons, etc.) load on-demand via `ensure_glyph_in_atlas` at first render encounter. Typical cost: ~0.1 ms per new glyph, transparent during normal use.

## Cross-Platform Notes
- Partial buffer write with byte offset: supported on all wgpu backends (Vulkan, Metal, DX12, GL)
- Pipeline cache: Vulkan only (`wgpu::Features::PIPELINE_CACHE`); gracefully skipped on other backends
- All optimizations avoid compute shaders, geometry shaders, and other non-universal features
