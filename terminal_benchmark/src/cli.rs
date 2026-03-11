// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use clap::{Parser, ValueEnum};
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

#[derive(Debug, Parser)]
#[command(
    name = "terminal-benchmark",
    version,
    about = "Self-authored benchmark suite for canonical rldyourterm terminal paths"
)]
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
