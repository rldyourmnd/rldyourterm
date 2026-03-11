// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::{Cli, ScaleArg, ScenarioArg};
use crate::coverage::benchmark_coverage_summary;
use crate::data::{SurfacePolicyCase, Workload, WorkloadScale};
use crate::metrics::IterationStats;
use crate::report::{BenchmarkSuiteReport, ScenarioReport};
use crate::scenario_registry::{
    BENCHMARK_SUITE_NAME, descriptor, selected_scenario_names,
    selected_scenarios as registry_selected_scenarios,
};
use anyhow::Result;
use rldyourterm_font::{GlyphCache, rasterize_for_atlas};
use rldyourterm_render_cpu::{CpuRenderer, render_terminal_buffer};
use rldyourterm_render_gpu::{
    SurfaceRecoveryPolicy, SurfaceRuntimeState, update_frame_latency_hint, update_surface_extent,
    validate_surface_configuration,
};
use rldyourterm_services::render_mode::RenderMode;
use rldyourterm_services::session::{SessionController, SessionTransition};
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Grid, MAX_FEED_BYTES_PER_CALL, Parser, TerminalState,
};
use rldyourterm_settings::SettingsService;
use rldyourterm_shell_integration::plan_shell_launch;
use rldyourterm_ui::{UiBootstrapConfig, UiBootstrapHooks, UiRuntime};
use std::hint::black_box;
use std::time::{Duration, Instant};

const PIXEL_BUFFER_MARGIN_COLS: usize = 4;
const PIXEL_BUFFER_MARGIN_ROWS: usize = 2;
const SURFACE_BASE_WIDTH: u32 = 960;
const SURFACE_BASE_HEIGHT: u32 = 540;
const SURFACE_BASE_FRAME_LATENCY: u32 = 2;
const FONT_CACHE_MAX_ENTRIES: usize = 256;

pub fn run_suite(cli: &Cli) -> Result<BenchmarkSuiteReport> {
    let scale = WorkloadScale::from_arg(cli.scale);
    let workload = Workload::generate(cli.cols, scale);
    let scenarios = registry_selected_scenarios(cli.scenario);
    let mut results = Vec::with_capacity(scenarios.len());

    for scenario in scenarios {
        warmup_scenario(scenario, cli, &workload)?;
        results.push(run_measured_scenario(scenario, cli, &workload)?);
    }

    Ok(BenchmarkSuiteReport {
        benchmark_tool: "terminal-benchmark",
        suite: BENCHMARK_SUITE_NAME,
        scenario_selection: cli.scenario.as_str().to_owned(),
        selected_scenarios: selected_scenario_names(cli.scenario),
        scale: scale_name(cli.scale),
        warmup_iterations: cli.warmup_iterations,
        measured_iterations: cli.iterations,
        cols: cli.cols,
        rows: cli.rows,
        chunk_bytes: canonical_chunk_bytes(cli.chunk_bytes),
        scrollback_cap: cli.scrollback_cap,
        workload: workload.summary(),
        coverage: benchmark_coverage_summary(),
        results,
    })
}

fn warmup_scenario(scenario: ScenarioArg, cli: &Cli, workload: &Workload) -> Result<()> {
    for _ in 0..cli.warmup_iterations {
        let _ = run_iteration(scenario, cli, workload)?;
    }
    Ok(())
}

fn run_measured_scenario(
    scenario: ScenarioArg,
    cli: &Cli,
    workload: &Workload,
) -> Result<ScenarioReport> {
    let metadata = descriptor(scenario);
    let mut durations = Vec::with_capacity(cli.iterations as usize);
    let mut primary_units = 0u64;
    let mut byte_units = 0u64;
    let mut notes: Vec<String> = Vec::new();

    for _ in 0..cli.iterations {
        let result = run_iteration(scenario, cli, workload)?;
        durations.push(result.elapsed);
        primary_units = result.primary_units;
        byte_units = result.byte_units;
        if notes.is_empty() {
            notes = result.notes;
        }
    }

    let stats = IterationStats::from_durations(&durations);
    let mean_seconds = stats.mean_nanos as f64 / 1_000_000_000.0;
    Ok(ScenarioReport {
        scenario: metadata.name,
        layer: metadata.layer,
        benchmark_kind: metadata.benchmark_kind,
        description: metadata.description,
        primary_unit_label: metadata.primary_unit_label,
        primary_units_per_iteration: primary_units,
        byte_units_per_iteration: byte_units,
        stats,
        primary_units_per_second: if mean_seconds > 0.0 {
            primary_units as f64 / mean_seconds
        } else {
            0.0
        },
        bytes_per_second: if mean_seconds > 0.0 {
            byte_units as f64 / mean_seconds
        } else {
            0.0
        },
        notes,
    })
}

struct IterationOutcome {
    elapsed: Duration,
    primary_units: u64,
    byte_units: u64,
    notes: Vec<String>,
}

fn run_iteration(
    scenario: ScenarioArg,
    cli: &Cli,
    workload: &Workload,
) -> Result<IterationOutcome> {
    match scenario {
        ScenarioArg::All => unreachable!("all is expanded before execution"),
        ScenarioArg::CoreIngestBurst => bench_core_ingest_burst(cli, workload),
        ScenarioArg::CoreScrollbackFlood => bench_core_scrollback_flood(cli, workload),
        ScenarioArg::CoreParserThroughput => bench_core_parser_throughput(cli, workload),
        ScenarioArg::CoreGridScroll => bench_core_grid_scroll(cli),
        ScenarioArg::ServiceSessionRuntimeCycle => bench_service_session_runtime_cycle(workload),
        ScenarioArg::UiCommandCycle => bench_ui_command_cycle(workload),
        ScenarioArg::SettingsApplyCycle => bench_settings_apply_cycle(workload),
        ScenarioArg::ShellResolutionPlan => bench_shell_resolution_plan(workload),
        ScenarioArg::FontCacheMixedRaster => bench_font_cache_mixed_raster(workload),
        ScenarioArg::GpuSurfacePolicy => bench_gpu_surface_policy(workload),
        ScenarioArg::CpuRenderFull => bench_cpu_render_full(cli, workload),
        ScenarioArg::CpuRenderDelta => bench_cpu_render_delta(cli, workload),
        ScenarioArg::CpuCycleIngestRenderDelta => {
            bench_cpu_cycle_ingest_render_delta(cli, workload)
        }
        ScenarioArg::CpuPixelRasterDelta => bench_cpu_pixel_raster_delta(cli, workload),
    }
}

fn bench_core_ingest_burst(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut state = TerminalState::new(cli.cols, cli.rows, cli.scrollback_cap);
    let mut responses = Vec::new();
    let chunk_bytes = canonical_chunk_bytes(cli.chunk_bytes);
    let start = Instant::now();
    feed_bytes_in_chunks(&mut state, &workload.ai_burst, chunk_bytes, &mut responses);
    let elapsed = start.elapsed();
    black_box(state.window_title());
    black_box(state.cursor);
    black_box(responses.len());
    Ok(IterationOutcome {
        elapsed,
        primary_units: workload.ai_burst.len() as u64,
        byte_units: workload.ai_burst.len() as u64,
        notes: vec![format!(
            "chunks={} responses={}",
            chunk_count(&workload.ai_burst, chunk_bytes),
            responses.len()
        )],
    })
}

fn bench_core_scrollback_flood(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut state = TerminalState::new(cli.cols, cli.rows, cli.scrollback_cap);
    let mut responses = Vec::new();
    let chunk_bytes = canonical_chunk_bytes(cli.chunk_bytes);
    let start = Instant::now();
    feed_bytes_in_chunks(
        &mut state,
        &workload.scrollback_flood,
        chunk_bytes,
        &mut responses,
    );
    let elapsed = start.elapsed();
    let scrollback_lines = state.scrollback.len();
    black_box(scrollback_lines);
    black_box(state.cursor);
    Ok(IterationOutcome {
        elapsed,
        primary_units: workload.scrollback_flood.len() as u64,
        byte_units: workload.scrollback_flood.len() as u64,
        notes: vec![format!(
            "scrollback_lines={} capped_at={}",
            scrollback_lines, cli.scrollback_cap
        )],
    })
}

fn bench_core_parser_throughput(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut parser = Parser::default();
    let mut actions = Vec::new();
    let chunk_bytes = canonical_chunk_bytes(cli.chunk_bytes);
    let start = Instant::now();
    for chunk in workload.ai_burst.chunks(chunk_bytes) {
        parser.feed_into(chunk, &mut actions);
        black_box(actions.len());
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: workload.ai_burst.len() as u64,
        byte_units: workload.ai_burst.len() as u64,
        notes: vec![format!(
            "chunks={} last_action_count={}",
            chunk_count(&workload.ai_burst, chunk_bytes),
            actions.len()
        )],
    })
}

fn bench_core_grid_scroll(cli: &Cli) -> Result<IterationOutcome> {
    let mut grid = Grid::new(cli.cols, cli.rows);
    let rows = cli.rows as usize;
    let cols = cli.cols as usize;
    for row in 0..rows {
        for col in 0..cols {
            let _ = grid.put_char(row as u16, col as u16, 'X', Attrs::default());
        }
    }
    let scroll_iterations = rows.saturating_mul(200);
    let start = Instant::now();
    for _ in 0..scroll_iterations {
        grid.scroll_up_discard(1);
        grid.clear_dirty_rows();
    }
    let elapsed = start.elapsed();
    black_box(grid.height());
    Ok(IterationOutcome {
        elapsed,
        primary_units: scroll_iterations as u64,
        byte_units: (scroll_iterations * cols * std::mem::size_of::<u32>()) as u64,
        notes: vec![format!(
            "scroll_iterations={} grid={}x{}",
            scroll_iterations, cli.cols, cli.rows
        )],
    })
}

fn bench_service_session_runtime_cycle(workload: &Workload) -> Result<IterationOutcome> {
    let mut transitions = 0u64;
    let start = Instant::now();
    for _ in 0..workload.session_cycles {
        let mut controller = SessionController::with_recoverable_budget(3);
        consume_transition(controller.mark_running()?, &mut transitions);
        for boundary in &workload.session_boundaries {
            consume_transition(
                controller.handle_boundary_failure(*boundary)?,
                &mut transitions,
            );
            if controller.state() == rldyourterm_services::session::SessionState::Degraded {
                consume_transition(controller.mark_running()?, &mut transitions);
            }
        }
        consume_transition(controller.request_stop()?, &mut transitions);
        consume_transition(controller.mark_stopped()?, &mut transitions);
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: transitions,
        byte_units: transitions * std::mem::size_of::<SessionTransition>() as u64,
        notes: vec![
            format!("cycles={}", workload.session_cycles),
            format!("boundaries_per_cycle={}", workload.session_boundaries.len()),
            "recoverable_budget=3".to_string(),
        ],
    })
}

fn bench_ui_command_cycle(workload: &Workload) -> Result<IterationOutcome> {
    let mut commands_handled = 0u64;
    let mut receipts_observed = 0usize;
    let start = Instant::now();
    for batch_index in 0..workload.ui_batch_repetitions {
        let commands =
            &workload.ui_command_batches[batch_index % workload.ui_command_batches.len()];
        let hooks = UiBootstrapHooks::from_commands(commands.iter().copied());
        let (runtime, receipts) = UiRuntime::bootstrap_with_hooks(
            UiBootstrapConfig::single_window(RenderMode::Auto, 60_000),
            &hooks,
        )?;
        black_box(runtime.state());
        black_box(runtime.active_render_path());
        black_box(runtime.cadence().refresh_rate_millihz);
        commands_handled += receipts.len() as u64;
        receipts_observed += receipts.len();
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: commands_handled,
        byte_units: commands_handled
            * std::mem::size_of::<rldyourterm_ui::UiRuntimeCommand>() as u64,
        notes: vec![
            format!("batch_templates={}", workload.ui_command_batches.len()),
            format!("batch_repetitions={}", workload.ui_batch_repetitions),
            format!("receipts={receipts_observed}"),
        ],
    })
}

fn bench_settings_apply_cycle(workload: &Workload) -> Result<IterationOutcome> {
    let mut commands = 0u64;
    let mut bytes = 0u64;
    let start = Instant::now();
    for _ in 0..workload.settings_rounds {
        let mut service = SettingsService::default();
        for input in &workload.settings_palette_inputs {
            let outcome = service.apply_palette_command(input);
            black_box(&outcome);
            commands += 1;
            bytes += input.len() as u64;
        }
        let profile = service.export_runtime_profile_state();
        let profile_outcome = service.apply_runtime_profile_state(profile.clone());
        black_box(profile);
        black_box(profile_outcome);
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: commands,
        byte_units: bytes,
        notes: vec![
            format!(
                "inputs_per_round={}",
                workload.settings_palette_inputs.len()
            ),
            format!("rounds={}", workload.settings_rounds),
            "runtime_profile_roundtrip=1".to_string(),
        ],
    })
}

fn bench_shell_resolution_plan(workload: &Workload) -> Result<IterationOutcome> {
    let mut cases = 0u64;
    let start = Instant::now();
    for _ in 0..workload.shell_rounds {
        for shell_case in &workload.shell_cases {
            let resolution = plan_shell_launch(shell_case.requested, shell_case.availability);
            black_box(&resolution);
            cases += 1;
        }
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: cases,
        byte_units: 0,
        notes: vec![
            format!("cases_per_round={}", workload.shell_cases.len()),
            format!("rounds={}", workload.shell_rounds),
        ],
    })
}

fn bench_font_cache_mixed_raster(workload: &Workload) -> Result<IterationOutcome> {
    let mut glyphs = 0u64;
    let mut bytes = 0u64;
    let mut cache = GlyphCache::new_with_max_entries(
        CELL_WIDTH as u16,
        CELL_HEIGHT as u16,
        FONT_CACHE_MAX_ENTRIES,
    );
    let start = Instant::now();
    for _ in 0..workload.font_passes {
        for &ch in &workload.font_glyphs {
            black_box(cache.has_glyph(ch));
            let cell = rasterize_for_atlas(&mut cache, ch);
            bytes += cell.len() as u64;
            glyphs += 1;
            black_box(cell.len());
        }
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: glyphs,
        byte_units: bytes,
        notes: vec![
            format!("glyphs_per_pass={}", workload.font_glyphs.len()),
            format!("passes={}", workload.font_passes),
            format!("cache_max_entries={FONT_CACHE_MAX_ENTRIES}"),
        ],
    })
}

fn bench_gpu_surface_policy(workload: &Workload) -> Result<IterationOutcome> {
    let policy = SurfaceRecoveryPolicy::default();
    let mut decisions = 0u64;
    let mut config = base_surface_configuration();
    let mut state = SurfaceRuntimeState::default();
    let start = Instant::now();
    for _ in 0..workload.surface_rounds {
        for &case in &workload.surface_policy_cases {
            match case {
                SurfacePolicyCase::AcquireTimeout => {
                    let decision =
                        policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Timeout);
                    black_box(decision.action);
                }
                SurfacePolicyCase::AcquireOutdated => {
                    let decision =
                        policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Outdated);
                    black_box(decision.action);
                }
                SurfacePolicyCase::AcquireLost => {
                    let decision =
                        policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Lost);
                    black_box(decision.action);
                }
                SurfacePolicyCase::AcquireOutOfMemory => {
                    let decision = policy
                        .on_surface_acquire_error(&mut state, wgpu::SurfaceError::OutOfMemory);
                    black_box(decision.action);
                }
                SurfacePolicyCase::AcquireOther => {
                    let decision =
                        policy.on_surface_acquire_error(&mut state, wgpu::SurfaceError::Other);
                    black_box(decision.action);
                }
                SurfacePolicyCase::ConfigureZeroWidth => {
                    let invalid = wgpu::SurfaceConfiguration {
                        width: 0,
                        ..base_surface_configuration()
                    };
                    let error = validate_surface_configuration(&invalid).expect_err("zero width");
                    let decision = policy.on_surface_configuration_error(&mut state, error);
                    black_box(decision.action);
                }
                SurfacePolicyCase::ConfigureZeroHeight => {
                    let invalid = wgpu::SurfaceConfiguration {
                        height: 0,
                        ..base_surface_configuration()
                    };
                    let error = validate_surface_configuration(&invalid).expect_err("zero height");
                    let decision = policy.on_surface_configuration_error(&mut state, error);
                    black_box(decision.action);
                }
                SurfacePolicyCase::ExtentNominal {
                    width,
                    height,
                    max_texture_dimension_2d,
                }
                | SurfacePolicyCase::ExtentClamped {
                    width,
                    height,
                    max_texture_dimension_2d,
                } => {
                    update_surface_extent(&mut config, width, height, max_texture_dimension_2d)?;
                    black_box(config.width);
                    black_box(config.height);
                    policy.on_reconfigure_success(&mut state);
                }
                SurfacePolicyCase::FrameLatency {
                    desired_maximum_frame_latency,
                } => {
                    update_frame_latency_hint(&mut config, desired_maximum_frame_latency);
                    black_box(config.desired_maximum_frame_latency);
                    policy.on_acquire_success(&mut state);
                }
            }
            decisions += 1;
        }
    }
    let elapsed = start.elapsed();
    Ok(IterationOutcome {
        elapsed,
        primary_units: decisions,
        byte_units: decisions * std::mem::size_of::<wgpu::SurfaceConfiguration>() as u64,
        notes: vec![
            format!("cases_per_round={}", workload.surface_policy_cases.len()),
            format!("rounds={}", workload.surface_rounds),
            format!(
                "surface_failures=acquire:{} reconfigure:{}",
                state.consecutive_acquire_failures(),
                state.consecutive_reconfigure_failures()
            ),
        ],
    })
}

fn bench_cpu_render_full(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let state = seeded_terminal_state(cli, workload)?;
    let renderer = CpuRenderer::default();
    let start = Instant::now();
    let frame = renderer.render_full(black_box(&state));
    let elapsed = start.elapsed();
    black_box(frame.rows.len());
    black_box(frame.stats.rendered_bytes);
    Ok(IterationOutcome {
        elapsed,
        primary_units: frame.stats.rendered_cells as u64,
        byte_units: frame.stats.rendered_bytes as u64,
        notes: vec![format!(
            "rendered_rows={} full_redraw={}",
            frame.stats.rendered_rows, frame.full_redraw
        )],
    })
}

fn bench_cpu_render_delta(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut state = seeded_terminal_state(cli, workload)?;
    let renderer = CpuRenderer::default();
    let _ = renderer.render_delta(&mut state);
    feed_bytes_in_chunks(
        &mut state,
        &workload.delta_batches[0],
        canonical_chunk_bytes(cli.chunk_bytes),
        &mut Vec::new(),
    );
    let start = Instant::now();
    let frame = renderer.render_delta(black_box(&mut state));
    let elapsed = start.elapsed();
    black_box(frame.rows.len());
    black_box(frame.stats.rendered_bytes);
    Ok(IterationOutcome {
        elapsed,
        primary_units: frame.stats.rendered_cells as u64,
        byte_units: frame.stats.rendered_bytes as u64,
        notes: vec![format!(
            "dirty_rows={} fallback_rows={}",
            frame.stats.rendered_rows, frame.stats.fallback_rows
        )],
    })
}

fn bench_cpu_cycle_ingest_render_delta(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut state = seeded_terminal_state(cli, workload)?;
    let renderer = CpuRenderer::default();
    let _ = renderer.render_delta(&mut state);
    let chunk_bytes = canonical_chunk_bytes(cli.chunk_bytes);
    let mut responses = Vec::new();
    let start = Instant::now();
    feed_bytes_in_chunks(
        &mut state,
        &workload.delta_batches[1],
        chunk_bytes,
        &mut responses,
    );
    let frame = renderer.render_delta(&mut state);
    let elapsed = start.elapsed();
    black_box(frame.rows.len());
    black_box(responses.len());
    Ok(IterationOutcome {
        elapsed,
        primary_units: frame.stats.rendered_cells as u64,
        byte_units: workload.delta_batches[1].len() as u64,
        notes: vec![format!(
            "dirty_rows={} chunk_count={} responses={}",
            frame.stats.rendered_rows,
            chunk_count(&workload.delta_batches[1], chunk_bytes),
            responses.len()
        )],
    })
}

fn bench_cpu_pixel_raster_delta(cli: &Cli, workload: &Workload) -> Result<IterationOutcome> {
    let mut state = seeded_terminal_state(cli, workload)?;
    let renderer = CpuRenderer::default();
    let _ = renderer.render_delta(&mut state);
    feed_bytes_in_chunks(
        &mut state,
        &workload.delta_batches[2],
        canonical_chunk_bytes(cli.chunk_bytes),
        &mut Vec::new(),
    );

    let visible_cols = usize::from(cli.cols).saturating_add(PIXEL_BUFFER_MARGIN_COLS);
    let visible_rows = usize::from(cli.rows).saturating_add(PIXEL_BUFFER_MARGIN_ROWS);
    let width = visible_cols * CELL_WIDTH;
    let height = visible_rows * CELL_HEIGHT;
    let mut buffer = vec![0u32; width * height];
    let mut glyph_cache = GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16);
    let mut dirty_rows_scratch = Vec::new();
    let start = Instant::now();
    render_terminal_buffer(
        &mut buffer,
        width,
        height,
        &mut state,
        &mut glyph_cache,
        None,
        &mut dirty_rows_scratch,
        true,
        0,
        u32::MAX,
        u32::MAX,
    );
    let elapsed = start.elapsed();
    black_box(buffer[0]);
    black_box(dirty_rows_scratch.len());
    Ok(IterationOutcome {
        elapsed,
        primary_units: (width * height) as u64,
        byte_units: (buffer.len() * std::mem::size_of::<u32>()) as u64,
        notes: vec![format!(
            "pixel_buffer={}x{} dirty_rows={}",
            width,
            height,
            dirty_rows_scratch.len()
        )],
    })
}

fn seeded_terminal_state(cli: &Cli, workload: &Workload) -> Result<TerminalState> {
    let mut state = TerminalState::new(cli.cols, cli.rows, cli.scrollback_cap);
    feed_bytes_in_chunks(
        &mut state,
        &workload.render_seed,
        canonical_chunk_bytes(cli.chunk_bytes),
        &mut Vec::new(),
    );
    Ok(state)
}

fn feed_bytes_in_chunks(
    state: &mut TerminalState,
    bytes: &[u8],
    chunk_bytes: usize,
    responses: &mut Vec<Vec<u8>>,
) {
    for chunk in bytes.chunks(chunk_bytes) {
        state.feed_terminal_responses_into(chunk, responses);
        black_box(responses.len());
    }
}

fn chunk_count(bytes: &[u8], chunk_bytes: usize) -> usize {
    bytes.len().div_ceil(chunk_bytes)
}

fn canonical_chunk_bytes(requested: usize) -> usize {
    requested.clamp(1, MAX_FEED_BYTES_PER_CALL)
}

fn consume_transition(transition: SessionTransition, transitions: &mut u64) {
    black_box(transition.sequence);
    *transitions += 1;
}

fn base_surface_configuration() -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        width: SURFACE_BASE_WIDTH,
        height: SURFACE_BASE_HEIGHT,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: SURFACE_BASE_FRAME_LATENCY,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    }
}

fn scale_name(scale: ScaleArg) -> &'static str {
    match scale {
        ScaleArg::Quick => "quick",
        ScaleArg::Standard => "standard",
        ScaleArg::Stress => "stress",
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_chunk_bytes;
    use crate::cli::ScenarioArg;
    use crate::scenario_registry::{descriptor, selected_scenarios};
    use rldyourterm_services::terminal::MAX_FEED_BYTES_PER_CALL;

    #[test]
    fn all_selection_expands_to_canonical_suite() {
        let scenarios = selected_scenarios(ScenarioArg::All);
        assert_eq!(scenarios.len(), 14);
        assert!(scenarios.contains(&ScenarioArg::ServiceSessionRuntimeCycle));
        assert!(scenarios.contains(&ScenarioArg::GpuSurfacePolicy));
        assert_eq!(descriptor(ScenarioArg::UiCommandCycle).layer, "ui");
    }

    #[test]
    fn chunk_bytes_are_clamped_to_core_ingest_cap() {
        assert_eq!(canonical_chunk_bytes(0), 1);
        assert_eq!(
            canonical_chunk_bytes(MAX_FEED_BYTES_PER_CALL * 2),
            MAX_FEED_BYTES_PER_CALL
        );
    }
}
