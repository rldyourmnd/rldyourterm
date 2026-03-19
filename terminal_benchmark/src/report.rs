// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::cli::OutputFormatArg;
use crate::coverage::CoverageSummary;
use crate::data::WorkloadSummary;
use crate::metrics::IterationStats;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BenchmarkReport {
    Headless(BenchmarkSuiteReport),
    LiveDisplay(LiveDisplayBenchmarkSuiteReport),
}

pub const SUITE_MANIFEST_SCHEMA_VERSION: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteManifest {
    pub schema_version: u16,
    pub scenarios: Vec<SuiteScenarioManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteScenarioManifest {
    pub scenario: String,
    pub layer: String,
    pub benchmark_kind: String,
    pub description: String,
    pub primary_unit_label: String,
    pub backend: Option<String>,
    pub controlled_monitor_cadence: bool,
}

impl BenchmarkReport {
    pub fn write_output(&self, path: &Path) -> anyhow::Result<()> {
        match self {
            Self::Headless(report) => report.write_output(path),
            Self::LiveDisplay(report) => report.write_output(path),
        }
    }

    pub fn render_stdout(&self, format: OutputFormatArg) -> anyhow::Result<String> {
        match self {
            Self::Headless(report) => report.render_stdout(format),
            Self::LiveDisplay(report) => report.render_stdout(format),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuiteReport {
    pub benchmark_tool: String,
    pub suite: String,
    pub suite_manifest: SuiteManifest,
    pub scenario_selection: String,
    pub selected_scenarios: Vec<String>,
    pub scale: String,
    pub warmup_iterations: u32,
    pub measured_iterations: u32,
    pub cols: u16,
    pub rows: u16,
    pub chunk_bytes: usize,
    pub scrollback_cap: usize,
    pub workload: WorkloadSummary,
    pub coverage: CoverageSummary,
    pub results: Vec<ScenarioReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayEnvironmentReport {
    pub kind: String,
    pub window_runtime: String,
    pub gpu_runtime: String,
    pub cpu_present_runtime: String,
    pub platform_dependent: bool,
    pub session_type: Option<String>,
    pub display_server_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayWorkloadSummary {
    pub startup_runs_per_iteration: u32,
    pub steady_frames_per_iteration: u32,
    pub resize_cycles_per_iteration: u32,
    pub requested_width: u32,
    pub requested_height: u32,
    pub resize_targets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayBenchmarkSuiteReport {
    pub benchmark_tool: String,
    pub suite: String,
    pub suite_manifest: SuiteManifest,
    pub scenario_selection: String,
    pub selected_scenarios: Vec<String>,
    pub scale: String,
    pub warmup_iterations: u32,
    pub measured_iterations: u32,
    pub cols: u16,
    pub rows: u16,
    pub environment: LiveDisplayEnvironmentReport,
    pub workload: LiveDisplayWorkloadSummary,
    pub results: Vec<LiveDisplayScenarioReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub scenario: String,
    pub layer: String,
    pub benchmark_kind: String,
    pub description: String,
    pub primary_unit_label: String,
    pub primary_units_per_iteration: u64,
    pub byte_units_per_iteration: u64,
    pub stats: IterationStats,
    pub primary_units_per_second: f64,
    pub bytes_per_second: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayScenarioReport {
    pub scenario: String,
    pub layer: String,
    pub benchmark_kind: String,
    pub backend: String,
    pub description: String,
    pub primary_unit_label: String,
    pub primary_units_per_iteration: u64,
    pub stats: IterationStats,
    pub primary_units_per_second: f64,
    pub pacing_mode: String,
    pub monitor_refresh_rate_millihz: Option<u32>,
    pub monitor_name: Option<String>,
    pub monitor_scale_factor: Option<f64>,
    pub display_phase_stats: LiveDisplayPhaseStats,
    pub redraws_per_iteration: u32,
    pub resize_cycles_per_iteration: u32,
    pub cpu_phase_stats: Option<LiveDisplayCpuPhaseStats>,
    pub cpu_buffer_age_counts: Option<LiveDisplayCpuBufferAgeReport>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayPhaseStats {
    pub redraw_dispatch: IterationStats,
    pub frame_gap: Option<IterationStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayCpuPhaseStats {
    pub buffer_acquire: IterationStats,
    pub raster: IterationStats,
    pub present: IterationStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisplayCpuBufferAgeReport {
    pub age_0: u64,
    pub age_1: u64,
    pub age_2: u64,
    pub age_3_plus: u64,
}

impl BenchmarkSuiteReport {
    pub fn write_output(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }

    pub fn render_stdout(&self, format: OutputFormatArg) -> anyhow::Result<String> {
        match format {
            OutputFormatArg::Json => Ok(serde_json::to_string_pretty(self)?),
            OutputFormatArg::Table => Ok(self.render_table()),
        }
    }

    fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "terminal-benchmark suite={} selection={} scale={} warmup={} measured={} grid={}x{} chunk_bytes={} scrollback_cap={}",
            self.suite,
            self.scenario_selection,
            self.scale,
            self.warmup_iterations,
            self.measured_iterations,
            self.cols,
            self.rows,
            self.chunk_bytes,
            self.scrollback_cap,
        );
        let _ = writeln!(
            out,
            "selected_scenarios count={} names={}",
            self.selected_scenarios.len(),
            self.selected_scenarios.join(","),
        );
        let _ = writeln!(
            out,
            "workload ai_burst_bytes={} scrollback_flood_bytes={} render_seed_bytes={} delta_batches={} delta_batch_bytes={} session_cycles={} ui_batch_repetitions={} settings_rounds={} shell_rounds={} font_passes={} surface_rounds={}",
            self.workload.ai_burst_bytes,
            self.workload.scrollback_flood_bytes,
            self.workload.render_seed_bytes,
            self.workload.delta_batches,
            self.workload.delta_batch_bytes,
            self.workload.session_cycles,
            self.workload.ui_batch_repetitions,
            self.workload.settings_rounds,
            self.workload.shell_rounds,
            self.workload.font_passes,
            self.workload.surface_rounds,
        );
        let _ = writeln!(
            out,
            "{:<32} {:<24} {:<14} {:>10} {:>10} {:>10} {:>10} {:>10} {:>16} {:>16}",
            "scenario",
            "layer",
            "kind",
            "mean_ms",
            "p95_ms",
            "p99_ms",
            "min_ms",
            "max_ms",
            "units/sec",
            "bytes/sec",
        );
        let _ = writeln!(
            out,
            "{:-<32} {:-<24} {:-<14} {:-<10} {:-<10} {:-<10} {:-<10} {:-<10} {:-<16} {:-<16}",
            "", "", "", "", "", "", "", "", "", ""
        );
        for result in &self.results {
            let _ = writeln!(
                out,
                "{:<32} {:<24} {:<14} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>16.2} {:>16.2}",
                result.scenario,
                result.layer,
                result.benchmark_kind,
                nanos_to_millis(result.stats.mean_nanos),
                nanos_to_millis(result.stats.p95_nanos),
                nanos_to_millis(result.stats.p99_nanos),
                nanos_to_millis(result.stats.min_nanos),
                nanos_to_millis(result.stats.max_nanos),
                result.primary_units_per_second,
                result.bytes_per_second,
            );
            let _ = writeln!(
                out,
                "  unit={} units/iter={} bytes/iter={} notes={}",
                result.primary_unit_label,
                result.primary_units_per_iteration,
                result.byte_units_per_iteration,
                if result.notes.is_empty() {
                    "none".to_owned()
                } else {
                    result.notes.join("; ")
                }
            );
        }
        out
    }
}

impl LiveDisplayBenchmarkSuiteReport {
    pub fn write_output(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }

    pub fn render_stdout(&self, format: OutputFormatArg) -> anyhow::Result<String> {
        match format {
            OutputFormatArg::Json => Ok(serde_json::to_string_pretty(self)?),
            OutputFormatArg::Table => Ok(self.render_table()),
        }
    }

    fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "terminal-benchmark suite={} selection={} scale={} warmup={} measured={} grid={}x{} environment={} window_runtime={} gpu_runtime={} cpu_present_runtime={} session_type={} display_server_hint={}",
            self.suite,
            self.scenario_selection,
            self.scale,
            self.warmup_iterations,
            self.measured_iterations,
            self.cols,
            self.rows,
            self.environment.kind,
            self.environment.window_runtime,
            self.environment.gpu_runtime,
            self.environment.cpu_present_runtime,
            self.environment
                .session_type
                .clone()
                .unwrap_or_else(|| "none".to_owned()),
            self.environment.display_server_hint,
        );
        let _ = writeln!(
            out,
            "selected_scenarios count={} names={}",
            self.selected_scenarios.len(),
            self.selected_scenarios.join(","),
        );
        let _ = writeln!(
            out,
            "workload startup_runs={} steady_frames={} resize_cycles={} requested_extent={}x{} resize_targets={}",
            self.workload.startup_runs_per_iteration,
            self.workload.steady_frames_per_iteration,
            self.workload.resize_cycles_per_iteration,
            self.workload.requested_width,
            self.workload.requested_height,
            self.workload.resize_targets,
        );
        let _ = writeln!(
            out,
            "{:<32} {:<18} {:<16} {:>10} {:>10} {:>10} {:>10} {:>10} {:>16}",
            "scenario",
            "layer",
            "backend",
            "mean_ms",
            "p95_ms",
            "p99_ms",
            "min_ms",
            "max_ms",
            "units/sec",
        );
        let _ = writeln!(
            out,
            "{:-<32} {:-<18} {:-<16} {:-<10} {:-<10} {:-<10} {:-<10} {:-<10} {:-<16}",
            "", "", "", "", "", "", "", "", ""
        );
        for result in &self.results {
            let _ = writeln!(
                out,
                "{:<32} {:<18} {:<16} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>16.2}",
                result.scenario,
                result.layer,
                result.backend,
                nanos_to_millis(result.stats.mean_nanos),
                nanos_to_millis(result.stats.p95_nanos),
                nanos_to_millis(result.stats.p99_nanos),
                nanos_to_millis(result.stats.min_nanos),
                nanos_to_millis(result.stats.max_nanos),
                result.primary_units_per_second,
            );
            let _ = writeln!(
                out,
                "  kind={} unit={} pacing={} monitor_mhz={} units/iter={} redraws/iter={} resize_cycles/iter={} notes={}",
                result.benchmark_kind,
                result.primary_unit_label,
                result.pacing_mode,
                result
                    .monitor_refresh_rate_millihz
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                result.primary_units_per_iteration,
                result.redraws_per_iteration,
                result.resize_cycles_per_iteration,
                if result.notes.is_empty() {
                    "none".to_owned()
                } else {
                    result.notes.join("; ")
                }
            );
            let _ = writeln!(
                out,
                "  monitor name={} scale_factor={}",
                result
                    .monitor_name
                    .clone()
                    .unwrap_or_else(|| "none".to_owned()),
                result
                    .monitor_scale_factor
                    .map(|value| format!("{value:.3}"))
                    .unwrap_or_else(|| "none".to_owned()),
            );
            let _ = writeln!(
                out,
                "  display_phases mean_ms redraw_dispatch={:.3} frame_gap={}",
                nanos_to_millis(result.display_phase_stats.redraw_dispatch.mean_nanos),
                result
                    .display_phase_stats
                    .frame_gap
                    .as_ref()
                    .map(|stats| format!("{:.3}", nanos_to_millis(stats.mean_nanos)))
                    .unwrap_or_else(|| "none".to_owned()),
            );
            if let Some(cpu_phase_stats) = &result.cpu_phase_stats {
                let _ = writeln!(
                    out,
                    "  cpu_phases mean_ms acquire={:.3} raster={:.3} present={:.3}",
                    nanos_to_millis(cpu_phase_stats.buffer_acquire.mean_nanos),
                    nanos_to_millis(cpu_phase_stats.raster.mean_nanos),
                    nanos_to_millis(cpu_phase_stats.present.mean_nanos),
                );
            }
            if let Some(cpu_buffer_age_counts) = &result.cpu_buffer_age_counts {
                let _ = writeln!(
                    out,
                    "  cpu_buffer_age_counts age0={} age1={} age2={} age3_plus={}",
                    cpu_buffer_age_counts.age_0,
                    cpu_buffer_age_counts.age_1,
                    cpu_buffer_age_counts.age_2,
                    cpu_buffer_age_counts.age_3_plus,
                );
            }
        }
        out
    }
}

fn nanos_to_millis(nanos: u128) -> f64 {
    nanos as f64 / 1_000_000.0
}
