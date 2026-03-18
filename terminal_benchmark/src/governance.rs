// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::cli::{
    CalibrationCli, CalibrationCommands, CalibrationEmitCli, CalibrationValidateCli,
    ComparisonModeArg, GovernanceCli, GovernanceCommands, LiveDisplayModeArg,
    RunnerReadinessCheckCli, RunnerReadinessCli, RunnerReadinessCommands,
    RunnerReadinessValidateCli, SuiteArg, SystemSuiteCli, SystemSuiteCommands, SystemSuiteEmitCli,
    SystemSuiteValidateCli, ThresholdCli, ThresholdCommands, ThresholdValidateCli, ValidateCli,
};
use crate::report::BenchmarkReport;
use crate::{environment, validate};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CALIBRATION_TOOL: &str = "terminal-display-calibration";
const RUNNER_READINESS_TOOL: &str = "terminal-display-runner-readiness";
const SYSTEM_SUITE_TOOL: &str = "terminal-system-suite";
const THRESHOLD_BASELINE_TOOL: &str = "terminal-benchmark-thresholds";
const THRESHOLD_MODE_ENFORCED: &str = "enforced";
const THRESHOLD_MODE_ADVISORY: &str = "advisory";
const THRESHOLD_MAX_MEAN_RATIO: &str = "max_mean_nanos_ratio";
const THRESHOLD_MAX_P95_RATIO: &str = "max_p95_nanos_ratio";
const THRESHOLD_MIN_PRIMARY_RATIO: &str = "min_primary_units_per_second_ratio";
const THRESHOLD_MIN_BYTES_RATIO: &str = "min_bytes_per_second_ratio";
const METRIC_MEAN_NANOS: &str = "mean_nanos";
const METRIC_P95_NANOS: &str = "p95_nanos";
const METRIC_PRIMARY_UNITS_PER_SECOND: &str = "primary_units_per_second";
const METRIC_BYTES_PER_SECOND: &str = "bytes_per_second";
const QUALITY_GATE_CARGO_FMT: &str = "cargo-fmt";
const QUALITY_GATE_CARGO_CHECK: &str = "cargo-check-workspace";
const QUALITY_GATE_CARGO_TEST: &str = "cargo-test-workspace";
const QUALITY_GATE_CARGO_CLIPPY: &str = "cargo-clippy-workspace";
const QUALITY_GATE_CARGO_MSRV_CHECK: &str = "cargo-msrv-check-workspace";
const QUALITY_GATE_CARGO_FUZZ_CHECK: &str = "cargo-check-fuzz-manifest";
const QUALITY_GATE_BENCHMARK_SMOKE: &str = "terminal-benchmark-smoke";
const QUALITY_GATE_BENCHMARK_FULL: &str = "terminal-benchmark-full";
const QUALITY_GATE_E2E_GOVERNANCE: &str = "terminal-e2e-governance";
const QUALITY_GATE_LIVE_DISPLAY_PREFIX: &str = "terminal-display-benchmark";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ReportStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedDisplayEnvironment {
    os: String,
    session_type: Option<String>,
    display_server_hint: String,
    display_env_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RunnerReadinessReport {
    system_tool: String,
    status: ReportStatus,
    generated_at_utc: String,
    os: String,
    session_type: Option<String>,
    display_server_hint: String,
    display_env_present: bool,
    required_session_type: Option<String>,
    required_display_server_hint: Option<String>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CalibrationReport {
    system_tool: String,
    status: ReportStatus,
    generated_at_utc: String,
    benchmark_report: PathBuf,
    baseline: PathBuf,
    comparison_mode: String,
    required_session_type: Option<String>,
    required_display_server_hint: Option<String>,
    runner_readiness_report: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SystemSuiteLiveDisplayReport {
    mode: String,
    report: PathBuf,
    baseline: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SystemSuiteReport {
    system_tool: String,
    status: ReportStatus,
    generated_at_utc: String,
    governance_mode: String,
    benchmark_report: PathBuf,
    benchmark_baseline: Option<PathBuf>,
    live_display: Option<SystemSuiteLiveDisplayReport>,
    quality_gates: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ThresholdBaseline {
    baseline_tool: String,
    benchmark_tool: String,
    suite: String,
    scale: String,
    comparison_mode: String,
    environment_scope: environment::EnvironmentScope,
    environment_requirements: Option<environment::EnvironmentRequirements>,
    defaults: HashMap<String, f64>,
    scenarios: HashMap<String, ThresholdScenarioPolicy>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ThresholdScenarioPolicy {
    baseline_metrics: HashMap<String, f64>,
    #[serde(default)]
    thresholds: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThresholdComparisonMode {
    Enforced,
    Advisory,
}

#[derive(Debug, Clone, Copy)]
struct ScenarioMetrics {
    mean_nanos: f64,
    p95_nanos: f64,
    primary_units_per_second: f64,
    bytes_per_second: Option<f64>,
}

pub fn run(args: &GovernanceCli) -> Result<()> {
    match &args.command {
        GovernanceCommands::RunnerReadiness(cli) => run_runner_readiness(cli),
        GovernanceCommands::Threshold(cli) => run_threshold(cli),
        GovernanceCommands::Calibration(cli) => run_calibration(cli),
        GovernanceCommands::SystemSuite(cli) => run_system_suite(cli),
    }
}

fn run_runner_readiness(args: &RunnerReadinessCli) -> Result<()> {
    match &args.command {
        RunnerReadinessCommands::Check(cli) => run_runner_readiness_check(cli),
        RunnerReadinessCommands::Validate(cli) => run_runner_readiness_validate(cli),
    }
}

fn run_calibration(args: &CalibrationCli) -> Result<()> {
    match &args.command {
        CalibrationCommands::Emit(cli) => run_calibration_emit(cli),
        CalibrationCommands::Validate(cli) => run_calibration_validate(cli),
    }
}

fn run_threshold(args: &ThresholdCli) -> Result<()> {
    match &args.command {
        ThresholdCommands::Validate(cli) => run_threshold_validate(cli),
    }
}

fn run_system_suite(args: &SystemSuiteCli) -> Result<()> {
    match &args.command {
        SystemSuiteCommands::Emit(cli) => run_system_suite_emit(cli),
        SystemSuiteCommands::Validate(cli) => run_system_suite_validate(cli),
    }
}

fn run_runner_readiness_check(args: &RunnerReadinessCheckCli) -> Result<()> {
    let observed = observe_display_environment();
    let report = build_runner_readiness_report(
        observed,
        args.require_session_type.clone(),
        args.require_display_server_hint.clone(),
    );
    validate_runner_readiness_report(&report, false)?;
    write_json(&args.report, &report)?;

    if args.require_pass && report.status != ReportStatus::Pass {
        let summary = report.errors.join("; ");
        bail!(
            "display benchmark runner readiness failed: {}{}",
            args.report.display(),
            if summary.is_empty() {
                String::new()
            } else {
                format!(" - {summary}")
            }
        );
    }

    println!(
        "display benchmark runner readiness ok: {}",
        args.report.display()
    );
    Ok(())
}

fn run_runner_readiness_validate(args: &RunnerReadinessValidateCli) -> Result<()> {
    let report: RunnerReadinessReport = read_json(&args.report).with_context(|| {
        format!(
            "failed to read runner readiness report {}",
            args.report.display()
        )
    })?;
    validate_runner_readiness_report(&report, args.require_pass)?;
    println!(
        "display benchmark runner readiness validation ok: {}",
        args.report.display()
    );
    Ok(())
}

fn run_calibration_emit(args: &CalibrationEmitCli) -> Result<()> {
    let report = CalibrationReport {
        system_tool: CALIBRATION_TOOL.to_owned(),
        status: ReportStatus::Pass,
        generated_at_utc: generated_at_utc_string(),
        benchmark_report: args.benchmark_report.clone(),
        baseline: args.baseline.clone(),
        comparison_mode: args.comparison_mode.as_str().to_owned(),
        required_session_type: args.required_session_type.clone(),
        required_display_server_hint: args.required_display_server_hint.clone(),
        runner_readiness_report: args.runner_readiness_report.clone(),
    };
    validate_calibration_report(
        &report,
        &CalibrationValidateCli {
            report: args.report.clone(),
            benchmark_report: args.benchmark_report.clone(),
            baseline: args.baseline.clone(),
            comparison_mode: args.comparison_mode,
            required_session_type: args.required_session_type.clone(),
            required_display_server_hint: args.required_display_server_hint.clone(),
            runner_readiness_report: args.runner_readiness_report.clone(),
        },
        false,
    )?;
    write_json(&args.report, &report)?;
    println!(
        "display benchmark calibration report emitted: {}",
        args.report.display()
    );
    Ok(())
}

fn run_calibration_validate(args: &CalibrationValidateCli) -> Result<()> {
    let report: CalibrationReport = read_json(&args.report).with_context(|| {
        format!(
            "failed to read calibration report {}",
            args.report.display()
        )
    })?;
    validate_calibration_report(&report, args, true)?;
    println!(
        "display benchmark calibration validation ok: {}",
        args.report.display()
    );
    Ok(())
}

fn run_threshold_validate(args: &ThresholdValidateCli) -> Result<()> {
    let mode = validate_threshold_baseline(&args.report, &args.baseline, args.allow_advisory)?;
    let mode_note = mode.as_str();
    println!(
        "benchmark threshold validation ok ({mode_note}): {} vs {}",
        args.report.display(),
        args.baseline.display()
    );
    Ok(())
}

fn run_system_suite_emit(args: &SystemSuiteEmitCli) -> Result<()> {
    let report = SystemSuiteReport {
        system_tool: SYSTEM_SUITE_TOOL.to_owned(),
        status: ReportStatus::Pass,
        generated_at_utc: generated_at_utc_string(),
        governance_mode: args.governance_mode.as_str().to_owned(),
        benchmark_report: args.benchmark_report.clone(),
        benchmark_baseline: args.benchmark_baseline.clone(),
        live_display: build_system_suite_live_display(
            args.live_display_mode,
            args.live_display_report.clone(),
            args.live_display_baseline.clone(),
        )?,
        quality_gates: args.quality_gate.clone(),
    };
    validate_system_suite_report(
        &report,
        &SystemSuiteValidateCli {
            report: args.report.clone(),
            benchmark_report: args.benchmark_report.clone(),
            governance_mode: args.governance_mode,
            benchmark_baseline: args.benchmark_baseline.clone(),
            live_display_mode: args.live_display_mode,
            live_display_report: args.live_display_report.clone(),
            live_display_baseline: args.live_display_baseline.clone(),
        },
        false,
    )?;
    write_json(&args.report, &report)?;
    println!(
        "terminal system suite report emitted: {}",
        args.report.display()
    );
    Ok(())
}

fn run_system_suite_validate(args: &SystemSuiteValidateCli) -> Result<()> {
    let report: SystemSuiteReport = read_json(&args.report).with_context(|| {
        format!(
            "failed to read terminal system suite report {}",
            args.report.display()
        )
    })?;
    validate_system_suite_report(&report, args, true)?;
    println!(
        "terminal system suite validation ok: {}",
        args.report.display()
    );
    Ok(())
}

fn observe_display_environment() -> ObservedDisplayEnvironment {
    let os = match env::consts::OS {
        "linux" => "Linux".to_owned(),
        "macos" => "Darwin".to_owned(),
        other => other.to_owned(),
    };
    let session_type = non_empty_env("XDG_SESSION_TYPE");

    let wayland_present = env_flag_present("WAYLAND_DISPLAY");
    let x11_present = env_flag_present("DISPLAY");
    let (display_server_hint, display_env_present) = match os.as_str() {
        "Linux" if wayland_present => ("wayland".to_owned(), true),
        "Linux" if x11_present => ("x11".to_owned(), true),
        "Linux" => ("unknown".to_owned(), false),
        "Darwin" => ("appkit".to_owned(), true),
        _ => ("unknown".to_owned(), false),
    };

    ObservedDisplayEnvironment {
        os,
        session_type,
        display_server_hint,
        display_env_present,
    }
}

fn build_runner_readiness_report(
    observed: ObservedDisplayEnvironment,
    required_session_type: Option<String>,
    required_display_server_hint: Option<String>,
) -> RunnerReadinessReport {
    let mut errors = Vec::new();
    if observed.os == "Linux" && !observed.display_env_present {
        errors.push(
            "linux self-hosted runner requires DISPLAY or WAYLAND_DISPLAY for display benchmark calibration"
                .to_owned(),
        );
    }
    if let Some(required) = required_session_type.as_deref()
        && observed.session_type.as_deref() != Some(required)
    {
        errors.push(format!(
            "required session_type '{required}' does not match detected '{}'",
            observed.session_type.as_deref().unwrap_or("")
        ));
    }
    if let Some(required) = required_display_server_hint.as_deref()
        && observed.display_server_hint != required
    {
        errors.push(format!(
            "required display_server_hint '{required}' does not match detected '{}'",
            observed.display_server_hint
        ));
    }

    let status = if errors.is_empty() {
        ReportStatus::Pass
    } else {
        ReportStatus::Fail
    };

    RunnerReadinessReport {
        system_tool: RUNNER_READINESS_TOOL.to_owned(),
        status,
        generated_at_utc: generated_at_utc_string(),
        os: observed.os,
        session_type: observed.session_type,
        display_server_hint: observed.display_server_hint,
        display_env_present: observed.display_env_present,
        required_session_type,
        required_display_server_hint,
        errors,
    }
}

fn validate_runner_readiness_report(
    report: &RunnerReadinessReport,
    require_pass: bool,
) -> Result<()> {
    validate_non_empty_string(&report.system_tool, "system_tool")?;
    if report.system_tool != RUNNER_READINESS_TOOL {
        bail!("system_tool must be {RUNNER_READINESS_TOOL:?}");
    }
    validate_non_empty_string(&report.generated_at_utc, "generated_at_utc")?;
    validate_non_empty_string(&report.os, "os")?;
    validate_optional_non_empty_string(report.session_type.as_deref(), "session_type")?;
    validate_non_empty_string(&report.display_server_hint, "display_server_hint")?;
    validate_optional_non_empty_string(
        report.required_session_type.as_deref(),
        "required_session_type",
    )?;
    validate_optional_non_empty_string(
        report.required_display_server_hint.as_deref(),
        "required_display_server_hint",
    )?;
    if report.status == ReportStatus::Pass && !report.errors.is_empty() {
        bail!("errors must be empty when status is 'pass'");
    }
    if report.status == ReportStatus::Fail && report.errors.is_empty() {
        bail!("errors must be non-empty when status is 'fail'");
    }
    for error in &report.errors {
        validate_non_empty_string(error, "errors entry")?;
    }
    if report.status == ReportStatus::Pass {
        if let Some(required) = report.required_session_type.as_deref()
            && report.session_type.as_deref() != Some(required)
        {
            bail!(
                "pass readiness report required_session_type {:?} does not match session_type {:?}",
                required,
                report.session_type
            );
        }
        if let Some(required) = report.required_display_server_hint.as_deref()
            && report.display_server_hint != required
        {
            bail!(
                "pass readiness report required_display_server_hint {:?} does not match display_server_hint {:?}",
                required,
                report.display_server_hint
            );
        }
    }
    if require_pass && report.status != ReportStatus::Pass {
        bail!("status must be 'pass', got {:?}", report.status);
    }
    Ok(())
}

fn validate_calibration_report(
    report: &CalibrationReport,
    args: &CalibrationValidateCli,
    require_inputs: bool,
) -> Result<()> {
    validate_non_empty_string(&report.system_tool, "system_tool")?;
    if report.system_tool != CALIBRATION_TOOL {
        bail!("system_tool must be {CALIBRATION_TOOL:?}");
    }
    if report.status != ReportStatus::Pass {
        bail!("status must be 'pass'");
    }
    validate_non_empty_string(&report.generated_at_utc, "generated_at_utc")?;
    validate_non_empty_string(&report.comparison_mode, "comparison_mode")?;

    if report.benchmark_report != args.benchmark_report {
        bail!(
            "benchmark_report must be {:?}, got {:?}",
            args.benchmark_report,
            report.benchmark_report
        );
    }
    if report.baseline != args.baseline {
        bail!(
            "baseline must be {:?}, got {:?}",
            args.baseline,
            report.baseline
        );
    }

    let expected_mode = args.comparison_mode.as_str();
    if report.comparison_mode != expected_mode {
        bail!(
            "comparison_mode must be {:?}, got {:?}",
            expected_mode,
            report.comparison_mode
        );
    }

    if report.required_session_type != args.required_session_type {
        bail!(
            "required_session_type must be {:?}, got {:?}",
            args.required_session_type,
            report.required_session_type
        );
    }
    if report.required_display_server_hint != args.required_display_server_hint {
        bail!(
            "required_display_server_hint must be {:?}, got {:?}",
            args.required_display_server_hint,
            report.required_display_server_hint
        );
    }
    if report.runner_readiness_report != args.runner_readiness_report {
        bail!(
            "runner_readiness_report must be {:?}, got {:?}",
            args.runner_readiness_report,
            report.runner_readiness_report
        );
    }

    validate_optional_non_empty_string(
        report.required_session_type.as_deref(),
        "required_session_type",
    )?;
    validate_optional_non_empty_string(
        report.required_display_server_hint.as_deref(),
        "required_display_server_hint",
    )?;

    if require_inputs {
        if !args.benchmark_report.is_file() {
            bail!(
                "benchmark report does not exist: {}",
                args.benchmark_report.display()
            );
        }
        if !args.baseline.is_file() {
            bail!("baseline does not exist: {}", args.baseline.display());
        }
    }

    validate::run(&ValidateCli {
        suite: SuiteArg::LiveDisplay,
        report: args.benchmark_report.clone(),
        require_scenario: Vec::new(),
        require_full_suite: true,
    })?;

    if require_inputs {
        let benchmark_report =
            environment::read_report(&args.benchmark_report).with_context(|| {
                format!(
                    "failed to parse benchmark report {}",
                    args.benchmark_report.display()
                )
            })?;
        validate_required_live_display_environment(
            &benchmark_report,
            args.required_session_type.as_deref(),
            args.required_display_server_hint.as_deref(),
        )?;

        validate_threshold_baseline(
            &args.benchmark_report,
            &args.baseline,
            args.comparison_mode == ComparisonModeArg::Advisory,
        )?;
    }

    if let Some(path) = args.runner_readiness_report.as_ref() {
        if require_inputs && !path.is_file() {
            bail!("runner readiness report does not exist: {}", path.display());
        }
        let readiness: RunnerReadinessReport = read_json(path).with_context(|| {
            format!("failed to read runner readiness report {}", path.display())
        })?;
        validate_runner_readiness_report(&readiness, true)?;
        if readiness.required_session_type != args.required_session_type {
            bail!(
                "runner readiness required_session_type must be {:?}, got {:?}",
                args.required_session_type,
                readiness.required_session_type
            );
        }
        if readiness.required_display_server_hint != args.required_display_server_hint {
            bail!(
                "runner readiness required_display_server_hint must be {:?}, got {:?}",
                args.required_display_server_hint,
                readiness.required_display_server_hint
            );
        }
        if let Some(required) = args.required_session_type.as_deref()
            && readiness.session_type.as_deref() != Some(required)
        {
            bail!(
                "runner readiness session_type {:?} does not satisfy required_session_type {:?}",
                readiness.session_type,
                required
            );
        }
        if let Some(required) = args.required_display_server_hint.as_deref()
            && readiness.display_server_hint != required
        {
            bail!(
                "runner readiness display_server_hint {:?} does not satisfy required_display_server_hint {:?}",
                readiness.display_server_hint,
                required
            );
        }
    }

    Ok(())
}

fn validate_required_live_display_environment(
    report: &BenchmarkReport,
    required_session_type: Option<&str>,
    required_display_server_hint: Option<&str>,
) -> Result<()> {
    let live_display = match report {
        BenchmarkReport::LiveDisplay(report) => report,
        BenchmarkReport::Headless(_) => {
            bail!("calibration validation requires a live-display benchmark report");
        }
    };

    if let Some(required) = required_session_type
        && live_display.environment.session_type.as_deref() != Some(required)
    {
        bail!(
            "benchmark report session_type {:?} does not satisfy required_session_type {:?}",
            live_display.environment.session_type,
            required
        );
    }
    if let Some(required) = required_display_server_hint
        && live_display.environment.display_server_hint != required
    {
        bail!(
            "benchmark report display_server_hint {:?} does not satisfy required_display_server_hint {:?}",
            live_display.environment.display_server_hint,
            required
        );
    }

    Ok(())
}

fn build_system_suite_live_display(
    live_display_mode: Option<LiveDisplayModeArg>,
    live_display_report: Option<PathBuf>,
    live_display_baseline: Option<PathBuf>,
) -> Result<Option<SystemSuiteLiveDisplayReport>> {
    match (
        live_display_mode,
        live_display_report,
        live_display_baseline,
    ) {
        (None, None, None) => Ok(None),
        (None, _, _) => bail!(
            "live_display report and baseline must be omitted when live_display_mode is not set"
        ),
        (Some(mode), Some(report), baseline) => Ok(Some(SystemSuiteLiveDisplayReport {
            mode: mode.as_str().to_owned(),
            report,
            baseline,
        })),
        (Some(_), None, _) => {
            bail!("live_display_report is required when live_display_mode is set")
        }
    }
}

fn validate_system_suite_report(
    report: &SystemSuiteReport,
    args: &SystemSuiteValidateCli,
    require_inputs: bool,
) -> Result<()> {
    validate_non_empty_string(&report.system_tool, "system_tool")?;
    if report.system_tool != SYSTEM_SUITE_TOOL {
        bail!("system_tool must be {SYSTEM_SUITE_TOOL:?}");
    }
    if report.status != ReportStatus::Pass {
        bail!("status must be 'pass'");
    }
    validate_non_empty_string(&report.generated_at_utc, "generated_at_utc")?;
    validate_non_empty_string(&report.governance_mode, "governance_mode")?;

    let expected_governance_mode = args.governance_mode.as_str();
    if report.governance_mode != expected_governance_mode {
        bail!(
            "governance_mode must be {:?}, got {:?}",
            expected_governance_mode,
            report.governance_mode
        );
    }
    if report.benchmark_report != args.benchmark_report {
        bail!(
            "benchmark_report must be {:?}, got {:?}",
            args.benchmark_report,
            report.benchmark_report
        );
    }
    if report.benchmark_baseline != args.benchmark_baseline {
        bail!(
            "benchmark_baseline must be {:?}, got {:?}",
            args.benchmark_baseline,
            report.benchmark_baseline
        );
    }

    let expected_live_display = build_system_suite_live_display(
        args.live_display_mode,
        args.live_display_report.clone(),
        args.live_display_baseline.clone(),
    )?;
    if report.live_display != expected_live_display {
        bail!(
            "live_display must be {:?}, got {:?}",
            expected_live_display,
            report.live_display
        );
    }

    if report.quality_gates.is_empty() {
        bail!("quality_gates must be a non-empty list");
    }
    for gate in &report.quality_gates {
        validate_non_empty_string(gate, "quality_gates entry")?;
    }

    let expected_gates = expected_system_suite_quality_gates(args);
    if report.quality_gates != expected_gates {
        bail!(
            "quality_gates mismatch: expected {:?}, got {:?}",
            expected_gates,
            report.quality_gates
        );
    }

    if require_inputs {
        if !args.benchmark_report.is_file() {
            bail!(
                "benchmark report does not exist: {}",
                args.benchmark_report.display()
            );
        }
        if let Some(path) = args.benchmark_baseline.as_ref()
            && !path.is_file()
        {
            bail!("benchmark baseline does not exist: {}", path.display());
        }
        if let Some(path) = args.live_display_report.as_ref()
            && !path.is_file()
        {
            bail!("live display report does not exist: {}", path.display());
        }
        if let Some(path) = args.live_display_baseline.as_ref()
            && !path.is_file()
        {
            bail!("live display baseline does not exist: {}", path.display());
        }
    }

    validate::run(&ValidateCli {
        suite: SuiteArg::CanonicalHeadless,
        report: args.benchmark_report.clone(),
        require_scenario: Vec::new(),
        require_full_suite: true,
    })?;

    if require_inputs && let Some(path) = args.benchmark_baseline.as_ref() {
        validate_threshold_baseline(&args.benchmark_report, path, false)?;
    }

    if let Some(path) = args.live_display_report.as_ref() {
        validate::run(&ValidateCli {
            suite: SuiteArg::LiveDisplay,
            report: path.clone(),
            require_scenario: Vec::new(),
            require_full_suite: true,
        })?;

        if require_inputs && let Some(baseline_path) = args.live_display_baseline.as_ref() {
            validate_threshold_baseline(path, baseline_path, true)?;
        }
    }

    Ok(())
}

impl ThresholdComparisonMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            THRESHOLD_MODE_ENFORCED => Ok(Self::Enforced),
            THRESHOLD_MODE_ADVISORY => Ok(Self::Advisory),
            _ => bail!(
                "comparison_mode must be {:?} or {:?}",
                THRESHOLD_MODE_ENFORCED,
                THRESHOLD_MODE_ADVISORY
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => THRESHOLD_MODE_ENFORCED,
            Self::Advisory => THRESHOLD_MODE_ADVISORY,
        }
    }
}

fn validate_threshold_baseline(
    report_path: &Path,
    baseline_path: &Path,
    allow_advisory: bool,
) -> Result<ThresholdComparisonMode> {
    let report: BenchmarkReport = environment::read_report(report_path)?;
    let baseline: ThresholdBaseline = read_json(baseline_path).with_context(|| {
        format!(
            "failed to read threshold baseline {}",
            baseline_path.display()
        )
    })?;

    validate_non_empty_string(&baseline.baseline_tool, "baseline.baseline_tool")?;
    if baseline.baseline_tool != THRESHOLD_BASELINE_TOOL {
        bail!("baseline_tool must be {THRESHOLD_BASELINE_TOOL:?}");
    }
    validate_non_empty_string(&baseline.benchmark_tool, "baseline.benchmark_tool")?;
    validate_non_empty_string(&baseline.suite, "baseline.suite")?;
    validate_non_empty_string(&baseline.scale, "baseline.scale")?;
    validate_non_empty_string(&baseline.comparison_mode, "baseline.comparison_mode")?;
    if baseline.scenarios.is_empty() {
        bail!("baseline.scenarios must be a non-empty object");
    }

    let comparison_mode = ThresholdComparisonMode::parse(&baseline.comparison_mode)?;
    if comparison_mode == ThresholdComparisonMode::Advisory && !allow_advisory {
        bail!(
            "baseline comparison_mode is advisory; rerun with --allow-advisory to acknowledge environment-specific thresholds"
        );
    }

    let (report_benchmark_tool, report_suite, report_scale, report_metrics) =
        report_metadata_and_metrics(&report)?;
    if baseline.benchmark_tool != report_benchmark_tool {
        bail!(
            "benchmark_tool mismatch between report and baseline: report={:?} baseline={:?}",
            report_benchmark_tool,
            baseline.benchmark_tool
        );
    }
    if baseline.suite != report_suite {
        bail!(
            "suite mismatch between report and baseline: report={:?} baseline={:?}",
            report_suite,
            baseline.suite
        );
    }
    if baseline.scale != report_scale {
        bail!(
            "scale mismatch between report and baseline: report={:?} baseline={:?}",
            report_scale,
            baseline.scale
        );
    }

    let report_scope = environment::infer_report_environment_scope(&report)?;
    if report_scope != baseline.environment_scope {
        bail!(
            "environment_scope mismatch between report and baseline: report={:?} baseline={:?}",
            report_scope,
            baseline.environment_scope
        );
    }
    if baseline.environment_scope == environment::EnvironmentScope::ControlledDisplaySession
        && baseline.environment_requirements.is_none()
    {
        bail!("controlled-display-session baselines must declare environment_requirements");
    }
    if let Some(requirements) = baseline.environment_requirements.as_ref() {
        environment::validate_report_against_environment_requirements(&report, requirements)?;
    }

    let baseline_names = baseline.scenarios.keys().cloned().collect::<BTreeSet<_>>();
    let report_names = report_metrics.keys().cloned().collect::<BTreeSet<_>>();
    if baseline_names != report_names {
        bail!(
            "scenario set mismatch between report and baseline: baseline={:?} report={:?}",
            baseline_names,
            report_names
        );
    }

    let mut advisory_violations = Vec::new();

    for (scenario, policy) in &baseline.scenarios {
        if policy.baseline_metrics.is_empty() {
            bail!(
                "baseline scenario {:?} baseline_metrics must be an object",
                scenario
            );
        }
        let current = report_metrics
            .get(scenario)
            .with_context(|| format!("scenario {:?} is missing from report metrics", scenario))?;

        let mean_ratio =
            threshold_ratio_value(&baseline, policy, scenario, THRESHOLD_MAX_MEAN_RATIO, true)?
                .expect("required ratio must be present");
        let p95_ratio =
            threshold_ratio_value(&baseline, policy, scenario, THRESHOLD_MAX_P95_RATIO, true)?
                .expect("required ratio must be present");
        let primary_ratio = threshold_ratio_value(
            &baseline,
            policy,
            scenario,
            THRESHOLD_MIN_PRIMARY_RATIO,
            true,
        )?
        .expect("required ratio must be present");

        let baseline_mean = baseline_metric_value(policy, scenario, METRIC_MEAN_NANOS)?;
        let baseline_p95 = baseline_metric_value(policy, scenario, METRIC_P95_NANOS)?;
        let baseline_primary =
            baseline_metric_value(policy, scenario, METRIC_PRIMARY_UNITS_PER_SECOND)?;

        collect_regression(
            compare_max_ratio(
                scenario,
                METRIC_MEAN_NANOS,
                current.mean_nanos,
                baseline_mean,
                mean_ratio,
            )?,
            comparison_mode,
            &mut advisory_violations,
        )?;
        collect_regression(
            compare_max_ratio(
                scenario,
                METRIC_P95_NANOS,
                current.p95_nanos,
                baseline_p95,
                p95_ratio,
            )?,
            comparison_mode,
            &mut advisory_violations,
        )?;
        collect_regression(
            compare_min_ratio(
                scenario,
                METRIC_PRIMARY_UNITS_PER_SECOND,
                current.primary_units_per_second,
                baseline_primary,
                primary_ratio,
            )?,
            comparison_mode,
            &mut advisory_violations,
        )?;

        if let Some(bytes_ratio) = threshold_ratio_value(
            &baseline,
            policy,
            scenario,
            THRESHOLD_MIN_BYTES_RATIO,
            false,
        )? {
            let baseline_bytes = policy
                .baseline_metrics
                .get(METRIC_BYTES_PER_SECOND)
                .copied();
            let current_bytes = current.bytes_per_second;
            if let (Some(baseline_bytes), Some(current_bytes)) = (baseline_bytes, current_bytes)
                && baseline_bytes > 0.0
            {
                collect_regression(
                    compare_min_ratio(
                        scenario,
                        METRIC_BYTES_PER_SECOND,
                        current_bytes,
                        baseline_bytes,
                        bytes_ratio,
                    )?,
                    comparison_mode,
                    &mut advisory_violations,
                )?;
            }
        }
    }

    if !advisory_violations.is_empty() {
        eprintln!(
            "benchmark threshold validation advisory regressions ({}): {} vs {}",
            advisory_violations.len(),
            report_path.display(),
            baseline_path.display()
        );
        for violation in advisory_violations {
            eprintln!("- {violation}");
        }
    }

    Ok(comparison_mode)
}

fn threshold_ratio_value(
    baseline: &ThresholdBaseline,
    policy: &ThresholdScenarioPolicy,
    scenario: &str,
    key: &str,
    required: bool,
) -> Result<Option<f64>> {
    let ratio = policy
        .thresholds
        .get(key)
        .copied()
        .or_else(|| baseline.defaults.get(key).copied());

    if required && ratio.is_none() {
        bail!("scenario {:?} threshold {} must be defined", scenario, key);
    }

    if let Some(value) = ratio
        && (!value.is_finite() || value <= 0.0)
    {
        bail!(
            "scenario {:?} threshold {} must be a positive finite number",
            scenario,
            key
        );
    }

    Ok(ratio)
}

fn baseline_metric_value(
    policy: &ThresholdScenarioPolicy,
    scenario: &str,
    key: &str,
) -> Result<f64> {
    let value =
        policy.baseline_metrics.get(key).copied().with_context(|| {
            format!("baseline scenario {:?} is missing metric {}", scenario, key)
        })?;
    if !value.is_finite() {
        bail!(
            "baseline scenario {:?} metric {} must be a finite number",
            scenario,
            key
        );
    }
    Ok(value)
}

fn report_metadata_and_metrics(
    report: &BenchmarkReport,
) -> Result<(String, String, String, HashMap<String, ScenarioMetrics>)> {
    let (benchmark_tool, suite, scale, metrics) = match report {
        BenchmarkReport::Headless(report) => (
            report.benchmark_tool.clone(),
            report.suite.clone(),
            report.scale.clone(),
            report
                .results
                .iter()
                .map(|result| {
                    (
                        result.scenario.clone(),
                        ScenarioMetrics {
                            mean_nanos: result.stats.mean_nanos as f64,
                            p95_nanos: result.stats.p95_nanos as f64,
                            primary_units_per_second: result.primary_units_per_second,
                            bytes_per_second: Some(result.bytes_per_second),
                        },
                    )
                })
                .collect::<Vec<_>>(),
        ),
        BenchmarkReport::LiveDisplay(report) => (
            report.benchmark_tool.clone(),
            report.suite.clone(),
            report.scale.clone(),
            report
                .results
                .iter()
                .map(|result| {
                    (
                        result.scenario.clone(),
                        ScenarioMetrics {
                            mean_nanos: result.stats.mean_nanos as f64,
                            p95_nanos: result.stats.p95_nanos as f64,
                            primary_units_per_second: result.primary_units_per_second,
                            bytes_per_second: None,
                        },
                    )
                })
                .collect::<Vec<_>>(),
        ),
    };

    validate_non_empty_string(&benchmark_tool, "report.benchmark_tool")?;
    validate_non_empty_string(&suite, "report.suite")?;
    validate_non_empty_string(&scale, "report.scale")?;

    let mut scenario_metrics = HashMap::new();
    for (scenario, metrics) in metrics {
        validate_non_empty_string(&scenario, "report.results[].scenario")?;
        if scenario_metrics.insert(scenario.clone(), metrics).is_some() {
            bail!("report.results contains duplicate scenario {:?}", scenario);
        }
    }
    if scenario_metrics.is_empty() {
        bail!("report.results must be a non-empty list");
    }

    Ok((benchmark_tool, suite, scale, scenario_metrics))
}

fn compare_max_ratio(
    scenario: &str,
    metric_name: &str,
    current: f64,
    baseline: f64,
    ratio: f64,
) -> Result<Option<String>> {
    if baseline <= 0.0 {
        bail!(
            "scenario {:?} baseline {} must be > 0",
            scenario,
            metric_name
        );
    }
    let limit = baseline * ratio;
    if current > limit {
        return Ok(Some(format!(
            "scenario {:?} {} regression: current={:.6} exceeds baseline={:.6} * ratio={:.3} (limit={:.6})",
            scenario, metric_name, current, baseline, ratio, limit
        )));
    }
    Ok(None)
}

fn compare_min_ratio(
    scenario: &str,
    metric_name: &str,
    current: f64,
    baseline: f64,
    ratio: f64,
) -> Result<Option<String>> {
    if baseline <= 0.0 {
        bail!(
            "scenario {:?} baseline {} must be > 0",
            scenario,
            metric_name
        );
    }
    let floor = baseline * ratio;
    if current < floor {
        return Ok(Some(format!(
            "scenario {:?} {} regression: current={:.6} is below baseline={:.6} * ratio={:.3} (floor={:.6})",
            scenario, metric_name, current, baseline, ratio, floor
        )));
    }
    Ok(None)
}

fn collect_regression(
    regression: Option<String>,
    mode: ThresholdComparisonMode,
    advisory_violations: &mut Vec<String>,
) -> Result<()> {
    if let Some(regression) = regression {
        if mode == ThresholdComparisonMode::Advisory {
            advisory_violations.push(regression);
        } else {
            bail!("{regression}");
        }
    }
    Ok(())
}

fn expected_system_suite_quality_gates(args: &SystemSuiteValidateCli) -> Vec<String> {
    let mut gates = vec![
        QUALITY_GATE_CARGO_FMT.to_owned(),
        QUALITY_GATE_CARGO_CHECK.to_owned(),
        QUALITY_GATE_CARGO_TEST.to_owned(),
        QUALITY_GATE_CARGO_CLIPPY.to_owned(),
        QUALITY_GATE_CARGO_MSRV_CHECK.to_owned(),
        QUALITY_GATE_CARGO_FUZZ_CHECK.to_owned(),
        QUALITY_GATE_BENCHMARK_SMOKE.to_owned(),
        QUALITY_GATE_BENCHMARK_FULL.to_owned(),
        QUALITY_GATE_E2E_GOVERNANCE.to_owned(),
    ];

    if let Some(mode) = args.live_display_mode {
        gates.push(format!(
            "{QUALITY_GATE_LIVE_DISPLAY_PREFIX}-{}",
            mode.as_str()
        ));
    }

    gates
}

fn generated_at_utc_string() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("unix-seconds-utc:{}", duration.as_secs()),
        Err(_) => "unix-seconds-utc:0".to_owned(),
    }
}

fn env_flag_present(name: &str) -> bool {
    env::var(name)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
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

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(serde_json::from_reader(reader)?)
}

#[cfg(test)]
mod tests {
    use super::{
        CalibrationReport, METRIC_BYTES_PER_SECOND, METRIC_MEAN_NANOS, METRIC_P95_NANOS,
        METRIC_PRIMARY_UNITS_PER_SECOND, ObservedDisplayEnvironment, ReportStatus,
        RunnerReadinessReport, SystemSuiteLiveDisplayReport, SystemSuiteReport,
        THRESHOLD_BASELINE_TOOL, THRESHOLD_MAX_MEAN_RATIO, THRESHOLD_MAX_P95_RATIO,
        THRESHOLD_MIN_BYTES_RATIO, THRESHOLD_MIN_PRIMARY_RATIO, THRESHOLD_MODE_ADVISORY,
        THRESHOLD_MODE_ENFORCED, ThresholdBaseline, ThresholdComparisonMode,
        ThresholdScenarioPolicy, build_runner_readiness_report,
        expected_system_suite_quality_gates, validate_calibration_report,
        validate_runner_readiness_report, validate_system_suite_report,
        validate_threshold_baseline, write_json,
    };
    use crate::cli::{
        CalibrationValidateCli, ComparisonModeArg, GovernanceModeArg, LiveDisplayModeArg,
        SystemSuiteValidateCli,
    };
    use crate::environment;
    use crate::report::BenchmarkReport;
    use crate::validate::tests::{valid_headless_report, valid_live_display_report};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn linux_runner_without_display_fails_readiness() {
        let report = build_runner_readiness_report(
            ObservedDisplayEnvironment {
                os: "Linux".to_owned(),
                session_type: Some("wayland".to_owned()),
                display_server_hint: "unknown".to_owned(),
                display_env_present: false,
            },
            Some("wayland".to_owned()),
            Some("wayland".to_owned()),
        );

        assert_eq!(report.status, ReportStatus::Fail);
        assert_eq!(report.errors.len(), 2);
    }

    #[test]
    fn readiness_validation_requires_errors_when_failed() {
        let report = RunnerReadinessReport {
            system_tool: "terminal-display-runner-readiness".to_owned(),
            status: ReportStatus::Fail,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            os: "Linux".to_owned(),
            session_type: Some("wayland".to_owned()),
            display_server_hint: "wayland".to_owned(),
            display_env_present: true,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            errors: Vec::new(),
        };

        let error = validate_runner_readiness_report(&report, false)
            .expect_err("failed reports need errors");
        assert!(error.to_string().contains("errors must be non-empty"));
    }

    #[test]
    fn readiness_validation_rejects_pass_required_session_mismatch() {
        let report = RunnerReadinessReport {
            system_tool: "terminal-display-runner-readiness".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            os: "Linux".to_owned(),
            session_type: Some("x11".to_owned()),
            display_server_hint: "wayland".to_owned(),
            display_env_present: true,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            errors: Vec::new(),
        };

        let error = validate_runner_readiness_report(&report, true)
            .expect_err("pass report must align required and observed session_type");
        assert!(error.to_string().contains("required_session_type"));
    }

    #[test]
    fn calibration_validation_accepts_matching_reports() {
        let temp_dir = temp_dir("calibration_validation_accepts_matching_reports");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");
        let readiness_path = temp_dir.join("readiness.json");

        let benchmark_report = valid_live_display_report();
        benchmark_report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(benchmark_report.clone()),
            &baseline_path,
            "advisory",
        );

        let readiness = RunnerReadinessReport {
            system_tool: "terminal-display-runner-readiness".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            os: "Linux".to_owned(),
            session_type: Some("wayland".to_owned()),
            display_server_hint: "wayland".to_owned(),
            display_env_present: true,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            errors: Vec::new(),
        };
        write_json(&readiness_path, &readiness).expect("readiness report should write");

        let report = CalibrationReport {
            system_tool: "terminal-display-calibration".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            benchmark_report: benchmark_report_path.clone(),
            baseline: baseline_path.clone(),
            comparison_mode: "advisory".to_owned(),
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: Some(readiness_path.clone()),
        };
        let args = CalibrationValidateCli {
            report: temp_dir.join("calibration.json"),
            benchmark_report: benchmark_report_path,
            baseline: baseline_path,
            comparison_mode: ComparisonModeArg::Advisory,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: Some(readiness_path),
        };

        validate_calibration_report(&report, &args, true)
            .expect("matching calibration inputs should validate");
    }

    #[test]
    fn calibration_validation_rejects_required_session_type_mismatch() {
        let temp_dir = temp_dir("calibration_validation_rejects_required_session_type_mismatch");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");

        let benchmark_report = valid_live_display_report();
        benchmark_report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(benchmark_report.clone()),
            &baseline_path,
            "advisory",
        );

        let report = CalibrationReport {
            system_tool: "terminal-display-calibration".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            benchmark_report: benchmark_report_path.clone(),
            baseline: baseline_path.clone(),
            comparison_mode: "advisory".to_owned(),
            required_session_type: None,
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };
        let args = CalibrationValidateCli {
            report: temp_dir.join("calibration.json"),
            benchmark_report: benchmark_report_path,
            baseline: baseline_path,
            comparison_mode: ComparisonModeArg::Advisory,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };

        let error = validate_calibration_report(&report, &args, false)
            .expect_err("required session type mismatch must fail");
        assert!(error.to_string().contains("required_session_type"));
    }

    #[test]
    fn calibration_validation_rejects_benchmark_environment_mismatch() {
        let temp_dir = temp_dir("calibration_validation_rejects_benchmark_environment_mismatch");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");

        let mut benchmark_report = valid_live_display_report();
        benchmark_report.environment.session_type = Some("x11".to_owned());
        benchmark_report.environment.display_server_hint = "x11".to_owned();
        benchmark_report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(benchmark_report.clone()),
            &baseline_path,
            "advisory",
        );

        let report = CalibrationReport {
            system_tool: "terminal-display-calibration".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            benchmark_report: benchmark_report_path.clone(),
            baseline: baseline_path.clone(),
            comparison_mode: "advisory".to_owned(),
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };
        let args = CalibrationValidateCli {
            report: temp_dir.join("calibration.json"),
            benchmark_report: benchmark_report_path,
            baseline: baseline_path,
            comparison_mode: ComparisonModeArg::Advisory,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };

        let error = validate_calibration_report(&report, &args, true)
            .expect_err("benchmark report environment mismatch must fail");
        assert!(error.to_string().contains("benchmark report session_type"));
    }

    #[test]
    fn calibration_validation_rejects_runner_readiness_requirement_mismatch() {
        let temp_dir =
            temp_dir("calibration_validation_rejects_runner_readiness_requirement_mismatch");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");
        let readiness_path = temp_dir.join("readiness.json");

        let benchmark_report = valid_live_display_report();
        benchmark_report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(benchmark_report.clone()),
            &baseline_path,
            "advisory",
        );

        let readiness = RunnerReadinessReport {
            system_tool: "terminal-display-runner-readiness".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            os: "Linux".to_owned(),
            session_type: Some("wayland".to_owned()),
            display_server_hint: "wayland".to_owned(),
            display_env_present: true,
            required_session_type: None,
            required_display_server_hint: Some("wayland".to_owned()),
            errors: Vec::new(),
        };
        write_json(&readiness_path, &readiness).expect("readiness report should write");

        let report = CalibrationReport {
            system_tool: "terminal-display-calibration".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            benchmark_report: benchmark_report_path.clone(),
            baseline: baseline_path.clone(),
            comparison_mode: "advisory".to_owned(),
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: Some(readiness_path.clone()),
        };
        let args = CalibrationValidateCli {
            report: temp_dir.join("calibration.json"),
            benchmark_report: benchmark_report_path,
            baseline: baseline_path,
            comparison_mode: ComparisonModeArg::Advisory,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: Some(readiness_path),
        };

        let error = validate_calibration_report(&report, &args, true)
            .expect_err("runner readiness requirement mismatch must fail");
        assert!(
            error
                .to_string()
                .contains("runner readiness required_session_type")
        );
    }

    #[test]
    fn calibration_validation_rejects_threshold_regression() {
        let temp_dir = temp_dir("calibration_validation_rejects_threshold_regression");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");

        let baseline_report = valid_live_display_report();
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(baseline_report.clone()),
            &baseline_path,
            "enforced",
        );

        let mut regressed_report = baseline_report.clone();
        let first = regressed_report
            .results
            .first_mut()
            .expect("fixture must include at least one live-display scenario");
        first.stats.mean_nanos = first.stats.mean_nanos.saturating_mul(10);
        first.stats.p95_nanos = first.stats.p95_nanos.saturating_mul(10);
        regressed_report
            .write_output(&benchmark_report_path)
            .expect("regressed benchmark report should write");

        let report = CalibrationReport {
            system_tool: "terminal-display-calibration".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            benchmark_report: benchmark_report_path.clone(),
            baseline: baseline_path.clone(),
            comparison_mode: "enforced".to_owned(),
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };
        let args = CalibrationValidateCli {
            report: temp_dir.join("calibration.json"),
            benchmark_report: benchmark_report_path,
            baseline: baseline_path,
            comparison_mode: ComparisonModeArg::Enforced,
            required_session_type: Some("wayland".to_owned()),
            required_display_server_hint: Some("wayland".to_owned()),
            runner_readiness_report: None,
        };

        let error =
            validate_calibration_report(&report, &args, true).expect_err("regression must fail");
        assert!(error.to_string().contains("regression"));
    }

    #[test]
    fn expected_system_suite_quality_gates_include_live_display_baseline() {
        let args = SystemSuiteValidateCli {
            report: PathBuf::from("system-suite.json"),
            benchmark_report: PathBuf::from("benchmark.json"),
            governance_mode: GovernanceModeArg::Ci,
            benchmark_baseline: Some(PathBuf::from("benchmark-baseline.json")),
            live_display_mode: Some(LiveDisplayModeArg::Controlled),
            live_display_report: Some(PathBuf::from("live-display.json")),
            live_display_baseline: Some(PathBuf::from("live-display-baseline.json")),
        };

        let gates = expected_system_suite_quality_gates(&args);
        assert_eq!(
            gates.last().expect("expected live-display gate"),
            "terminal-display-benchmark-controlled"
        );
    }

    #[test]
    fn system_suite_validation_accepts_matching_report_contract() {
        let temp_dir = temp_dir("system_suite_validation_accepts_matching_report_contract");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let benchmark_baseline_path = temp_dir.join("benchmark-baseline.json");
        let live_display_report_path = temp_dir.join("live-display.json");
        let live_display_baseline_path = temp_dir.join("live-display-baseline.json");

        let benchmark_report = valid_headless_report();
        benchmark_report
            .write_output(&benchmark_report_path)
            .expect("headless report should write");
        let live_display_report = valid_live_display_report();
        live_display_report
            .write_output(&live_display_report_path)
            .expect("live-display report should write");
        write_threshold_baseline(
            &BenchmarkReport::Headless(benchmark_report.clone()),
            &benchmark_baseline_path,
            "enforced",
        );
        write_threshold_baseline(
            &BenchmarkReport::LiveDisplay(live_display_report.clone()),
            &live_display_baseline_path,
            "advisory",
        );

        let args = SystemSuiteValidateCli {
            report: temp_dir.join("system-suite.json"),
            benchmark_report: benchmark_report_path.clone(),
            governance_mode: GovernanceModeArg::Ci,
            benchmark_baseline: Some(benchmark_baseline_path.clone()),
            live_display_mode: Some(LiveDisplayModeArg::Controlled),
            live_display_report: Some(live_display_report_path.clone()),
            live_display_baseline: Some(live_display_baseline_path.clone()),
        };
        let report = SystemSuiteReport {
            system_tool: "terminal-system-suite".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            governance_mode: "ci".to_owned(),
            benchmark_report: benchmark_report_path,
            benchmark_baseline: Some(benchmark_baseline_path),
            live_display: Some(SystemSuiteLiveDisplayReport {
                mode: "controlled".to_owned(),
                report: live_display_report_path,
                baseline: Some(live_display_baseline_path),
            }),
            quality_gates: expected_system_suite_quality_gates(&args),
        };

        validate_system_suite_report(&report, &args, true)
            .expect("matching system-suite contract should validate");
    }

    #[test]
    fn system_suite_validation_rejects_headless_threshold_regression() {
        let temp_dir = temp_dir("system_suite_validation_rejects_headless_threshold_regression");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let benchmark_baseline_path = temp_dir.join("benchmark-baseline.json");

        let baseline_report = valid_headless_report();
        write_threshold_baseline(
            &BenchmarkReport::Headless(baseline_report.clone()),
            &benchmark_baseline_path,
            "enforced",
        );

        let mut regressed_report = baseline_report.clone();
        let first = regressed_report
            .results
            .first_mut()
            .expect("fixture must include at least one headless scenario");
        first.stats.mean_nanos = first.stats.mean_nanos.saturating_mul(10);
        first.stats.p95_nanos = first.stats.p95_nanos.saturating_mul(10);
        regressed_report
            .write_output(&benchmark_report_path)
            .expect("regressed headless report should write");

        let args = SystemSuiteValidateCli {
            report: temp_dir.join("system-suite.json"),
            benchmark_report: benchmark_report_path.clone(),
            governance_mode: GovernanceModeArg::Ci,
            benchmark_baseline: Some(benchmark_baseline_path.clone()),
            live_display_mode: None,
            live_display_report: None,
            live_display_baseline: None,
        };
        let report = SystemSuiteReport {
            system_tool: "terminal-system-suite".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            governance_mode: "ci".to_owned(),
            benchmark_report: benchmark_report_path,
            benchmark_baseline: Some(benchmark_baseline_path),
            live_display: None,
            quality_gates: expected_system_suite_quality_gates(&args),
        };

        let error =
            validate_system_suite_report(&report, &args, true).expect_err("regression must fail");
        assert!(error.to_string().contains("regression"));
    }

    #[test]
    fn threshold_validation_returns_enforced_mode_from_baseline() {
        let temp_dir = temp_dir("threshold_validation_returns_enforced_mode_from_baseline");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let benchmark_baseline_path = temp_dir.join("benchmark-baseline.json");

        let report = valid_headless_report();
        report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::Headless(report),
            &benchmark_baseline_path,
            THRESHOLD_MODE_ENFORCED,
        );

        let mode =
            validate_threshold_baseline(&benchmark_report_path, &benchmark_baseline_path, true)
                .expect("validation should pass with enforced baseline");
        assert_eq!(mode, ThresholdComparisonMode::Enforced);
    }

    #[test]
    fn threshold_validation_returns_advisory_mode_from_baseline() {
        let temp_dir = temp_dir("threshold_validation_returns_advisory_mode_from_baseline");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let benchmark_baseline_path = temp_dir.join("benchmark-baseline.json");

        let report = valid_headless_report();
        report
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        write_threshold_baseline(
            &BenchmarkReport::Headless(report),
            &benchmark_baseline_path,
            THRESHOLD_MODE_ADVISORY,
        );

        let mode =
            validate_threshold_baseline(&benchmark_report_path, &benchmark_baseline_path, true)
                .expect("validation should pass with advisory baseline");
        assert_eq!(mode, ThresholdComparisonMode::Advisory);
    }

    #[test]
    fn system_suite_validation_rejects_quality_gate_drift() {
        let args = SystemSuiteValidateCli {
            report: PathBuf::from("system-suite.json"),
            benchmark_report: PathBuf::from("benchmark.json"),
            governance_mode: GovernanceModeArg::Ci,
            benchmark_baseline: None,
            live_display_mode: None,
            live_display_report: None,
            live_display_baseline: None,
        };
        let report = SystemSuiteReport {
            system_tool: "terminal-system-suite".to_owned(),
            status: ReportStatus::Pass,
            generated_at_utc: "unix-seconds-utc:1".to_owned(),
            governance_mode: "ci".to_owned(),
            benchmark_report: PathBuf::from("benchmark.json"),
            benchmark_baseline: None,
            live_display: None,
            quality_gates: vec!["cargo-fmt".to_owned()],
        };

        let error = validate_system_suite_report(&report, &args, false)
            .expect_err("quality gate drift must fail");
        assert!(error.to_string().contains("quality_gates mismatch"));
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for test path")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rldyourterm-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should create");
        path
    }

    fn write_threshold_baseline(report: &BenchmarkReport, path: &Path, comparison_mode: &str) {
        let mut scenarios = HashMap::new();
        match report {
            BenchmarkReport::Headless(report) => {
                for scenario in &report.results {
                    let mut baseline_metrics = HashMap::new();
                    baseline_metrics.insert(
                        METRIC_MEAN_NANOS.to_owned(),
                        scenario.stats.mean_nanos as f64,
                    );
                    baseline_metrics
                        .insert(METRIC_P95_NANOS.to_owned(), scenario.stats.p95_nanos as f64);
                    baseline_metrics.insert(
                        METRIC_PRIMARY_UNITS_PER_SECOND.to_owned(),
                        scenario.primary_units_per_second,
                    );
                    baseline_metrics.insert(
                        METRIC_BYTES_PER_SECOND.to_owned(),
                        scenario.bytes_per_second,
                    );
                    scenarios.insert(
                        scenario.scenario.clone(),
                        ThresholdScenarioPolicy {
                            baseline_metrics,
                            thresholds: HashMap::new(),
                        },
                    );
                }
            }
            BenchmarkReport::LiveDisplay(report) => {
                for scenario in &report.results {
                    let mut baseline_metrics = HashMap::new();
                    baseline_metrics.insert(
                        METRIC_MEAN_NANOS.to_owned(),
                        scenario.stats.mean_nanos as f64,
                    );
                    baseline_metrics
                        .insert(METRIC_P95_NANOS.to_owned(), scenario.stats.p95_nanos as f64);
                    baseline_metrics.insert(
                        METRIC_PRIMARY_UNITS_PER_SECOND.to_owned(),
                        scenario.primary_units_per_second,
                    );
                    scenarios.insert(
                        scenario.scenario.clone(),
                        ThresholdScenarioPolicy {
                            baseline_metrics,
                            thresholds: HashMap::new(),
                        },
                    );
                }
            }
        }

        let mut defaults = HashMap::new();
        defaults.insert(THRESHOLD_MAX_MEAN_RATIO.to_owned(), 1.0);
        defaults.insert(THRESHOLD_MAX_P95_RATIO.to_owned(), 1.0);
        defaults.insert(THRESHOLD_MIN_PRIMARY_RATIO.to_owned(), 1.0);
        if scenarios.values().any(|policy| {
            policy
                .baseline_metrics
                .contains_key(METRIC_BYTES_PER_SECOND)
        }) {
            defaults.insert(THRESHOLD_MIN_BYTES_RATIO.to_owned(), 1.0);
        }

        let (benchmark_tool, suite, scale) = match report {
            BenchmarkReport::Headless(report) => (
                report.benchmark_tool.clone(),
                report.suite.clone(),
                report.scale.clone(),
            ),
            BenchmarkReport::LiveDisplay(report) => (
                report.benchmark_tool.clone(),
                report.suite.clone(),
                report.scale.clone(),
            ),
        };

        let environment_scope = environment::infer_report_environment_scope(report)
            .expect("environment scope should infer");
        let environment_requirements = environment::extract_environment_requirements(report)
            .expect("environment requirements should infer");

        let baseline = ThresholdBaseline {
            baseline_tool: THRESHOLD_BASELINE_TOOL.to_owned(),
            benchmark_tool,
            suite,
            scale,
            comparison_mode: comparison_mode.to_owned(),
            environment_scope,
            environment_requirements,
            defaults,
            scenarios,
        };
        write_json(path, &baseline).expect("threshold baseline should write");
    }
}
