use crate::cli::OutputFormatArg;
use crate::data::WorkloadSummary;
use crate::metrics::IterationStats;
use serde::Serialize;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSuiteReport {
    pub benchmark_tool: &'static str,
    pub scenario_selection: String,
    pub scale: &'static str,
    pub warmup_iterations: u32,
    pub measured_iterations: u32,
    pub cols: u16,
    pub rows: u16,
    pub chunk_bytes: usize,
    pub scrollback_cap: usize,
    pub workload: WorkloadSummary,
    pub results: Vec<ScenarioReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioReport {
    pub scenario: &'static str,
    pub description: &'static str,
    pub primary_unit_label: &'static str,
    pub primary_units_per_iteration: u64,
    pub byte_units_per_iteration: u64,
    pub stats: IterationStats,
    pub primary_units_per_second: f64,
    pub bytes_per_second: f64,
    pub notes: Vec<String>,
}

impl BenchmarkSuiteReport {
    pub fn write_output(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(path, payload)?;
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
            "terminal-benchmark scale={} warmup={} measured={} grid={}x{} chunk_bytes={} scrollback_cap={}",
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
            "workload ai_burst_bytes={} scrollback_flood_bytes={} render_seed_bytes={} delta_batches={} delta_batch_bytes={}",
            self.workload.ai_burst_bytes,
            self.workload.scrollback_flood_bytes,
            self.workload.render_seed_bytes,
            self.workload.delta_batches,
            self.workload.delta_batch_bytes,
        );
        let _ = writeln!(
            out,
            "{:<32} {:>10} {:>10} {:>10} {:>10} {:>16} {:>16}",
            "scenario", "mean_ms", "p95_ms", "min_ms", "max_ms", "units/sec", "bytes/sec",
        );
        let _ = writeln!(
            out,
            "{:-<32} {:-<10} {:-<10} {:-<10} {:-<10} {:-<16} {:-<16}",
            "", "", "", "", "", "", ""
        );
        for result in &self.results {
            let _ = writeln!(
                out,
                "{:<32} {:>10.3} {:>10.3} {:>10.3} {:>10.3} {:>16.2} {:>16.2}",
                result.scenario,
                nanos_to_millis(result.stats.mean_nanos),
                nanos_to_millis(result.stats.p95_nanos),
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

fn nanos_to_millis(nanos: u128) -> f64 {
    nanos as f64 / 1_000_000.0
}
