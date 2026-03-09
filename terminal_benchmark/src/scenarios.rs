use crate::cli::{Cli, ScaleArg, ScenarioArg};
use crate::data::{Workload, WorkloadScale};
use crate::metrics::IterationStats;
use crate::report::{BenchmarkSuiteReport, ScenarioReport};
use anyhow::Result;
use rldyourterm_font::GlyphCache;
use rldyourterm_render_cpu::{CpuRenderer, render_terminal_buffer};
use rldyourterm_services::terminal::{
    Attrs, CELL_HEIGHT, CELL_WIDTH, Grid, MAX_FEED_BYTES_PER_CALL, Parser, TerminalState,
};
use std::hint::black_box;
use std::time::{Duration, Instant};

const PIXEL_BUFFER_MARGIN_COLS: usize = 4;
const PIXEL_BUFFER_MARGIN_ROWS: usize = 2;

pub fn run_suite(cli: &Cli) -> Result<BenchmarkSuiteReport> {
    let scale = WorkloadScale::from_arg(cli.scale);
    let workload = Workload::generate(cli.cols, scale);
    let scenarios = selected_scenarios(cli.scenario);
    let mut results = Vec::with_capacity(scenarios.len());

    for scenario in scenarios {
        warmup_scenario(scenario, cli, &workload)?;
        results.push(run_measured_scenario(scenario, cli, &workload)?);
    }

    Ok(BenchmarkSuiteReport {
        benchmark_tool: "terminal-benchmark",
        scenario_selection: cli.scenario.as_str().to_owned(),
        scale: scale_name(cli.scale),
        warmup_iterations: cli.warmup_iterations,
        measured_iterations: cli.iterations,
        cols: cli.cols,
        rows: cli.rows,
        chunk_bytes: canonical_chunk_bytes(cli.chunk_bytes),
        scrollback_cap: cli.scrollback_cap,
        workload: workload.summary(),
        results,
    })
}

fn selected_scenarios(selection: ScenarioArg) -> Vec<ScenarioArg> {
    match selection {
        ScenarioArg::All => vec![
            ScenarioArg::CoreIngestBurst,
            ScenarioArg::CoreScrollbackFlood,
            ScenarioArg::CoreParserThroughput,
            ScenarioArg::CoreGridScroll,
            ScenarioArg::CpuRenderFull,
            ScenarioArg::CpuRenderDelta,
            ScenarioArg::CpuCycleIngestRenderDelta,
            ScenarioArg::CpuPixelRasterDelta,
        ],
        one => vec![one],
    }
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
        scenario: scenario_name(scenario),
        description: scenario_description(scenario),
        primary_unit_label: scenario_primary_unit(scenario),
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

fn scenario_name(scenario: ScenarioArg) -> &'static str {
    match scenario {
        ScenarioArg::All => "all",
        ScenarioArg::CoreIngestBurst => "core-ingest-burst",
        ScenarioArg::CoreScrollbackFlood => "core-scrollback-flood",
        ScenarioArg::CoreParserThroughput => "core-parser-throughput",
        ScenarioArg::CoreGridScroll => "core-grid-scroll",
        ScenarioArg::CpuRenderFull => "cpu-render-full",
        ScenarioArg::CpuRenderDelta => "cpu-render-delta",
        ScenarioArg::CpuCycleIngestRenderDelta => "cpu-cycle-ingest-render-delta",
        ScenarioArg::CpuPixelRasterDelta => "cpu-pixel-raster-delta",
    }
}

fn scenario_description(scenario: ScenarioArg) -> &'static str {
    match scenario {
        ScenarioArg::All => "all scenarios",
        ScenarioArg::CoreIngestBurst => "Chunked ANSI-heavy AI output ingest through TerminalState",
        ScenarioArg::CoreScrollbackFlood => {
            "Deep scrollback ingest and trimming pressure through TerminalState"
        }
        ScenarioArg::CoreParserThroughput => {
            "Isolated ANSI parser throughput without grid dispatch"
        }
        ScenarioArg::CoreGridScroll => {
            "Grid scroll_up_discard throughput with copy_within and dirty tracking"
        }
        ScenarioArg::CpuRenderFull => "Canonical full-frame CPU render snapshot",
        ScenarioArg::CpuRenderDelta => "Canonical dirty-row CPU delta render",
        ScenarioArg::CpuCycleIngestRenderDelta => "Steady-state ingest plus CPU delta render cycle",
        ScenarioArg::CpuPixelRasterDelta => {
            "Headless CPU pixel raster path over a dirty terminal buffer"
        }
    }
}

fn scenario_primary_unit(scenario: ScenarioArg) -> &'static str {
    match scenario {
        ScenarioArg::All => "n/a",
        ScenarioArg::CoreIngestBurst
        | ScenarioArg::CoreScrollbackFlood
        | ScenarioArg::CoreParserThroughput => "bytes",
        ScenarioArg::CoreGridScroll => "scrolls",
        ScenarioArg::CpuRenderFull
        | ScenarioArg::CpuRenderDelta
        | ScenarioArg::CpuCycleIngestRenderDelta => "cells",
        ScenarioArg::CpuPixelRasterDelta => "pixels",
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
    use super::{canonical_chunk_bytes, selected_scenarios};
    use crate::cli::ScenarioArg;
    use rldyourterm_services::terminal::MAX_FEED_BYTES_PER_CALL;

    #[test]
    fn all_selection_expands_to_canonical_suite() {
        let scenarios = selected_scenarios(ScenarioArg::All);
        assert_eq!(scenarios.len(), 8);
        assert!(scenarios.contains(&ScenarioArg::CoreParserThroughput));
        assert!(scenarios.contains(&ScenarioArg::CoreGridScroll));
        assert!(scenarios.contains(&ScenarioArg::CpuPixelRasterDelta));
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
