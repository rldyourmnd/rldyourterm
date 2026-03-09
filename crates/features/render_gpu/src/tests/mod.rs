// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod stress;

use super::*;

#[test]
fn timeout_is_retryable() {
    assert_eq!(
        classify_surface_error(&wgpu::SurfaceError::Timeout),
        SurfaceErrorCategory::Retryable
    );
}

#[test]
fn pipeline_cache_reader_accepts_payload_at_cap() {
    let payload = vec![0_u8; MAX_PIPELINE_CACHE_BYTES as usize];
    let mut cursor = std::io::Cursor::new(payload.clone());
    let loaded = read_pipeline_cache_with_limit(&mut cursor, payload.len())
        .expect("payload at cap must load");
    assert_eq!(loaded.len(), payload.len());
}

#[test]
fn pipeline_cache_reader_rejects_payload_above_cap() {
    let mut cursor = std::io::Cursor::new(vec![0_u8; MAX_PIPELINE_CACHE_BYTES as usize + 1]);
    assert_eq!(
        read_pipeline_cache_with_limit(&mut cursor, 0),
        Err(PipelineCacheReadError::TooLarge)
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

    let second =
        policy.on_surface_configuration_error(&mut state, SurfaceConfigurationError::ZeroHeight);
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
fn update_surface_extent_applies_within_max_texture_size() {
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

    update_surface_extent(&mut config, 1920, 1080, 2048).expect("extent update");
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
}

#[test]
fn update_surface_extent_clamps_to_max_texture_size() {
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
fn cell_buffer_capacity_shrink_is_triggered_only_when_underutilized() {
    assert_eq!(
        shrink_cell_buffer_capacity(16_384, 2_000, INITIAL_CELL_BUFFER_CAPACITY),
        Some(3840),
    );
    assert_eq!(
        shrink_cell_buffer_capacity(16_384, 5_000, INITIAL_CELL_BUFFER_CAPACITY),
        None,
    );
}

#[test]
fn cell_buffer_capacity_shrink_never_drops_below_initial_capacity() {
    assert_eq!(
        shrink_cell_buffer_capacity(
            INITIAL_CELL_BUFFER_CAPACITY,
            1,
            INITIAL_CELL_BUFFER_CAPACITY
        ),
        None
    );
}

#[test]
fn initial_capacity_scales_with_viewport() {
    assert_eq!(
        initial_cell_buffer_capacity(0, 0),
        INITIAL_CELL_BUFFER_CAPACITY
    );
    assert_eq!(
        initial_cell_buffer_capacity(3840, 2160),
        next_cell_buffer_capacity(
            INITIAL_CELL_BUFFER_CAPACITY,
            (3840usize / CELL_WIDTH) * (2160usize / CELL_HEIGHT),
        )
    );
}

#[test]
fn render_frame_returns_backend_unavailable_when_uninitialized() {
    let mut renderer = GpuRenderer::default();
    let terminal = TerminalState::new(80, 24, 100);
    let dirty = vec![true; 24];
    assert_eq!(
        renderer.render_frame(&terminal, &dirty, 0, true, 0, u32::MAX, u32::MAX),
        Err(GpuRenderError::BackendUnavailable)
    );
}

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
    let result = color_to_u32(Color::Default, (0xFF, 0x80, 0x40));
    assert_eq!(result, 0xFF8040);
}

#[test]
fn color_to_u32_rgb() {
    let result = color_to_u32(Color::Rgb(0x12, 0x34, 0x56), (0, 0, 0));
    assert_eq!(result, 0x123456);
}

#[test]
fn color_to_u32_indexed() {
    let result = color_to_u32(Color::Indexed(1), (0, 0, 0));
    assert_eq!(result, ANSI_PALETTE[1]);
}

#[test]
fn write_glyph_to_atlas_places_data_correctly() {
    let cw = ATLAS_GLYPH_WIDTH as usize;
    let ch = ATLAS_GLYPH_HEIGHT as usize;
    let mut atlas_data = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize];
    let mut cell_buf = vec![0u8; cw * ch];
    cell_buf[0] = 200;
    write_glyph_to_atlas(&mut atlas_data, 1, &cell_buf);
    let expected_idx = cw;
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
    assert_eq!(std::mem::size_of::<GridUniforms>(), 64);
    assert_eq!(std::mem::align_of::<GridUniforms>(), 4);
}

#[test]
fn attr_flags_do_not_overlap_atlas_index() {
    assert_eq!(ATTR_BOLD & 0xFFFF, 0);
    assert_eq!(ATTR_ITALIC & 0xFFFF, 0);
    assert_eq!(ATTR_UNDERLINE & 0xFFFF, 0);
    assert_eq!(ATTR_STRIKETHROUGH & 0xFFFF, 0);
    assert_eq!(ATTR_DIM & 0xFFFF, 0);
    assert_eq!(ATTR_INVERSE & 0xFFFF, 0);
    let all_flags =
        ATTR_BOLD | ATTR_ITALIC | ATTR_UNDERLINE | ATTR_STRIKETHROUGH | ATTR_DIM | ATTR_INVERSE;
    assert_eq!(all_flags.count_ones(), 6);
}

#[test]
fn selection_none_sentinel_is_u32_max() {
    assert_eq!(SELECTION_NONE, u32::MAX);
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
        GpuFailureKind::OutOfMemory
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

#[test]
fn pack_cell_flags_sets_double_underline_bit() {
    let attrs = Attrs {
        double_underline: true,
        ..Attrs::default()
    };
    let flags = pack_cell_flags(42, &attrs);
    assert_eq!(flags & 0xFFFF, 42, "lower 16 bits = slot");
    assert_ne!(
        flags & ATTR_DOUBLE_UNDERLINE,
        0,
        "double_underline bit must be set"
    );
    assert_eq!(
        flags & ATTR_UNDERLINE,
        0,
        "single underline must not be set"
    );
}

#[test]
fn pack_cell_flags_sets_overline_bit() {
    let attrs = Attrs {
        overline: true,
        ..Attrs::default()
    };
    let flags = pack_cell_flags(0, &attrs);
    assert_ne!(flags & ATTR_OVERLINE, 0, "overline bit must be set");
}

#[test]
fn pack_cell_flags_wide_and_continuation_are_independent_of_attrs() {
    // ATTR_WIDE and ATTR_CONTINUATION are set per-cell in write_row_instances,
    // not by pack_cell_flags. Verify attrs alone don't set them.
    let all_attrs = Attrs {
        bold: true,
        italic: true,
        underline: true,
        strikethrough: true,
        dim: true,
        inverse: true,
        blink: true,
        hidden: true,
        double_underline: true,
        overline: true,
        ..Attrs::default()
    };
    let flags = pack_cell_flags(0, &all_attrs);
    assert_eq!(flags & ATTR_WIDE, 0, "ATTR_WIDE must not be set by attrs");
    assert_eq!(
        flags & ATTR_CONTINUATION,
        0,
        "ATTR_CONTINUATION must not be set by attrs"
    );
}

#[test]
fn pack_cell_flags_all_bits_combined() {
    let attrs = Attrs {
        bold: true,
        italic: true,
        underline: true,
        strikethrough: true,
        dim: true,
        inverse: true,
        blink: true,
        hidden: true,
        double_underline: true,
        overline: true,
        ..Attrs::default()
    };
    let flags = pack_cell_flags(0xFFFF, &attrs);
    assert_ne!(flags & ATTR_BOLD, 0);
    assert_ne!(flags & ATTR_ITALIC, 0);
    assert_ne!(flags & ATTR_UNDERLINE, 0);
    assert_ne!(flags & ATTR_STRIKETHROUGH, 0);
    assert_ne!(flags & ATTR_DIM, 0);
    assert_ne!(flags & ATTR_INVERSE, 0);
    assert_ne!(flags & ATTR_BLINK, 0);
    assert_ne!(flags & ATTR_HIDDEN, 0);
    assert_ne!(flags & ATTR_DOUBLE_UNDERLINE, 0);
    assert_ne!(flags & ATTR_OVERLINE, 0);
    assert_eq!(flags & 0xFFFF, 0xFFFF, "slot bits preserved");
}
