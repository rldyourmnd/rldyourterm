// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::ScenarioArg;
use crate::report::{SUITE_MANIFEST_SCHEMA_VERSION, SuiteManifest, SuiteScenarioManifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioDescriptor {
    pub name: &'static str,
    pub layer: &'static str,
    pub benchmark_kind: &'static str,
    pub description: &'static str,
    pub primary_unit_label: &'static str,
}

pub const BENCHMARK_SUITE_NAME: &str = "canonical-headless";

pub fn scenario_belongs_to_suite(scenario: ScenarioArg) -> bool {
    matches!(
        scenario,
        ScenarioArg::All
            | ScenarioArg::CoreIngestBurst
            | ScenarioArg::CoreScrollbackFlood
            | ScenarioArg::CoreParserThroughput
            | ScenarioArg::CoreGridScroll
            | ScenarioArg::ServiceSessionRuntimeCycle
            | ScenarioArg::UiCommandCycle
            | ScenarioArg::SettingsApplyCycle
            | ScenarioArg::ShellResolutionPlan
            | ScenarioArg::FontCacheMixedRaster
            | ScenarioArg::GpuSurfacePolicy
            | ScenarioArg::CpuRenderFull
            | ScenarioArg::CpuRenderDelta
            | ScenarioArg::CpuCycleIngestRenderDelta
            | ScenarioArg::CpuPixelRasterDelta
    )
}

pub fn selected_scenarios(selection: ScenarioArg) -> Vec<ScenarioArg> {
    match selection {
        ScenarioArg::All => vec![
            ScenarioArg::CoreIngestBurst,
            ScenarioArg::CoreScrollbackFlood,
            ScenarioArg::CoreParserThroughput,
            ScenarioArg::CoreGridScroll,
            ScenarioArg::ServiceSessionRuntimeCycle,
            ScenarioArg::UiCommandCycle,
            ScenarioArg::SettingsApplyCycle,
            ScenarioArg::ShellResolutionPlan,
            ScenarioArg::FontCacheMixedRaster,
            ScenarioArg::GpuSurfacePolicy,
            ScenarioArg::CpuRenderFull,
            ScenarioArg::CpuRenderDelta,
            ScenarioArg::CpuCycleIngestRenderDelta,
            ScenarioArg::CpuPixelRasterDelta,
        ],
        one => vec![one],
    }
}

pub const fn descriptor(scenario: ScenarioArg) -> ScenarioDescriptor {
    match scenario {
        ScenarioArg::All => ScenarioDescriptor {
            name: "all",
            layer: "suite",
            benchmark_kind: "aggregate",
            description: "all scenarios",
            primary_unit_label: "n/a",
        },
        ScenarioArg::CoreIngestBurst => ScenarioDescriptor {
            name: "core-ingest-burst",
            layer: "core",
            benchmark_kind: "throughput",
            description: "Chunked ANSI-heavy AI output ingest through TerminalState",
            primary_unit_label: "bytes",
        },
        ScenarioArg::CoreScrollbackFlood => ScenarioDescriptor {
            name: "core-scrollback-flood",
            layer: "core",
            benchmark_kind: "throughput",
            description: "Deep scrollback ingest and trimming pressure through TerminalState",
            primary_unit_label: "bytes",
        },
        ScenarioArg::CoreParserThroughput => ScenarioDescriptor {
            name: "core-parser-throughput",
            layer: "core",
            benchmark_kind: "throughput",
            description: "Isolated ANSI parser throughput without grid dispatch",
            primary_unit_label: "bytes",
        },
        ScenarioArg::CoreGridScroll => ScenarioDescriptor {
            name: "core-grid-scroll",
            layer: "core",
            benchmark_kind: "throughput",
            description: "Grid scroll_up_discard throughput with dirty-row tracking",
            primary_unit_label: "scrolls",
        },
        ScenarioArg::ServiceSessionRuntimeCycle => ScenarioDescriptor {
            name: "service-session-runtime-cycle",
            layer: "services/session",
            benchmark_kind: "control-plane",
            description: "SessionController recoverable lifecycle cycle over canonical PTY boundaries",
            primary_unit_label: "transitions",
        },
        ScenarioArg::UiCommandCycle => ScenarioDescriptor {
            name: "ui-command-cycle",
            layer: "ui",
            benchmark_kind: "control-plane",
            description: "UiRuntime command handling over canonical runtime command batches",
            primary_unit_label: "commands",
        },
        ScenarioArg::SettingsApplyCycle => ScenarioDescriptor {
            name: "settings-apply-cycle",
            layer: "features/settings",
            benchmark_kind: "control-plane",
            description: "Settings palette parse plus apply cycle over canonical command inputs",
            primary_unit_label: "commands",
        },
        ScenarioArg::ShellResolutionPlan => ScenarioDescriptor {
            name: "shell-resolution-plan",
            layer: "features/shell-integration",
            benchmark_kind: "control-plane",
            description: "Shell resolution and launch-plan derivation over deterministic availability cases",
            primary_unit_label: "cases",
        },
        ScenarioArg::FontCacheMixedRaster => ScenarioDescriptor {
            name: "font-cache-mixed-raster",
            layer: "features/font",
            benchmark_kind: "raster-prep",
            description: "GlyphCache lookup and raster path over mixed ASCII, Cyrillic, and box-drawing text",
            primary_unit_label: "glyphs",
        },
        ScenarioArg::GpuSurfacePolicy => ScenarioDescriptor {
            name: "gpu-surface-policy",
            layer: "features/render-gpu",
            benchmark_kind: "policy",
            description: "Surface recovery and configuration helpers over deterministic acquire and resize failures",
            primary_unit_label: "decisions",
        },
        ScenarioArg::CpuRenderFull => ScenarioDescriptor {
            name: "cpu-render-full",
            layer: "features/render-cpu",
            benchmark_kind: "raster",
            description: "Canonical full-frame CPU render snapshot",
            primary_unit_label: "cells",
        },
        ScenarioArg::CpuRenderDelta => ScenarioDescriptor {
            name: "cpu-render-delta",
            layer: "features/render-cpu",
            benchmark_kind: "raster",
            description: "Canonical dirty-row CPU delta render",
            primary_unit_label: "cells",
        },
        ScenarioArg::CpuCycleIngestRenderDelta => ScenarioDescriptor {
            name: "cpu-cycle-ingest-render-delta",
            layer: "features/render-cpu",
            benchmark_kind: "raster",
            description: "Steady-state ingest plus CPU delta render cycle",
            primary_unit_label: "cells",
        },
        ScenarioArg::CpuPixelRasterDelta => ScenarioDescriptor {
            name: "cpu-pixel-raster-delta",
            layer: "features/render-cpu",
            benchmark_kind: "raster",
            description: "Headless CPU pixel raster path over a dirty terminal buffer",
            primary_unit_label: "pixels",
        },
        _ => panic!("invalid canonical-headless scenario"),
    }
}

pub fn selected_scenario_names(selection: ScenarioArg) -> Vec<&'static str> {
    selected_scenarios(selection)
        .into_iter()
        .map(|scenario| descriptor(scenario).name)
        .collect()
}

pub fn suite_manifest() -> SuiteManifest {
    let scenarios = selected_scenarios(ScenarioArg::All)
        .into_iter()
        .map(|scenario| {
            let descriptor = descriptor(scenario);
            SuiteScenarioManifest {
                scenario: descriptor.name,
                layer: descriptor.layer,
                benchmark_kind: descriptor.benchmark_kind,
                description: descriptor.description,
                primary_unit_label: descriptor.primary_unit_label,
                backend: None,
                controlled_monitor_cadence: false,
            }
        })
        .collect();

    SuiteManifest {
        schema_version: SUITE_MANIFEST_SCHEMA_VERSION,
        scenarios,
    }
}

#[cfg(test)]
mod tests {
    use super::{BENCHMARK_SUITE_NAME, descriptor, scenario_belongs_to_suite, selected_scenarios};
    use crate::cli::ScenarioArg;

    #[test]
    fn all_selection_expands_to_full_canonical_suite() {
        let scenarios = selected_scenarios(ScenarioArg::All);
        assert_eq!(scenarios.len(), 14);
        assert!(scenarios.contains(&ScenarioArg::ServiceSessionRuntimeCycle));
        assert!(scenarios.contains(&ScenarioArg::UiCommandCycle));
        assert!(scenarios.contains(&ScenarioArg::SettingsApplyCycle));
        assert!(scenarios.contains(&ScenarioArg::ShellResolutionPlan));
        assert!(scenarios.contains(&ScenarioArg::FontCacheMixedRaster));
        assert!(scenarios.contains(&ScenarioArg::GpuSurfacePolicy));
        assert_eq!(BENCHMARK_SUITE_NAME, "canonical-headless");
    }

    #[test]
    fn descriptors_cover_layer_and_kind_metadata() {
        let descriptor = descriptor(ScenarioArg::GpuSurfacePolicy);
        assert_eq!(descriptor.layer, "features/render-gpu");
        assert_eq!(descriptor.benchmark_kind, "policy");
        assert_eq!(descriptor.primary_unit_label, "decisions");
    }

    #[test]
    fn suite_membership_rejects_live_display_scenarios() {
        assert!(scenario_belongs_to_suite(ScenarioArg::CoreIngestBurst));
        assert!(!scenario_belongs_to_suite(
            ScenarioArg::StartupFirstFrameGpu
        ));
    }
}
