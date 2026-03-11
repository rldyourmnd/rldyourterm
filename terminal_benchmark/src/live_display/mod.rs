// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod scenario_registry;

use crate::cli::{Cli, ScaleArg, ScenarioArg};
use crate::fixtures::scale_name;
use crate::metrics::IterationStats;
use crate::report::{
    LiveDisplayBenchmarkSuiteReport, LiveDisplayCpuPhaseStats, LiveDisplayEnvironmentReport,
    LiveDisplayScenarioReport, LiveDisplayWorkloadSummary,
};
use anyhow::{Context, Result, bail};
use rldyourterm_render_cpu::render_terminal_buffer;
use rldyourterm_render_gpu::{GpuRenderError, GpuRenderer, SurfaceRecoveryPolicy};
use rldyourterm_services::terminal::{CELL_HEIGHT, CELL_WIDTH, TerminalState};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::data::Workload;
use crate::fixtures::seeded_terminal_state;
use crate::live_display::scenario_registry::{
    BENCHMARK_SUITE_NAME, descriptor, scenario_belongs_to_suite, selected_scenario_names,
    selected_scenarios,
};
use rldyourterm_font::GlyphCache;
use softbuffer::{Context as SoftbufferContext, Surface as SoftbufferSurface};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayBackend {
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenarioKind {
    StartupFirstFrame,
    SteadyRedraw,
    ResizeCycle,
}

#[derive(Debug, Clone)]
struct LiveDisplayWorkload {
    startup_runs_per_iteration: u32,
    steady_frames_per_iteration: u32,
    resize_cycles_per_iteration: u32,
    requested_width: u32,
    requested_height: u32,
    resize_targets: Vec<PhysicalSize<u32>>,
}

#[derive(Debug, Clone)]
struct LiveDisplayIterationOutcome {
    elapsed: Duration,
    primary_units: u64,
    redraws_observed: u32,
    resize_cycles_observed: u32,
    cpu_phase_totals: Option<LiveDisplayCpuPhaseTotals>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct LiveDisplayCpuPhaseTotals {
    buffer_acquire: Duration,
    raster: Duration,
    present: Duration,
}

pub fn run_suite(cli: &Cli) -> Result<LiveDisplayBenchmarkSuiteReport> {
    if !scenario_belongs_to_suite(cli.scenario) {
        bail!(
            "--suite live-display does not support scenario {}",
            cli.scenario.as_str()
        );
    }

    let workload = LiveDisplayWorkload::from_cli(cli);
    let scenarios = selected_scenarios(cli.scenario);
    let mut event_loop = EventLoop::new()?;
    let mut results = Vec::with_capacity(scenarios.len());

    for scenario in scenarios {
        results.push(run_measured_scenario(
            &mut event_loop,
            scenario,
            cli,
            &workload,
        )?);
    }

    Ok(LiveDisplayBenchmarkSuiteReport {
        benchmark_tool: "terminal-benchmark",
        suite: BENCHMARK_SUITE_NAME,
        scenario_selection: cli.scenario.as_str().to_owned(),
        selected_scenarios: selected_scenario_names(cli.scenario),
        scale: scale_name(cli.scale),
        warmup_iterations: cli.warmup_iterations,
        measured_iterations: cli.iterations,
        cols: cli.cols,
        rows: cli.rows,
        environment: LiveDisplayEnvironmentReport {
            kind: "live-display",
            window_runtime: "winit",
            gpu_runtime: "wgpu",
            cpu_present_runtime: "softbuffer",
            platform_dependent: true,
        },
        workload: LiveDisplayWorkloadSummary {
            startup_runs_per_iteration: workload.startup_runs_per_iteration,
            steady_frames_per_iteration: workload.steady_frames_per_iteration,
            resize_cycles_per_iteration: workload.resize_cycles_per_iteration,
            requested_width: workload.requested_width,
            requested_height: workload.requested_height,
            resize_targets: workload.resize_targets.len(),
        },
        results,
    })
}

impl LiveDisplayWorkload {
    fn from_cli(cli: &Cli) -> Self {
        let requested_width = u32::from(cli.cols)
            .saturating_mul(CELL_WIDTH as u32)
            .max(320);
        let requested_height = u32::from(cli.rows)
            .saturating_mul(CELL_HEIGHT as u32)
            .max(240);
        let (steady_frames_per_iteration, resize_cycles_per_iteration) = match cli.scale {
            ScaleArg::Quick => (12, 4),
            ScaleArg::Standard => (48, 8),
            ScaleArg::Stress => (120, 16),
        };
        let resize_targets = vec![
            PhysicalSize::new(requested_width, requested_height),
            PhysicalSize::new(
                requested_width.saturating_mul(3) / 4,
                requested_height.saturating_mul(3) / 4,
            ),
            PhysicalSize::new(
                requested_width.saturating_mul(5) / 4,
                requested_height.saturating_mul(5) / 4,
            ),
            PhysicalSize::new(requested_width.saturating_add(160), requested_height),
        ];
        Self {
            startup_runs_per_iteration: 1,
            steady_frames_per_iteration,
            resize_cycles_per_iteration,
            requested_width,
            requested_height,
            resize_targets,
        }
    }
}

fn run_measured_scenario(
    event_loop: &mut EventLoop<()>,
    scenario: ScenarioArg,
    cli: &Cli,
    workload: &LiveDisplayWorkload,
) -> Result<LiveDisplayScenarioReport> {
    let metadata = descriptor(scenario);

    for _ in 0..cli.warmup_iterations {
        let mut app = LiveDisplayApp::new(scenario, cli, workload);
        event_loop.run_app_on_demand(&mut app)?;
        app.into_result()?;
    }

    let mut durations = Vec::with_capacity(cli.iterations as usize);
    let mut primary_units = 0u64;
    let mut redraws_observed = 0u32;
    let mut resize_cycles_observed = 0u32;
    let mut cpu_buffer_acquire_durations = Vec::with_capacity(cli.iterations as usize);
    let mut cpu_raster_durations = Vec::with_capacity(cli.iterations as usize);
    let mut cpu_present_durations = Vec::with_capacity(cli.iterations as usize);
    let mut notes = Vec::new();

    for _ in 0..cli.iterations {
        let mut app = LiveDisplayApp::new(scenario, cli, workload);
        event_loop.run_app_on_demand(&mut app)?;
        let outcome = app.into_result()?;
        durations.push(outcome.elapsed);
        primary_units = outcome.primary_units;
        redraws_observed = outcome.redraws_observed;
        resize_cycles_observed = outcome.resize_cycles_observed;
        if let Some(cpu_phase_totals) = outcome.cpu_phase_totals {
            cpu_buffer_acquire_durations.push(cpu_phase_totals.buffer_acquire);
            cpu_raster_durations.push(cpu_phase_totals.raster);
            cpu_present_durations.push(cpu_phase_totals.present);
        }
        if notes.is_empty() {
            notes = outcome.notes;
        }
    }

    let stats = IterationStats::from_durations(&durations);
    let cpu_phase_stats = if cpu_buffer_acquire_durations.is_empty() {
        None
    } else {
        Some(LiveDisplayCpuPhaseStats {
            buffer_acquire: IterationStats::from_durations(&cpu_buffer_acquire_durations),
            raster: IterationStats::from_durations(&cpu_raster_durations),
            present: IterationStats::from_durations(&cpu_present_durations),
        })
    };
    let mean_seconds = stats.mean_nanos as f64 / 1_000_000_000.0;
    Ok(LiveDisplayScenarioReport {
        scenario: metadata.name,
        layer: metadata.layer,
        benchmark_kind: metadata.benchmark_kind,
        backend: metadata.backend,
        description: metadata.description,
        primary_unit_label: metadata.primary_unit_label,
        primary_units_per_iteration: primary_units,
        stats,
        primary_units_per_second: if mean_seconds > 0.0 {
            primary_units as f64 / mean_seconds
        } else {
            0.0
        },
        redraws_per_iteration: redraws_observed,
        resize_cycles_per_iteration: resize_cycles_observed,
        cpu_phase_stats,
        notes,
    })
}

struct LiveDisplayApp {
    backend: DisplayBackend,
    scenario_kind: ScenarioKind,
    requested_extent: PhysicalSize<u32>,
    resize_targets: Vec<PhysicalSize<u32>>,
    resize_cycles_target: u32,
    steady_frames_target: u32,
    terminal: TerminalState,
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu_renderer: Option<GpuRenderer>,
    softbuffer_context: Option<SoftbufferContext<Arc<Window>>>,
    softbuffer_surface: Option<SoftbufferSurface<Arc<Window>, Arc<Window>>>,
    glyph_cache: GlyphCache,
    dirty_rows_scratch: Vec<u16>,
    redraws_observed: u32,
    resize_cycles_observed: u32,
    resize_request_cursor: usize,
    pending_resize_started_at: Option<Instant>,
    iteration_started_at: Option<Instant>,
    cpu_phase_totals: LiveDisplayCpuPhaseTotals,
    result: Option<Result<LiveDisplayIterationOutcome>>,
}

impl LiveDisplayApp {
    fn new(scenario: ScenarioArg, cli: &Cli, workload: &LiveDisplayWorkload) -> Self {
        let (backend, scenario_kind) = match scenario {
            ScenarioArg::StartupFirstFrameGpu => {
                (DisplayBackend::Gpu, ScenarioKind::StartupFirstFrame)
            }
            ScenarioArg::StartupFirstFrameCpu => {
                (DisplayBackend::Cpu, ScenarioKind::StartupFirstFrame)
            }
            ScenarioArg::SteadyRedrawGpu => (DisplayBackend::Gpu, ScenarioKind::SteadyRedraw),
            ScenarioArg::SteadyRedrawCpu => (DisplayBackend::Cpu, ScenarioKind::SteadyRedraw),
            ScenarioArg::ResizeCycleGpu => (DisplayBackend::Gpu, ScenarioKind::ResizeCycle),
            ScenarioArg::ResizeCycleCpu => (DisplayBackend::Cpu, ScenarioKind::ResizeCycle),
            _ => unreachable!("invalid live-display scenario"),
        };

        let requested_extent =
            PhysicalSize::new(workload.requested_width, workload.requested_height);
        let terminal = seeded_terminal_state(
            cli,
            &Workload::generate(cli.cols, crate::data::WorkloadScale::from_arg(cli.scale)),
        );

        Self {
            backend,
            scenario_kind,
            requested_extent,
            resize_targets: workload.resize_targets.clone(),
            resize_cycles_target: workload.resize_cycles_per_iteration,
            steady_frames_target: workload.steady_frames_per_iteration,
            terminal,
            window: None,
            window_id: None,
            gpu_renderer: matches!(backend, DisplayBackend::Gpu)
                .then(|| GpuRenderer::new(SurfaceRecoveryPolicy::default())),
            softbuffer_context: None,
            softbuffer_surface: None,
            glyph_cache: GlyphCache::new(CELL_WIDTH as u16, CELL_HEIGHT as u16),
            dirty_rows_scratch: Vec::new(),
            redraws_observed: 0,
            resize_cycles_observed: 0,
            resize_request_cursor: 0,
            pending_resize_started_at: None,
            iteration_started_at: None,
            cpu_phase_totals: LiveDisplayCpuPhaseTotals::default(),
            result: None,
        }
    }

    fn into_result(self) -> Result<LiveDisplayIterationOutcome> {
        self.result
            .unwrap_or_else(|| bail!("live display benchmark exited without a result"))
    }

    fn finish_success(&mut self, event_loop: &ActiveEventLoop, notes: Vec<String>) {
        let elapsed = self
            .iteration_started_at
            .map(|started| started.elapsed())
            .unwrap_or_default();
        let primary_units = match self.scenario_kind {
            ScenarioKind::StartupFirstFrame => 1,
            ScenarioKind::SteadyRedraw => u64::from(self.redraws_observed),
            ScenarioKind::ResizeCycle => u64::from(self.resize_cycles_observed),
        };
        self.result = Some(Ok(LiveDisplayIterationOutcome {
            elapsed,
            primary_units,
            redraws_observed: self.redraws_observed,
            resize_cycles_observed: self.resize_cycles_observed,
            cpu_phase_totals: matches!(self.backend, DisplayBackend::Cpu)
                .then_some(self.cpu_phase_totals),
            notes,
        }));
        event_loop.exit();
    }

    fn finish_error(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.result = Some(Err(error));
        event_loop.exit();
    }

    fn window_attributes(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title("rldyourterm live display benchmark")
            .with_inner_size(LogicalSize::new(
                f64::from(self.requested_extent.width),
                f64::from(self.requested_extent.height),
            ))
            .with_visible(true)
    }

    fn request_next_resize(&mut self, window: &Window) -> bool {
        if self.resize_cycles_observed >= self.resize_cycles_target
            || self.resize_targets.is_empty()
        {
            return false;
        }
        let target = self.resize_targets[self.resize_request_cursor % self.resize_targets.len()];
        self.resize_request_cursor = self.resize_request_cursor.wrapping_add(1);
        self.pending_resize_started_at = Some(Instant::now());
        let current_size = window.inner_size();
        if let Some(applied_size) = window.request_inner_size(target) {
            self.pending_resize_started_at = None;
            if applied_size != current_size {
                self.handle_resize(applied_size);
                self.resize_cycles_observed = self.resize_cycles_observed.saturating_add(1);
            }
        }
        true
    }

    fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if let Some(renderer) = self.gpu_renderer.as_mut() {
            renderer.resize(size.width, size.height);
        }
        if let Some(surface) = self.softbuffer_surface.as_mut()
            && let (Some(width), Some(height)) =
                (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        {
            let _ = surface.resize(width, height);
        }
    }

    fn render_gpu(&mut self) -> Result<()> {
        let dirty_rows = vec![true; self.terminal.grid.height() as usize];
        self.terminal.cursor.visible = !self.terminal.cursor.visible;
        self.gpu_renderer
            .as_mut()
            .context("gpu renderer not initialized")?
            .render_frame(&self.terminal, &dirty_rows, 0, true, 0, u32::MAX, u32::MAX)
            .map_err(|error| match error {
                GpuRenderError::SurfaceAcquire(source) => {
                    anyhow::anyhow!("gpu surface acquire failed during live benchmark: {source}")
                }
                other => anyhow::anyhow!("gpu render failed during live benchmark: {other}"),
            })
    }

    fn render_cpu(&mut self) -> Result<()> {
        let window = self.window.as_ref().context("window not initialized")?;
        let size = window.inner_size();
        self.terminal.cursor.visible = !self.terminal.cursor.visible;
        let surface = self
            .softbuffer_surface
            .as_mut()
            .context("softbuffer surface not initialized")?;
        let acquire_started_at = Instant::now();
        let mut buffer = surface.buffer_mut().map_err(|error| {
            anyhow::anyhow!("softbuffer buffer_mut failed in live benchmark: {error}")
        })?;
        self.cpu_phase_totals.buffer_acquire += acquire_started_at.elapsed();

        let raster_started_at = Instant::now();
        render_terminal_buffer(
            &mut buffer,
            size.width as usize,
            size.height as usize,
            &mut self.terminal,
            &mut self.glyph_cache,
            None,
            &mut self.dirty_rows_scratch,
            true,
            0,
            u32::MAX,
            u32::MAX,
        );
        self.cpu_phase_totals.raster += raster_started_at.elapsed();

        let present_started_at = Instant::now();
        buffer.present().map_err(|error| {
            anyhow::anyhow!("softbuffer present failed in live benchmark: {error}")
        })?;
        self.cpu_phase_totals.present += present_started_at.elapsed();
        Ok(())
    }
}

impl ApplicationHandler for LiveDisplayApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.iteration_started_at = Some(Instant::now());
        let window = match event_loop.create_window(self.window_attributes()) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.finish_error(
                    event_loop,
                    anyhow::anyhow!("failed to create live benchmark window: {error}"),
                );
                return;
            }
        };

        self.window_id = Some(window.id());
        match self.backend {
            DisplayBackend::Gpu => {
                let mut renderer = self.gpu_renderer.take().unwrap_or_default();
                if let Err(error) = renderer.initialize(
                    window.clone(),
                    self.requested_extent.width,
                    self.requested_extent.height,
                    None,
                ) {
                    self.finish_error(
                        event_loop,
                        anyhow::anyhow!("gpu initialize failed in live benchmark: {error}"),
                    );
                    return;
                }
                self.gpu_renderer = Some(renderer);
            }
            DisplayBackend::Cpu => {
                let context = match SoftbufferContext::new(window.clone()) {
                    Ok(context) => context,
                    Err(error) => {
                        self.finish_error(
                            event_loop,
                            anyhow::anyhow!(
                                "softbuffer context init failed in live benchmark: {error}"
                            ),
                        );
                        return;
                    }
                };
                let mut surface = match SoftbufferSurface::new(&context, window.clone()) {
                    Ok(surface) => surface,
                    Err(error) => {
                        self.finish_error(
                            event_loop,
                            anyhow::anyhow!(
                                "softbuffer surface init failed in live benchmark: {error}"
                            ),
                        );
                        return;
                    }
                };
                if let (Some(width), Some(height)) = (
                    NonZeroU32::new(self.requested_extent.width),
                    NonZeroU32::new(self.requested_extent.height),
                ) && let Err(error) = surface.resize(width, height)
                {
                    self.finish_error(
                        event_loop,
                        anyhow::anyhow!("softbuffer resize failed in live benchmark: {error}"),
                    );
                    return;
                }
                self.softbuffer_context = Some(context);
                self.softbuffer_surface = Some(surface);
            }
        }
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.finish_error(
                    event_loop,
                    anyhow::anyhow!("live benchmark window was closed before completion"),
                );
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size);
                if self.pending_resize_started_at.take().is_some() {
                    self.resize_cycles_observed = self.resize_cycles_observed.saturating_add(1);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.clone() {
                    let size = window.inner_size();
                    self.handle_resize(size);
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let render_result = match self.backend {
                    DisplayBackend::Gpu => self.render_gpu(),
                    DisplayBackend::Cpu => self.render_cpu(),
                };
                if let Err(error) = render_result {
                    self.finish_error(event_loop, error);
                    return;
                }

                self.redraws_observed = self.redraws_observed.saturating_add(1);
                let mut notes = vec![format!(
                    "requested_extent={}x{}",
                    self.requested_extent.width, self.requested_extent.height
                )];

                match self.scenario_kind {
                    ScenarioKind::StartupFirstFrame => {
                        notes.push(format!("redraws={}", self.redraws_observed));
                        self.finish_success(event_loop, notes);
                    }
                    ScenarioKind::SteadyRedraw => {
                        if self.redraws_observed >= self.steady_frames_target {
                            notes.push(format!("frames={}", self.redraws_observed));
                            self.finish_success(event_loop, notes);
                        }
                    }
                    ScenarioKind::ResizeCycle => {
                        if self.resize_cycles_observed >= self.resize_cycles_target {
                            notes.push(format!("resize_cycles={}", self.resize_cycles_observed));
                            self.finish_success(event_loop, notes);
                            return;
                        }
                        if let Some(window) = self.window.clone() {
                            let requested = self.request_next_resize(&window);
                            notes.push(format!("requested_next_resize={requested}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
