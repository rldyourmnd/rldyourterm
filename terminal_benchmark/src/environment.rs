// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::{
    EnvironmentCli, EnvironmentCommands, EnvironmentSnapshotCli, EnvironmentValidateBaselineCli,
    EnvironmentValidateCli,
};
use crate::report::{BenchmarkReport, BenchmarkSuiteReport, LiveDisplayBenchmarkSuiteReport};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const HEADLESS_SUITE: &str = "canonical-headless";
const EXPECTED_TOOL: &str = "terminal-benchmark";
const EXPECTED_LIVE_DISPLAY_SUITE: &str = "live-display";
const REQUIRED_PACING_MODE: &str = "monitor-cadence";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnvironmentScope {
    PortableHeadless,
    LocalDisplaySession,
    ControlledDisplaySession,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlledCpuScenarioRequirements {
    pub pacing_mode: String,
    pub monitor_refresh_rate_millihz: u32,
    pub monitor_scale_factor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentRequirements {
    pub display_server_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    pub cpu_scenarios: BTreeMap<String, ControlledCpuScenarioRequirements>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    pub environment_scope: EnvironmentScope,
    pub environment_requirements: Option<EnvironmentRequirements>,
}

#[derive(Debug, Clone, Deserialize)]
struct BaselineEnvironmentContract {
    pub environment_scope: EnvironmentScope,
    pub environment_requirements: Option<EnvironmentRequirements>,
}

pub fn run(args: &EnvironmentCli) -> Result<()> {
    match &args.command {
        EnvironmentCommands::Snapshot(snapshot) => run_snapshot(snapshot),
        EnvironmentCommands::Validate(validate) => run_validate(validate),
        EnvironmentCommands::ValidateBaseline(validate) => run_validate_baseline(validate),
    }
}

pub fn infer_report_environment_scope(report: &BenchmarkReport) -> Result<EnvironmentScope> {
    match report {
        BenchmarkReport::Headless(_) => Ok(EnvironmentScope::PortableHeadless),
        BenchmarkReport::LiveDisplay(report) => infer_live_display_environment_scope(report),
    }
}

pub fn extract_environment_requirements(
    report: &BenchmarkReport,
) -> Result<Option<EnvironmentRequirements>> {
    match report {
        BenchmarkReport::Headless(_) => Ok(None),
        BenchmarkReport::LiveDisplay(report) => {
            extract_live_display_environment_requirements(report)
        }
    }
}

pub fn validate_report_against_environment_requirements(
    report: &BenchmarkReport,
    requirements: &EnvironmentRequirements,
) -> Result<()> {
    let live_display = require_live_display_report(report)?;
    validate_non_empty_string(
        &requirements.display_server_hint,
        "environment_requirements.display_server_hint",
    )?;
    if let Some(session_type) = requirements.session_type.as_deref() {
        validate_non_empty_string(session_type, "environment_requirements.session_type")?;
    }
    if requirements.cpu_scenarios.is_empty() {
        bail!("environment_requirements.cpu_scenarios must be a non-empty object");
    }

    if live_display.environment.display_server_hint != requirements.display_server_hint {
        bail!(
            "display_server_hint mismatch between report and baseline requirements: report={:?} baseline={:?}",
            live_display.environment.display_server_hint,
            requirements.display_server_hint
        );
    }
    if live_display.environment.session_type != requirements.session_type {
        bail!(
            "session_type mismatch between report and baseline requirements: report={:?} baseline={:?}",
            live_display.environment.session_type,
            requirements.session_type
        );
    }

    let result_map = live_display_result_map(live_display)?;
    for (scenario, scenario_requirements) in &requirements.cpu_scenarios {
        let result = result_map.get(scenario.as_str()).with_context(|| {
            format!("report is missing cpu scenario required by baseline: {scenario:?}")
        })?;
        if result.pacing_mode != scenario_requirements.pacing_mode {
            bail!(
                "scenario {:?} pacing_mode mismatch: report={:?} baseline={:?}",
                scenario,
                result.pacing_mode,
                scenario_requirements.pacing_mode
            );
        }
        let report_refresh = positive_u32(
            result.monitor_refresh_rate_millihz,
            &format!("scenario {scenario:?} monitor_refresh_rate_millihz"),
        )?;
        if report_refresh != scenario_requirements.monitor_refresh_rate_millihz {
            bail!(
                "scenario {:?} monitor_refresh_rate_millihz mismatch: report={:?} baseline={:?}",
                scenario,
                report_refresh,
                scenario_requirements.monitor_refresh_rate_millihz
            );
        }
        let report_scale = positive_f64(
            result.monitor_scale_factor,
            &format!("scenario {scenario:?} monitor_scale_factor"),
        )?;
        if !floats_match(report_scale, scenario_requirements.monitor_scale_factor) {
            bail!(
                "scenario {:?} monitor_scale_factor mismatch: report={:?} baseline={:?}",
                scenario,
                report_scale,
                scenario_requirements.monitor_scale_factor
            );
        }
    }

    Ok(())
}

fn run_snapshot(args: &EnvironmentSnapshotCli) -> Result<()> {
    let report = read_report(&args.report)?;
    let snapshot = EnvironmentSnapshot {
        environment_scope: infer_report_environment_scope(&report)?,
        environment_requirements: extract_environment_requirements(&report)?,
    };
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn run_validate(args: &EnvironmentValidateCli) -> Result<()> {
    let report = read_report(&args.report)?;
    let live_display = require_live_display_report(&report)?;
    if live_display.benchmark_tool != EXPECTED_TOOL {
        bail!("benchmark_tool must be {EXPECTED_TOOL:?}");
    }
    if live_display.suite != EXPECTED_LIVE_DISPLAY_SUITE {
        bail!("suite must be {EXPECTED_LIVE_DISPLAY_SUITE:?}");
    }

    if let Some(session_type) = args.require_session_type.as_deref()
        && live_display.environment.session_type.as_deref() != Some(session_type)
    {
        bail!(
            "environment.session_type must be {:?}, got {:?}",
            session_type,
            live_display.environment.session_type
        );
    }
    if let Some(display_server_hint) = args.require_display_server_hint.as_deref()
        && live_display.environment.display_server_hint != display_server_hint
    {
        bail!(
            "environment.display_server_hint must be {:?}, got {:?}",
            display_server_hint,
            live_display.environment.display_server_hint
        );
    }

    let controlled_cpu_scenarios = controlled_cpu_scenario_names(live_display);
    let result_map = live_display_result_map(live_display)?;
    let available = controlled_cpu_scenarios
        .into_iter()
        .filter(|scenario| result_map.contains_key(scenario.as_str()))
        .collect::<Vec<_>>();

    if args.require_monitor_cadence {
        if available.is_empty() {
            bail!("report does not contain any CPU monitor-cadence scenarios");
        }
        for scenario in &available {
            let result = result_map
                .get(scenario.as_str())
                .expect("available scenario must exist in result map");
            if result.pacing_mode != REQUIRED_PACING_MODE {
                bail!(
                    "scenario {:?} must use monitor-cadence, got {:?}",
                    scenario,
                    result.pacing_mode
                );
            }
            positive_u32(
                result.monitor_refresh_rate_millihz,
                &format!("scenario {scenario:?} monitor_refresh_rate_millihz"),
            )?;
        }
    }

    if args.require_monitor_scale_factor {
        if available.is_empty() {
            bail!("report does not contain any CPU monitor scenarios");
        }
        for scenario in &available {
            let result = result_map
                .get(scenario.as_str())
                .expect("available scenario must exist in result map");
            positive_f64(
                result.monitor_scale_factor,
                &format!("scenario {scenario:?} monitor_scale_factor"),
            )?;
        }
    }

    println!(
        "live display environment validation ok: {}",
        args.report.display()
    );
    Ok(())
}

fn run_validate_baseline(args: &EnvironmentValidateBaselineCli) -> Result<()> {
    let report = read_report(&args.report)?;
    let baseline: BaselineEnvironmentContract = read_json(&args.baseline)?;
    let report_scope = infer_report_environment_scope(&report)?;
    if report_scope != baseline.environment_scope {
        bail!(
            "environment_scope mismatch between report and baseline: report={:?} baseline={:?}",
            report_scope,
            baseline.environment_scope
        );
    }
    if baseline.environment_scope == EnvironmentScope::ControlledDisplaySession
        && baseline.environment_requirements.is_none()
    {
        bail!("controlled-display-session baselines must declare environment_requirements");
    }
    if let Some(requirements) = baseline.environment_requirements.as_ref() {
        validate_report_against_environment_requirements(&report, requirements)?;
    }
    Ok(())
}

fn infer_live_display_environment_scope(
    report: &LiveDisplayBenchmarkSuiteReport,
) -> Result<EnvironmentScope> {
    let controlled_cpu_scenarios = controlled_cpu_scenario_names(report);
    let result_map = live_display_result_map(report)?;
    let controlled_results = controlled_cpu_scenarios
        .into_iter()
        .filter_map(|scenario| result_map.get(scenario.as_str()))
        .collect::<Vec<_>>();

    if controlled_results.is_empty() {
        return Ok(EnvironmentScope::LocalDisplaySession);
    }

    for result in controlled_results {
        if result.pacing_mode != REQUIRED_PACING_MODE {
            return Ok(EnvironmentScope::LocalDisplaySession);
        }
        if positive_u32(
            result.monitor_refresh_rate_millihz,
            "monitor_refresh_rate_millihz",
        )
        .is_err()
        {
            return Ok(EnvironmentScope::LocalDisplaySession);
        }
        if positive_f64(result.monitor_scale_factor, "monitor_scale_factor").is_err() {
            return Ok(EnvironmentScope::LocalDisplaySession);
        }
    }

    Ok(EnvironmentScope::ControlledDisplaySession)
}

fn extract_live_display_environment_requirements(
    report: &LiveDisplayBenchmarkSuiteReport,
) -> Result<Option<EnvironmentRequirements>> {
    if infer_live_display_environment_scope(report)? != EnvironmentScope::ControlledDisplaySession {
        return Ok(None);
    }

    validate_non_empty_string(
        &report.environment.display_server_hint,
        "environment.display_server_hint",
    )?;
    if let Some(session_type) = report.environment.session_type.as_deref() {
        validate_non_empty_string(session_type, "environment.session_type")?;
    }

    let controlled_cpu_scenarios = controlled_cpu_scenario_names(report);
    let result_map = live_display_result_map(report)?;
    let mut cpu_scenarios = BTreeMap::new();

    for scenario in controlled_cpu_scenarios {
        let result = result_map.get(scenario.as_str()).with_context(|| {
            format!("controlled calibration requires cpu scenario {scenario:?}")
        })?;
        if result.pacing_mode != REQUIRED_PACING_MODE {
            bail!(
                "cpu scenario {:?} must use monitor-cadence for controlled calibration",
                scenario
            );
        }
        let refresh_rate_millihz = positive_u32(
            result.monitor_refresh_rate_millihz,
            &format!("scenario {scenario:?} monitor_refresh_rate_millihz"),
        )?;
        let monitor_scale_factor = positive_f64(
            result.monitor_scale_factor,
            &format!("scenario {scenario:?} monitor_scale_factor"),
        )?;
        cpu_scenarios.insert(
            scenario,
            ControlledCpuScenarioRequirements {
                pacing_mode: result.pacing_mode.clone(),
                monitor_refresh_rate_millihz: refresh_rate_millihz,
                monitor_scale_factor,
            },
        );
    }

    if cpu_scenarios.is_empty() {
        bail!("controlled calibration requires at least one cpu live-display scenario");
    }

    Ok(Some(EnvironmentRequirements {
        display_server_hint: report.environment.display_server_hint.clone(),
        session_type: report.environment.session_type.clone(),
        cpu_scenarios,
    }))
}

fn require_live_display_report(
    report: &BenchmarkReport,
) -> Result<&LiveDisplayBenchmarkSuiteReport> {
    match report {
        BenchmarkReport::LiveDisplay(report) => Ok(report),
        BenchmarkReport::Headless(_) => bail!("suite must be 'live-display'"),
    }
}

fn live_display_result_map(
    report: &LiveDisplayBenchmarkSuiteReport,
) -> Result<BTreeMap<&str, &crate::report::LiveDisplayScenarioReport>> {
    if report.results.is_empty() {
        bail!("results must be a non-empty list");
    }

    let mut result_map = BTreeMap::new();
    for result in &report.results {
        validate_non_empty_string(&result.scenario, "scenario")?;
        if result_map
            .insert(result.scenario.as_str(), result)
            .is_some()
        {
            bail!("results must not contain duplicate scenario names");
        }
    }
    Ok(result_map)
}

fn controlled_cpu_scenario_names(report: &LiveDisplayBenchmarkSuiteReport) -> BTreeSet<String> {
    report
        .suite_manifest
        .scenarios
        .iter()
        .filter(|entry| entry.backend.as_deref() == Some("cpu") && entry.controlled_monitor_cadence)
        .map(|entry| entry.scenario.clone())
        .collect()
}

fn positive_u32(value: Option<u32>, label: &str) -> Result<u32> {
    match value {
        Some(value) if value > 0 => Ok(value),
        _ => bail!("{label} must be positive"),
    }
}

fn positive_f64(value: Option<f64>, label: &str) -> Result<f64> {
    match value {
        Some(value) if value > 0.0 => Ok(value),
        _ => bail!("{label} must be positive"),
    }
}

fn validate_non_empty_string(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} must be a non-empty string");
    }
    Ok(())
}

fn floats_match(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

fn read_report(path: &Path) -> Result<BenchmarkReport> {
    let value: Value = read_json(path)
        .with_context(|| format!("failed to read benchmark report {}", path.display()))?;
    parse_report_value(value)
        .with_context(|| format!("failed to parse benchmark report {}", path.display()))
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(serde_json::from_reader(reader)?)
}

fn parse_report_value(value: Value) -> Result<BenchmarkReport> {
    let suite = value
        .get("suite")
        .and_then(Value::as_str)
        .context("benchmark report must define string field 'suite'")?;
    match suite {
        HEADLESS_SUITE => Ok(BenchmarkReport::Headless(
            serde_json::from_value::<BenchmarkSuiteReport>(value)
                .context("canonical-headless report shape is invalid")?,
        )),
        EXPECTED_LIVE_DISPLAY_SUITE => Ok(BenchmarkReport::LiveDisplay(
            serde_json::from_value::<LiveDisplayBenchmarkSuiteReport>(value)
                .context("live-display report shape is invalid")?,
        )),
        other => bail!("unsupported benchmark suite {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlledCpuScenarioRequirements, EnvironmentRequirements, EnvironmentScope,
        extract_environment_requirements, infer_report_environment_scope, parse_report_value,
        validate_report_against_environment_requirements,
    };
    use crate::report::BenchmarkReport;
    use crate::validate::tests::{valid_headless_report, valid_live_display_report};
    use std::collections::BTreeMap;

    #[test]
    fn headless_reports_are_portable_scope() {
        let report = BenchmarkReport::Headless(valid_headless_report());

        let scope = infer_report_environment_scope(&report).expect("headless scope should parse");
        assert_eq!(scope, EnvironmentScope::PortableHeadless);
        assert!(
            extract_environment_requirements(&report)
                .expect("headless requirements should parse")
                .is_none()
        );
    }

    #[test]
    fn live_display_scope_is_local_when_controlled_cpu_data_is_incomplete() {
        let mut report = valid_live_display_report();
        let cpu_result = report
            .results
            .iter_mut()
            .find(|result| result.backend == "cpu" && result.scenario == "steady-redraw-cpu")
            .expect("test report must include controlled cpu scenario");
        cpu_result.monitor_refresh_rate_millihz = None;

        let scope = infer_report_environment_scope(&BenchmarkReport::LiveDisplay(report))
            .expect("scope inference should succeed");
        assert_eq!(scope, EnvironmentScope::LocalDisplaySession);
    }

    #[test]
    fn controlled_live_display_reports_emit_environment_requirements() {
        let report = BenchmarkReport::LiveDisplay(valid_live_display_report());

        let requirements = extract_environment_requirements(&report)
            .expect("requirements extraction should succeed")
            .expect("controlled report should emit requirements");
        assert_eq!(requirements.display_server_hint, "wayland");
        assert_eq!(requirements.session_type.as_deref(), Some("wayland"));
        assert_eq!(
            requirements
                .cpu_scenarios
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                "resize-cycle-cpu".to_owned(),
                "steady-redraw-cpu".to_owned()
            ]
        );
    }

    #[test]
    fn environment_requirements_validation_rejects_mismatched_scale_factor() {
        let report = BenchmarkReport::LiveDisplay(valid_live_display_report());
        let requirements = EnvironmentRequirements {
            display_server_hint: "wayland".to_owned(),
            session_type: Some("wayland".to_owned()),
            cpu_scenarios: BTreeMap::from([
                (
                    "steady-redraw-cpu".to_owned(),
                    ControlledCpuScenarioRequirements {
                        pacing_mode: "monitor-cadence".to_owned(),
                        monitor_refresh_rate_millihz: 60_000,
                        monitor_scale_factor: 2.0,
                    },
                ),
                (
                    "resize-cycle-cpu".to_owned(),
                    ControlledCpuScenarioRequirements {
                        pacing_mode: "monitor-cadence".to_owned(),
                        monitor_refresh_rate_millihz: 60_000,
                        monitor_scale_factor: 2.0,
                    },
                ),
            ]),
        };

        let error = validate_report_against_environment_requirements(&report, &requirements)
            .expect_err("scale mismatch must fail");
        assert!(error.to_string().contains("monitor_scale_factor mismatch"));
    }

    #[test]
    fn parse_report_value_round_trips_headless_suite() {
        let value = serde_json::to_value(valid_headless_report())
            .expect("headless report should serialize");

        let parsed = parse_report_value(value).expect("headless report should deserialize");
        assert!(matches!(parsed, BenchmarkReport::Headless(_)));
    }

    #[test]
    fn parse_report_value_round_trips_live_display_suite() {
        let value = serde_json::to_value(valid_live_display_report())
            .expect("live display report should serialize");

        let parsed = parse_report_value(value).expect("live display report should deserialize");
        assert!(matches!(parsed, BenchmarkReport::LiveDisplay(_)));
    }
}
