// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use crate::cli::ScenarioArg;
use crate::report::{SUITE_MANIFEST_SCHEMA_VERSION, SuiteManifest, SuiteScenarioManifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioDescriptor {
    pub name: &'static str,
    pub layer: &'static str,
    pub benchmark_kind: &'static str,
    pub backend: &'static str,
    pub description: &'static str,
    pub primary_unit_label: &'static str,
}

pub const BENCHMARK_SUITE_NAME: &str = "live-display";

pub fn selected_scenarios(selection: ScenarioArg) -> Vec<ScenarioArg> {
    match selection {
        ScenarioArg::All => vec![
            ScenarioArg::StartupFirstFrameGpu,
            ScenarioArg::StartupFirstFrameCpu,
            ScenarioArg::SteadyRedrawGpu,
            ScenarioArg::SteadyRedrawCpu,
            ScenarioArg::ResizeCycleGpu,
            ScenarioArg::ResizeCycleCpu,
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
            backend: "mixed",
            description: "all live display scenarios",
            primary_unit_label: "n/a",
        },
        ScenarioArg::StartupFirstFrameGpu => ScenarioDescriptor {
            name: "startup-first-frame-gpu",
            layer: "features/render-gpu",
            benchmark_kind: "display-startup",
            backend: "gpu",
            description: "Window creation, GPU renderer initialization, and first successful present on a live winit surface",
            primary_unit_label: "windows",
        },
        ScenarioArg::StartupFirstFrameCpu => ScenarioDescriptor {
            name: "startup-first-frame-cpu",
            layer: "features/render-cpu",
            benchmark_kind: "display-startup",
            backend: "cpu",
            description: "Window creation, softbuffer setup, CPU rasterization, and first successful present on a live winit surface",
            primary_unit_label: "windows",
        },
        ScenarioArg::SteadyRedrawGpu => ScenarioDescriptor {
            name: "steady-redraw-gpu",
            layer: "features/render-gpu",
            benchmark_kind: "display-frame",
            backend: "gpu",
            description: "Repeated full-frame redraw and present on a live GPU surface",
            primary_unit_label: "frames",
        },
        ScenarioArg::SteadyRedrawCpu => ScenarioDescriptor {
            name: "steady-redraw-cpu",
            layer: "features/render-cpu",
            benchmark_kind: "display-frame",
            backend: "cpu",
            description: "Repeated full-frame CPU rasterization and softbuffer present on a live window surface",
            primary_unit_label: "frames",
        },
        ScenarioArg::ResizeCycleGpu => ScenarioDescriptor {
            name: "resize-cycle-gpu",
            layer: "features/render-gpu",
            benchmark_kind: "display-resize",
            backend: "gpu",
            description: "Real window resize cycles followed by GPU redraw and present",
            primary_unit_label: "resizes",
        },
        ScenarioArg::ResizeCycleCpu => ScenarioDescriptor {
            name: "resize-cycle-cpu",
            layer: "features/render-cpu",
            benchmark_kind: "display-resize",
            backend: "cpu",
            description: "Real window resize cycles followed by CPU redraw and softbuffer present",
            primary_unit_label: "resizes",
        },
        _ => panic!("invalid live-display scenario"),
    }
}

pub fn selected_scenario_names(selection: ScenarioArg) -> Vec<String> {
    selected_scenarios(selection)
        .into_iter()
        .map(|scenario| descriptor(scenario).name.to_owned())
        .collect()
}

pub fn suite_manifest() -> SuiteManifest {
    let scenarios = selected_scenarios(ScenarioArg::All)
        .into_iter()
        .map(|scenario| {
            let descriptor = descriptor(scenario);
            SuiteScenarioManifest {
                scenario: descriptor.name.to_owned(),
                layer: descriptor.layer.to_owned(),
                benchmark_kind: descriptor.benchmark_kind.to_owned(),
                description: descriptor.description.to_owned(),
                primary_unit_label: descriptor.primary_unit_label.to_owned(),
                backend: Some(descriptor.backend.to_owned()),
                controlled_monitor_cadence: matches!(
                    scenario,
                    ScenarioArg::SteadyRedrawCpu | ScenarioArg::ResizeCycleCpu
                ),
            }
        })
        .collect();

    SuiteManifest {
        schema_version: SUITE_MANIFEST_SCHEMA_VERSION,
        scenarios,
        coverage: None,
    }
}

pub fn scenario_belongs_to_suite(scenario: ScenarioArg) -> bool {
    matches!(
        scenario,
        ScenarioArg::All
            | ScenarioArg::StartupFirstFrameGpu
            | ScenarioArg::StartupFirstFrameCpu
            | ScenarioArg::SteadyRedrawGpu
            | ScenarioArg::SteadyRedrawCpu
            | ScenarioArg::ResizeCycleGpu
            | ScenarioArg::ResizeCycleCpu
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BENCHMARK_SUITE_NAME, descriptor, scenario_belongs_to_suite, selected_scenarios,
        suite_manifest,
    };
    use crate::cli::ScenarioArg;

    #[test]
    fn all_selection_expands_to_live_display_suite() {
        let scenarios = selected_scenarios(ScenarioArg::All);
        assert_eq!(scenarios.len(), 6);
        assert!(scenarios.contains(&ScenarioArg::StartupFirstFrameGpu));
        assert!(scenarios.contains(&ScenarioArg::ResizeCycleCpu));
        assert_eq!(BENCHMARK_SUITE_NAME, "live-display");
    }

    #[test]
    fn descriptors_capture_backend_and_kind() {
        let descriptor = descriptor(ScenarioArg::SteadyRedrawGpu);
        assert_eq!(descriptor.backend, "gpu");
        assert_eq!(descriptor.benchmark_kind, "display-frame");
    }

    #[test]
    fn suite_membership_rejects_headless_scenarios() {
        assert!(scenario_belongs_to_suite(ScenarioArg::StartupFirstFrameGpu));
        assert!(!scenario_belongs_to_suite(ScenarioArg::CoreIngestBurst));
    }

    #[test]
    fn live_display_manifest_omits_headless_coverage_contract() {
        let manifest = suite_manifest();

        assert_eq!(manifest.schema_version, 2);
        assert!(manifest.coverage.is_none());
    }
}
