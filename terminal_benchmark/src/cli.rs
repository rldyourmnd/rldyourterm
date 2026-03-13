// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SuiteArg {
    CanonicalHeadless,
    LiveDisplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ScenarioArg {
    All,
    CoreIngestBurst,
    CoreScrollbackFlood,
    CoreParserThroughput,
    CoreGridScroll,
    ServiceSessionRuntimeCycle,
    UiCommandCycle,
    SettingsApplyCycle,
    ShellResolutionPlan,
    FontCacheMixedRaster,
    GpuSurfacePolicy,
    CpuRenderFull,
    CpuRenderDelta,
    CpuCycleIngestRenderDelta,
    CpuPixelRasterDelta,
    StartupFirstFrameGpu,
    StartupFirstFrameCpu,
    SteadyRedrawGpu,
    SteadyRedrawCpu,
    ResizeCycleGpu,
    ResizeCycleCpu,
}

impl ScenarioArg {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::CoreIngestBurst => "core-ingest-burst",
            Self::CoreScrollbackFlood => "core-scrollback-flood",
            Self::CoreParserThroughput => "core-parser-throughput",
            Self::CoreGridScroll => "core-grid-scroll",
            Self::ServiceSessionRuntimeCycle => "service-session-runtime-cycle",
            Self::UiCommandCycle => "ui-command-cycle",
            Self::SettingsApplyCycle => "settings-apply-cycle",
            Self::ShellResolutionPlan => "shell-resolution-plan",
            Self::FontCacheMixedRaster => "font-cache-mixed-raster",
            Self::GpuSurfacePolicy => "gpu-surface-policy",
            Self::CpuRenderFull => "cpu-render-full",
            Self::CpuRenderDelta => "cpu-render-delta",
            Self::CpuCycleIngestRenderDelta => "cpu-cycle-ingest-render-delta",
            Self::CpuPixelRasterDelta => "cpu-pixel-raster-delta",
            Self::StartupFirstFrameGpu => "startup-first-frame-gpu",
            Self::StartupFirstFrameCpu => "startup-first-frame-cpu",
            Self::SteadyRedrawGpu => "steady-redraw-gpu",
            Self::SteadyRedrawCpu => "steady-redraw-cpu",
            Self::ResizeCycleGpu => "resize-cycle-gpu",
            Self::ResizeCycleCpu => "resize-cycle-cpu",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ScaleArg {
    Quick,
    Standard,
    Stress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum OutputFormatArg {
    Table,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ComparisonModeArg {
    Advisory,
    Enforced,
}

impl ComparisonModeArg {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Enforced => "enforced",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum GovernanceModeArg {
    Ci,
    Release,
}

impl GovernanceModeArg {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ci => "ci",
            Self::Release => "release",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum LiveDisplayModeArg {
    Smoke,
    Full,
    Controlled,
}

impl LiveDisplayModeArg {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Full => "full",
            Self::Controlled => "controlled",
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct Cli {
    #[arg(long, value_enum, default_value = "canonical-headless")]
    pub suite: SuiteArg,
    #[arg(long, value_enum, default_value = "all")]
    pub scenario: ScenarioArg,
    #[arg(long, value_enum, default_value = "standard")]
    pub scale: ScaleArg,
    #[arg(long, default_value_t = 2)]
    pub warmup_iterations: u32,
    #[arg(long, default_value_t = 10)]
    pub iterations: u32,
    #[arg(long, default_value_t = 160)]
    pub cols: u16,
    #[arg(long, default_value_t = 48)]
    pub rows: u16,
    #[arg(long, default_value_t = 8 * 1024)]
    pub chunk_bytes: usize,
    #[arg(long, default_value_t = 50_000)]
    pub scrollback_cap: usize,
    #[arg(long, value_enum, default_value = "table")]
    pub format: OutputFormatArg,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ValidateCli {
    #[arg(long, value_enum)]
    pub suite: SuiteArg,
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long = "require-scenario")]
    pub require_scenario: Vec<String>,
    #[arg(long)]
    pub require_full_suite: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EnvironmentCli {
    #[command(subcommand)]
    pub command: EnvironmentCommands,
}

#[derive(Debug, Clone, Args)]
pub struct GovernanceCli {
    #[command(subcommand)]
    pub command: GovernanceCommands,
}

#[derive(Debug, Clone, Args)]
pub struct EnvironmentSnapshotCli {
    #[arg(long)]
    pub report: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct EnvironmentValidateCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub require_session_type: Option<String>,
    #[arg(long)]
    pub require_display_server_hint: Option<String>,
    #[arg(long)]
    pub require_monitor_cadence: bool,
    #[arg(long)]
    pub require_monitor_scale_factor: bool,
}

#[derive(Debug, Clone, Args)]
pub struct EnvironmentValidateBaselineCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub baseline: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct RunnerReadinessCli {
    #[command(subcommand)]
    pub command: RunnerReadinessCommands,
}

#[derive(Debug, Clone, Args)]
pub struct RunnerReadinessCheckCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub require_pass: bool,
    #[arg(long)]
    pub require_session_type: Option<String>,
    #[arg(long)]
    pub require_display_server_hint: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RunnerReadinessValidateCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub require_pass: bool,
}

#[derive(Debug, Clone, Args)]
pub struct CalibrationCli {
    #[command(subcommand)]
    pub command: CalibrationCommands,
}

#[derive(Debug, Clone, Args)]
pub struct SystemSuiteCli {
    #[command(subcommand)]
    pub command: SystemSuiteCommands,
}

#[derive(Debug, Clone, Args)]
pub struct CalibrationEmitCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub benchmark_report: PathBuf,
    #[arg(long)]
    pub baseline: PathBuf,
    #[arg(long, value_enum)]
    pub comparison_mode: ComparisonModeArg,
    #[arg(long)]
    pub required_session_type: Option<String>,
    #[arg(long)]
    pub required_display_server_hint: Option<String>,
    #[arg(long)]
    pub runner_readiness_report: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct CalibrationValidateCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub benchmark_report: PathBuf,
    #[arg(long)]
    pub baseline: PathBuf,
    #[arg(long, value_enum)]
    pub comparison_mode: ComparisonModeArg,
    #[arg(long)]
    pub required_session_type: Option<String>,
    #[arg(long)]
    pub required_display_server_hint: Option<String>,
    #[arg(long)]
    pub runner_readiness_report: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct SystemSuiteEmitCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub benchmark_report: PathBuf,
    #[arg(long, value_enum)]
    pub governance_mode: GovernanceModeArg,
    #[arg(long)]
    pub benchmark_baseline: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub live_display_mode: Option<LiveDisplayModeArg>,
    #[arg(long)]
    pub live_display_report: Option<PathBuf>,
    #[arg(long)]
    pub live_display_baseline: Option<PathBuf>,
    #[arg(long = "quality-gate")]
    pub quality_gate: Vec<String>,
}

#[derive(Debug, Clone, Args)]
pub struct SystemSuiteValidateCli {
    #[arg(long)]
    pub report: PathBuf,
    #[arg(long)]
    pub benchmark_report: PathBuf,
    #[arg(long, value_enum)]
    pub governance_mode: GovernanceModeArg,
    #[arg(long)]
    pub benchmark_baseline: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub live_display_mode: Option<LiveDisplayModeArg>,
    #[arg(long)]
    pub live_display_report: Option<PathBuf>,
    #[arg(long)]
    pub live_display_baseline: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvironmentCommands {
    Snapshot(EnvironmentSnapshotCli),
    Validate(EnvironmentValidateCli),
    ValidateBaseline(EnvironmentValidateBaselineCli),
}

#[derive(Debug, Clone, Subcommand)]
pub enum RunnerReadinessCommands {
    Check(RunnerReadinessCheckCli),
    Validate(RunnerReadinessValidateCli),
}

#[derive(Debug, Clone, Subcommand)]
pub enum CalibrationCommands {
    Emit(CalibrationEmitCli),
    Validate(CalibrationValidateCli),
}

#[derive(Debug, Clone, Subcommand)]
pub enum SystemSuiteCommands {
    Emit(SystemSuiteEmitCli),
    Validate(SystemSuiteValidateCli),
}

#[derive(Debug, Clone, Subcommand)]
pub enum GovernanceCommands {
    RunnerReadiness(RunnerReadinessCli),
    Calibration(CalibrationCli),
    SystemSuite(SystemSuiteCli),
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Validate(ValidateCli),
    Environment(EnvironmentCli),
    Governance(GovernanceCli),
}

#[derive(Debug, Parser)]
#[command(
    name = "terminal-benchmark",
    version,
    about = "Self-authored benchmark suite for canonical rldyourterm terminal paths",
    propagate_version = true
)]
pub struct TopLevelCli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub run: Cli,
}
