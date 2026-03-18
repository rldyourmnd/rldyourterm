// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use super::super::{
    MAX_FRAMEBUFFER_HEIGHT, MAX_FRAMEBUFFER_PIXELS, MAX_FRAMEBUFFER_WIDTH, MAX_VIEWPORT_CELLS,
    MAX_VIEWPORT_COLS, MAX_VIEWPORT_ROWS, ViewportGeometry, cap_framebuffer_extent,
    cap_terminal_geometry, viewport_geometry_changed,
};
use winit::dpi::PhysicalSize;

#[test]
fn viewport_geometry_cap_preserves_small_dimensions() {
    assert_eq!(cap_terminal_geometry(120, 32), (120, 32));
}

#[test]
fn viewport_geometry_change_detects_pixel_only_updates() {
    assert!(viewport_geometry_changed(
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        },
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1400,
            pixel_height: 800,
        }
    ));
    assert!(!viewport_geometry_changed(
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        },
        ViewportGeometry {
            cols: 120,
            rows: 32,
            pixel_width: 1280,
            pixel_height: 800,
        }
    ));
}

#[test]
fn viewport_geometry_cap_enforces_axis_and_cell_limits() {
    let (cols, rows) = cap_terminal_geometry(20_000, 20_000);
    assert!(cols as usize <= MAX_VIEWPORT_COLS);
    assert!(rows as usize <= MAX_VIEWPORT_ROWS);
    assert!((cols as usize) * (rows as usize) <= MAX_VIEWPORT_CELLS);
}

#[test]
fn framebuffer_extent_cap_preserves_small_dimensions() {
    let capped = cap_framebuffer_extent(PhysicalSize::new(1920, 1080));
    assert_eq!(capped, PhysicalSize::new(1920, 1080));
}

#[test]
fn framebuffer_extent_cap_enforces_axis_and_pixel_limits() {
    let capped = cap_framebuffer_extent(PhysicalSize::new(100_000, 100_000));
    assert!(capped.width <= MAX_FRAMEBUFFER_WIDTH);
    assert!(capped.height <= MAX_FRAMEBUFFER_HEIGHT);
    assert!(u64::from(capped.width) * u64::from(capped.height) <= MAX_FRAMEBUFFER_PIXELS);
}
