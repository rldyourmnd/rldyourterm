use super::super::*;

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
    let slot = 0xFFFFu16;
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
            underline_color: 0,
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
            cursor_shape: 0,
            _pad: 0,
        };
        let bytes = bytemuck::bytes_of(&uniforms);
        assert_eq!(bytes.len(), 64);
    }
}
