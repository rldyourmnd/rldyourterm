// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::cli::{SuiteArg, ValidateCli};
use crate::live_display::scenario_registry as live_display_registry;
use crate::report::{
    BenchmarkSuiteReport, LiveDisplayBenchmarkSuiteReport, SuiteManifest, SuiteScenarioManifest,
};
use crate::scenario_registry;
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const EXPECTED_TOOL: &str = "terminal-benchmark";
const LIVE_DISPLAY_KIND: &str = "live-display";
const LIVE_DISPLAY_WINDOW_RUNTIME: &str = "winit";
const LIVE_DISPLAY_GPU_RUNTIME: &str = "wgpu";
const LIVE_DISPLAY_CPU_PRESENT_RUNTIME: &str = "softbuffer";

pub fn run(args: &ValidateCli) -> Result<()> {
    match args.suite {
        SuiteArg::CanonicalHeadless => {
            let report: BenchmarkSuiteReport = read_json(&args.report)?;
            validate_headless_report(&report, args)
        }
        SuiteArg::LiveDisplay => {
            let report: LiveDisplayBenchmarkSuiteReport = read_json(&args.report)?;
            validate_live_display_report(&report, args)
        }
    }
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let file = File::open(path)
        .with_context(|| format!("failed to open benchmark report {}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .with_context(|| format!("failed to deserialize benchmark report {}", path.display()))
}

fn validate_headless_report(report: &BenchmarkSuiteReport, args: &ValidateCli) -> Result<()> {
    validate_non_empty_string(&report.benchmark_tool, "benchmark_tool")?;
    if report.benchmark_tool != EXPECTED_TOOL {
        bail!("benchmark_tool must be {EXPECTED_TOOL:?}");
    }
    validate_non_empty_string(&report.suite, "suite")?;
    if report.suite != scenario_registry::BENCHMARK_SUITE_NAME {
        bail!(
            "suite must be {:?}",
            scenario_registry::BENCHMARK_SUITE_NAME
        );
    }
    validate_non_empty_string(&report.scale, "scale")?;
    validate_non_empty_string(&report.scenario_selection, "scenario_selection")?;

    let expected_manifest = scenario_registry::suite_manifest();
    if report.suite_manifest != expected_manifest {
        bail!("suite_manifest must match canonical headless manifest");
    }

    let expected_coverage = expected_manifest
        .coverage
        .clone()
        .expect("canonical headless manifest must define coverage");
    if report.coverage != expected_coverage {
        bail!("coverage must match canonical headless coverage contract");
    }

    let expected_scenarios = manifest_names(&expected_manifest);
    let selected_scenarios = validate_selected_scenarios(
        &report.selected_scenarios,
        &expected_scenarios,
        args,
        "canonical headless",
    )?;
    validate_headless_results(report, &expected_manifest, &selected_scenarios)
}

fn validate_live_display_report(
    report: &LiveDisplayBenchmarkSuiteReport,
    args: &ValidateCli,
) -> Result<()> {
    validate_non_empty_string(&report.benchmark_tool, "benchmark_tool")?;
    if report.benchmark_tool != EXPECTED_TOOL {
        bail!("benchmark_tool must be {EXPECTED_TOOL:?}");
    }
    validate_non_empty_string(&report.suite, "suite")?;
    if report.suite != live_display_registry::BENCHMARK_SUITE_NAME {
        bail!(
            "suite must be {:?}",
            live_display_registry::BENCHMARK_SUITE_NAME
        );
    }
    validate_non_empty_string(&report.scale, "scale")?;
    validate_non_empty_string(&report.scenario_selection, "scenario_selection")?;

    let expected_manifest = live_display_registry::suite_manifest();
    if report.suite_manifest != expected_manifest {
        bail!("suite_manifest must match live display manifest");
    }

    validate_non_empty_string(&report.environment.kind, "environment.kind")?;
    validate_non_empty_string(
        &report.environment.window_runtime,
        "environment.window_runtime",
    )?;
    validate_non_empty_string(&report.environment.gpu_runtime, "environment.gpu_runtime")?;
    validate_non_empty_string(
        &report.environment.cpu_present_runtime,
        "environment.cpu_present_runtime",
    )?;
    if report.environment.kind != LIVE_DISPLAY_KIND {
        bail!("environment.kind must be {LIVE_DISPLAY_KIND:?}");
    }
    if report.environment.window_runtime != LIVE_DISPLAY_WINDOW_RUNTIME {
        bail!("environment.window_runtime must be {LIVE_DISPLAY_WINDOW_RUNTIME:?}");
    }
    if report.environment.gpu_runtime != LIVE_DISPLAY_GPU_RUNTIME {
        bail!("environment.gpu_runtime must be {LIVE_DISPLAY_GPU_RUNTIME:?}");
    }
    if report.environment.cpu_present_runtime != LIVE_DISPLAY_CPU_PRESENT_RUNTIME {
        bail!("environment.cpu_present_runtime must be {LIVE_DISPLAY_CPU_PRESENT_RUNTIME:?}");
    }
    if !report.environment.platform_dependent {
        bail!("environment.platform_dependent must be true");
    }
    validate_optional_non_empty_string(
        report.environment.session_type.as_deref(),
        "environment.session_type",
    )?;
    validate_non_empty_string(
        &report.environment.display_server_hint,
        "environment.display_server_hint",
    )?;

    let expected_scenarios = manifest_names(&expected_manifest);
    let selected_scenarios = validate_selected_scenarios(
        &report.selected_scenarios,
        &expected_scenarios,
        args,
        "live display",
    )?;
    validate_live_display_results(report, &expected_manifest, &selected_scenarios)
}

fn validate_selected_scenarios(
    selected_scenarios: &[String],
    expected_scenarios: &BTreeSet<String>,
    args: &ValidateCli,
    suite_label: &str,
) -> Result<BTreeSet<String>> {
    if selected_scenarios.is_empty() {
        bail!("selected_scenarios must not be empty");
    }

    let mut selected = BTreeSet::new();
    for scenario in selected_scenarios {
        validate_non_empty_string(scenario, "selected_scenarios entry")?;
        if !selected.insert(scenario.clone()) {
            bail!("selected_scenarios must be unique");
        }
        if !expected_scenarios.contains(scenario) {
            bail!("selected_scenarios contains unexpected scenario {scenario:?}");
        }
    }

    for required in &args.require_scenario {
        validate_non_empty_string(required, "required scenario")?;
        if !selected.contains(required) {
            bail!("missing required scenario {required:?}");
        }
    }

    if args.require_full_suite && selected != *expected_scenarios {
        bail!(
            "{suite_label} full suite mismatch: expected {:?}, got {:?}",
            expected_scenarios,
            selected
        );
    }

    Ok(selected)
}

fn validate_headless_results(
    report: &BenchmarkSuiteReport,
    expected_manifest: &SuiteManifest,
    selected_scenarios: &BTreeSet<String>,
) -> Result<()> {
    if report.results.is_empty() {
        bail!("results must not be empty");
    }

    let expected_by_name = manifest_lookup(expected_manifest);
    let mut result_names = BTreeSet::new();
    for result in &report.results {
        validate_non_empty_string(&result.scenario, "result.scenario")?;
        validate_non_empty_string(&result.layer, "result.layer")?;
        validate_non_empty_string(&result.benchmark_kind, "result.benchmark_kind")?;
        validate_non_empty_string(&result.description, "result.description")?;
        validate_non_empty_string(&result.primary_unit_label, "result.primary_unit_label")?;
        let expected = expected_by_name
            .get(result.scenario.as_str())
            .with_context(|| format!("unexpected scenario {:?}", result.scenario))?;
        validate_common_result_metadata(
            result.scenario.as_str(),
            result.layer.as_str(),
            result.benchmark_kind.as_str(),
            result.description.as_str(),
            result.primary_unit_label.as_str(),
            expected,
        )?;
        if !result_names.insert(result.scenario.clone()) {
            bail!("results must not contain duplicate scenario names");
        }
    }

    if &result_names != selected_scenarios {
        bail!(
            "results must match selected_scenarios exactly: results={:?} selected={:?}",
            result_names,
            selected_scenarios
        );
    }

    Ok(())
}

fn validate_live_display_results(
    report: &LiveDisplayBenchmarkSuiteReport,
    expected_manifest: &SuiteManifest,
    selected_scenarios: &BTreeSet<String>,
) -> Result<()> {
    if report.results.is_empty() {
        bail!("results must not be empty");
    }

    let expected_by_name = manifest_lookup(expected_manifest);
    let mut result_names = BTreeSet::new();
    for result in &report.results {
        validate_non_empty_string(&result.scenario, "result.scenario")?;
        validate_non_empty_string(&result.layer, "result.layer")?;
        validate_non_empty_string(&result.benchmark_kind, "result.benchmark_kind")?;
        validate_non_empty_string(&result.backend, "result.backend")?;
        validate_non_empty_string(&result.description, "result.description")?;
        validate_non_empty_string(&result.primary_unit_label, "result.primary_unit_label")?;
        validate_non_empty_string(&result.pacing_mode, "result.pacing_mode")?;
        validate_optional_non_empty_string(result.monitor_name.as_deref(), "result.monitor_name")?;
        if let Some(scale_factor) = result.monitor_scale_factor
            && scale_factor <= 0.0
        {
            bail!(
                "scenario {:?} monitor_scale_factor must be positive",
                result.scenario
            );
        }

        let expected = expected_by_name
            .get(result.scenario.as_str())
            .with_context(|| format!("unexpected scenario {:?}", result.scenario))?;
        validate_common_result_metadata(
            result.scenario.as_str(),
            result.layer.as_str(),
            result.benchmark_kind.as_str(),
            result.description.as_str(),
            result.primary_unit_label.as_str(),
            expected,
        )?;
        match expected.backend.as_deref() {
            Some(backend) if backend == result.backend => {}
            Some(backend) => {
                bail!(
                    "scenario {:?} has unexpected backend {:?}, expected {:?}",
                    result.scenario,
                    result.backend,
                    backend
                );
            }
            None => {
                bail!(
                    "live display manifest entry {:?} must define backend",
                    result.scenario
                );
            }
        }

        if result.backend == "cpu" {
            if result.cpu_phase_stats.is_none() {
                bail!(
                    "scenario {:?} cpu_phase_stats must be present",
                    result.scenario
                );
            }
            if result.cpu_buffer_age_counts.is_none() {
                bail!(
                    "scenario {:?} cpu_buffer_age_counts must be present",
                    result.scenario
                );
            }
        } else {
            if result.cpu_phase_stats.is_some() {
                bail!(
                    "scenario {:?} cpu_phase_stats must be null for non-cpu backends",
                    result.scenario
                );
            }
            if result.cpu_buffer_age_counts.is_some() {
                bail!(
                    "scenario {:?} cpu_buffer_age_counts must be null for non-cpu backends",
                    result.scenario
                );
            }
        }

        if !result_names.insert(result.scenario.clone()) {
            bail!("results must not contain duplicate scenario names");
        }
    }

    if &result_names != selected_scenarios {
        bail!(
            "results must match selected_scenarios exactly: results={:?} selected={:?}",
            result_names,
            selected_scenarios
        );
    }

    Ok(())
}

fn validate_common_result_metadata(
    scenario: &str,
    layer: &str,
    benchmark_kind: &str,
    description: &str,
    primary_unit_label: &str,
    expected: &SuiteScenarioManifest,
) -> Result<()> {
    if layer != expected.layer {
        bail!(
            "scenario {scenario:?} has unexpected layer {layer:?}, expected {:?}",
            expected.layer
        );
    }
    if benchmark_kind != expected.benchmark_kind {
        bail!(
            "scenario {scenario:?} has unexpected benchmark_kind {benchmark_kind:?}, expected {:?}",
            expected.benchmark_kind
        );
    }
    if description != expected.description {
        bail!(
            "scenario {scenario:?} has unexpected description {description:?}, expected {:?}",
            expected.description
        );
    }
    if primary_unit_label != expected.primary_unit_label {
        bail!(
            "scenario {scenario:?} has unexpected primary_unit_label {primary_unit_label:?}, expected {:?}",
            expected.primary_unit_label
        );
    }
    Ok(())
}

fn manifest_names(manifest: &SuiteManifest) -> BTreeSet<String> {
    manifest
        .scenarios
        .iter()
        .map(|entry| entry.scenario.clone())
        .collect()
}

fn manifest_lookup(manifest: &SuiteManifest) -> HashMap<&str, &SuiteScenarioManifest> {
    manifest
        .scenarios
        .iter()
        .map(|entry| (entry.scenario.as_str(), entry))
        .collect()
}

fn validate_non_empty_string(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} must be a non-empty string");
    }
    Ok(())
}

fn validate_optional_non_empty_string(value: Option<&str>, label: &str) -> Result<()> {
    if let Some(value) = value {
        validate_non_empty_string(value, label)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{validate_headless_report, validate_live_display_report};
    use crate::cli::{ScenarioArg, SuiteArg, ValidateCli};
    use crate::data::WorkloadSummary;
    use crate::live_display::scenario_registry as live_display_registry;
    use crate::metrics::IterationStats;
    use crate::report::{
        BenchmarkSuiteReport, LiveDisplayBenchmarkSuiteReport, LiveDisplayCpuBufferAgeReport,
        LiveDisplayCpuPhaseStats, LiveDisplayEnvironmentReport, LiveDisplayPhaseStats,
        LiveDisplayScenarioReport, LiveDisplayWorkloadSummary, ScenarioReport,
    };
    use crate::scenario_registry;
    use std::path::PathBuf;

    #[test]
    fn headless_validator_accepts_authoritative_full_suite() {
        let report = valid_headless_report();
        let args = validate_args(SuiteArg::CanonicalHeadless, true);

        validate_headless_report(&report, &args).expect("headless full suite should validate");
    }

    #[test]
    fn headless_validator_rejects_manifest_drift() {
        let mut report = valid_headless_report();
        report.suite_manifest.scenarios.pop();
        let args = validate_args(SuiteArg::CanonicalHeadless, true);

        let error =
            validate_headless_report(&report, &args).expect_err("truncated manifest must fail");
        assert!(
            error
                .to_string()
                .contains("suite_manifest must match canonical headless manifest")
        );
    }

    #[test]
    fn headless_validator_rejects_missing_coverage_layer() {
        let mut report = valid_headless_report();
        report.coverage.benchmarked_layers.pop();
        let args = validate_args(SuiteArg::CanonicalHeadless, true);

        let error = validate_headless_report(&report, &args).expect_err("coverage drift must fail");
        assert!(
            error
                .to_string()
                .contains("coverage must match canonical headless coverage contract")
        );
    }

    #[test]
    fn live_display_validator_accepts_authoritative_full_suite() {
        let report = valid_live_display_report();
        let args = validate_args(SuiteArg::LiveDisplay, true);

        validate_live_display_report(&report, &args)
            .expect("live display full suite should validate");
    }

    #[test]
    fn live_display_validator_rejects_missing_cpu_phase_stats() {
        let mut report = valid_live_display_report();
        let cpu_result = report
            .results
            .iter_mut()
            .find(|result| result.backend == "cpu")
            .expect("live display report must include cpu scenario");
        cpu_result.cpu_phase_stats = None;
        let args = validate_args(SuiteArg::LiveDisplay, true);

        let error = validate_live_display_report(&report, &args)
            .expect_err("cpu scenarios must keep cpu phase stats");
        assert!(
            error
                .to_string()
                .contains("cpu_phase_stats must be present")
        );
    }

    fn validate_args(suite: SuiteArg, require_full_suite: bool) -> ValidateCli {
        ValidateCli {
            suite,
            report: PathBuf::from("unused.json"),
            require_scenario: Vec::new(),
            require_full_suite,
        }
    }

    pub(crate) fn valid_headless_report() -> BenchmarkSuiteReport {
        let suite_manifest = scenario_registry::suite_manifest();
        let coverage = suite_manifest
            .coverage
            .clone()
            .expect("headless manifest must define coverage");
        let results = suite_manifest
            .scenarios
            .iter()
            .map(|scenario| ScenarioReport {
                scenario: scenario.scenario.clone(),
                layer: scenario.layer.clone(),
                benchmark_kind: scenario.benchmark_kind.clone(),
                description: scenario.description.clone(),
                primary_unit_label: scenario.primary_unit_label.clone(),
                primary_units_per_iteration: 1,
                byte_units_per_iteration: 1,
                stats: sample_stats(),
                primary_units_per_second: 1.0,
                bytes_per_second: 1.0,
                notes: Vec::new(),
            })
            .collect();

        BenchmarkSuiteReport {
            benchmark_tool: "terminal-benchmark".to_owned(),
            suite: scenario_registry::BENCHMARK_SUITE_NAME.to_owned(),
            suite_manifest,
            scenario_selection: ScenarioArg::All.as_str().to_owned(),
            selected_scenarios: scenario_registry::selected_scenario_names(ScenarioArg::All),
            scale: "quick".to_owned(),
            warmup_iterations: 0,
            measured_iterations: 1,
            cols: 160,
            rows: 48,
            chunk_bytes: 8 * 1024,
            scrollback_cap: 50_000,
            workload: WorkloadSummary {
                ai_burst_bytes: 1,
                scrollback_flood_bytes: 1,
                render_seed_bytes: 1,
                delta_batches: 1,
                delta_batch_bytes: 1,
                session_cycles: 1,
                session_boundaries_per_cycle: 1,
                ui_batch_repetitions: 1,
                ui_batch_templates: 1,
                ui_commands_per_template: 1,
                settings_inputs: 1,
                settings_rounds: 1,
                shell_cases: 1,
                shell_rounds: 1,
                font_glyphs: 1,
                font_passes: 1,
                surface_policy_cases: 1,
                surface_rounds: 1,
            },
            coverage,
            results,
        }
    }

    pub(crate) fn valid_live_display_report() -> LiveDisplayBenchmarkSuiteReport {
        let suite_manifest = live_display_registry::suite_manifest();
        let results = suite_manifest
            .scenarios
            .iter()
            .map(|scenario| {
                let is_cpu = scenario.backend.as_deref() == Some("cpu");
                LiveDisplayScenarioReport {
                    scenario: scenario.scenario.clone(),
                    layer: scenario.layer.clone(),
                    benchmark_kind: scenario.benchmark_kind.clone(),
                    backend: scenario
                        .backend
                        .clone()
                        .expect("live display manifest entries must define backend"),
                    description: scenario.description.clone(),
                    primary_unit_label: scenario.primary_unit_label.clone(),
                    primary_units_per_iteration: 1,
                    stats: sample_stats(),
                    primary_units_per_second: 1.0,
                    pacing_mode: if is_cpu {
                        "monitor-cadence".to_owned()
                    } else {
                        "event-driven".to_owned()
                    },
                    monitor_refresh_rate_millihz: is_cpu.then_some(60_000),
                    monitor_name: Some("Primary".to_owned()),
                    monitor_scale_factor: Some(1.0),
                    display_phase_stats: LiveDisplayPhaseStats {
                        redraw_dispatch: sample_stats(),
                        frame_gap: Some(sample_stats()),
                    },
                    redraws_per_iteration: 1,
                    resize_cycles_per_iteration: 1,
                    cpu_phase_stats: is_cpu.then_some(LiveDisplayCpuPhaseStats {
                        buffer_acquire: sample_stats(),
                        raster: sample_stats(),
                        present: sample_stats(),
                    }),
                    cpu_buffer_age_counts: is_cpu.then_some(LiveDisplayCpuBufferAgeReport {
                        age_0: 1,
                        age_1: 0,
                        age_2: 0,
                        age_3_plus: 0,
                    }),
                    notes: Vec::new(),
                }
            })
            .collect();

        LiveDisplayBenchmarkSuiteReport {
            benchmark_tool: "terminal-benchmark".to_owned(),
            suite: live_display_registry::BENCHMARK_SUITE_NAME.to_owned(),
            suite_manifest,
            scenario_selection: ScenarioArg::All.as_str().to_owned(),
            selected_scenarios: live_display_registry::selected_scenario_names(ScenarioArg::All),
            scale: "quick".to_owned(),
            warmup_iterations: 0,
            measured_iterations: 1,
            cols: 160,
            rows: 48,
            environment: LiveDisplayEnvironmentReport {
                kind: "live-display".to_owned(),
                window_runtime: "winit".to_owned(),
                gpu_runtime: "wgpu".to_owned(),
                cpu_present_runtime: "softbuffer".to_owned(),
                platform_dependent: true,
                session_type: Some("wayland".to_owned()),
                display_server_hint: "wayland".to_owned(),
            },
            workload: LiveDisplayWorkloadSummary {
                startup_runs_per_iteration: 1,
                steady_frames_per_iteration: 1,
                resize_cycles_per_iteration: 1,
                requested_width: 1280,
                requested_height: 720,
                resize_targets: 2,
            },
            results,
        }
    }

    fn sample_stats() -> IterationStats {
        IterationStats {
            min_nanos: 1,
            median_nanos: 1,
            p95_nanos: 1,
            max_nanos: 1,
            mean_nanos: 1,
            total_nanos: 1,
        }
    }
}
