// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::{
    CalibrationCli, CalibrationCommands, CalibrationEmitCli, CalibrationValidateCli, GovernanceCli,
    GovernanceCommands, LiveDisplayModeArg, RunnerReadinessCheckCli, RunnerReadinessCli,
    RunnerReadinessCommands, RunnerReadinessValidateCli, SuiteArg, SystemSuiteCli,
    SystemSuiteCommands, SystemSuiteEmitCli, SystemSuiteValidateCli, ValidateCli,
};
use crate::validate;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CALIBRATION_TOOL: &str = "terminal-display-calibration";
const RUNNER_READINESS_TOOL: &str = "terminal-display-runner-readiness";
const SYSTEM_SUITE_TOOL: &str = "terminal-system-suite";

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

pub fn run(args: &GovernanceCli) -> Result<()> {
    match &args.command {
        GovernanceCommands::RunnerReadiness(cli) => run_runner_readiness(cli),
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

    if let Some(path) = args.runner_readiness_report.as_ref() {
        if require_inputs && !path.is_file() {
            bail!("runner readiness report does not exist: {}", path.display());
        }
        let readiness: RunnerReadinessReport = read_json(path).with_context(|| {
            format!("failed to read runner readiness report {}", path.display())
        })?;
        validate_runner_readiness_report(&readiness, true)?;
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

    if let Some(path) = args.live_display_report.as_ref() {
        validate::run(&ValidateCli {
            suite: SuiteArg::LiveDisplay,
            report: path.clone(),
            require_scenario: Vec::new(),
            require_full_suite: true,
        })?;
    }

    Ok(())
}

fn expected_system_suite_quality_gates(args: &SystemSuiteValidateCli) -> Vec<String> {
    let mut gates = vec![
        "cargo fmt --all -- --check".to_owned(),
        "cargo check --workspace --all-targets --locked".to_owned(),
        "cargo test --workspace --locked".to_owned(),
        "cargo clippy --workspace --all-targets --all-features --locked -- -D warnings".to_owned(),
        "cargo +1.92.0 check --workspace --all-targets --locked".to_owned(),
        "cargo check --manifest-path fuzz/Cargo.toml --locked".to_owned(),
        "bash scripts/ci/run_terminal_benchmark_smoke.sh".to_owned(),
    ];

    if let Some(path) = args.benchmark_baseline.as_ref() {
        gates.push(format!(
            "TERMINAL_BENCHMARK_BASELINE={} bash scripts/ci/run_terminal_benchmark_full.sh {}",
            path.display(),
            args.benchmark_report.display()
        ));
    } else {
        gates.push(format!(
            "bash scripts/ci/run_terminal_benchmark_full.sh {}",
            args.benchmark_report.display()
        ));
    }

    gates.push(format!(
        "bash scripts/ci/run_e2e_governance.sh --mode {}",
        args.governance_mode.as_str()
    ));

    if let Some(mode) = args.live_display_mode {
        let report = args
            .live_display_report
            .as_ref()
            .expect("live_display_report is required when live_display_mode is set");
        if let Some(path) = args.live_display_baseline.as_ref() {
            gates.push(format!(
                "TERMINAL_DISPLAY_BENCHMARK_BASELINE={} bash scripts/ci/run_terminal_display_benchmark_{}.sh {}",
                path.display(),
                mode.as_str(),
                report.display()
            ));
        } else {
            gates.push(format!(
                "bash scripts/ci/run_terminal_display_benchmark_{}.sh {}",
                mode.as_str(),
                report.display()
            ));
        }
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
        CalibrationReport, ObservedDisplayEnvironment, ReportStatus, RunnerReadinessReport,
        SystemSuiteLiveDisplayReport, SystemSuiteReport, build_runner_readiness_report,
        expected_system_suite_quality_gates, validate_calibration_report,
        validate_runner_readiness_report, validate_system_suite_report, write_json,
    };
    use crate::cli::{
        CalibrationValidateCli, ComparisonModeArg, GovernanceModeArg, LiveDisplayModeArg,
        SystemSuiteValidateCli,
    };
    use crate::validate::tests::{valid_headless_report, valid_live_display_report};
    use std::fs;
    use std::path::PathBuf;
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
    fn calibration_validation_accepts_matching_reports() {
        let temp_dir = temp_dir("calibration_validation_accepts_matching_reports");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let baseline_path = temp_dir.join("baseline.json");
        let readiness_path = temp_dir.join("readiness.json");

        valid_live_display_report()
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        fs::write(&baseline_path, "{}\n").expect("baseline should write");

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

        valid_live_display_report()
            .write_output(&benchmark_report_path)
            .expect("benchmark report should write");
        fs::write(&baseline_path, "{}\n").expect("baseline should write");

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
            "TERMINAL_DISPLAY_BENCHMARK_BASELINE=live-display-baseline.json bash scripts/ci/run_terminal_display_benchmark_controlled.sh live-display.json"
        );
    }

    #[test]
    fn system_suite_validation_accepts_matching_report_contract() {
        let temp_dir = temp_dir("system_suite_validation_accepts_matching_report_contract");
        let benchmark_report_path = temp_dir.join("benchmark.json");
        let benchmark_baseline_path = temp_dir.join("benchmark-baseline.json");
        let live_display_report_path = temp_dir.join("live-display.json");
        let live_display_baseline_path = temp_dir.join("live-display-baseline.json");

        valid_headless_report()
            .write_output(&benchmark_report_path)
            .expect("headless report should write");
        valid_live_display_report()
            .write_output(&live_display_report_path)
            .expect("live-display report should write");
        fs::write(&benchmark_baseline_path, "{}\n").expect("benchmark baseline should write");
        fs::write(&live_display_baseline_path, "{}\n").expect("live-display baseline should write");

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
            quality_gates: vec!["cargo fmt --all -- --check".to_owned()],
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
}
