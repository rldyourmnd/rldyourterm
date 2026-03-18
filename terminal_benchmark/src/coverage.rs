// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub benchmarked_layers: Vec<CoverageLayer>,
    pub verified_only_layers: Vec<CoverageLayer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageLayer {
    pub layer: String,
    pub benchmark_scenarios: Vec<String>,
    pub validation_commands: Vec<String>,
    pub notes: String,
}

pub fn benchmark_coverage_summary() -> CoverageSummary {
    CoverageSummary {
        benchmarked_layers: vec![
            CoverageLayer {
                layer: "core".to_owned(),
                benchmark_scenarios: vec![
                    "core-ingest-burst".to_owned(),
                    "core-scrollback-flood".to_owned(),
                    "core-parser-throughput".to_owned(),
                    "core-grid-scroll".to_owned(),
                    "core-grid-reflow".to_owned(),
                    "core-unicode-ingest".to_owned(),
                ],
                validation_commands: vec![
                    "cargo test -p rldyourterm-core --locked".to_owned(),
                    "cargo test -p rldyourterm-integration-tests --locked".to_owned(),
                ],
                notes: "Domain ingest, parser, grid, and scrollback hot paths are benchmarked headlessly and verified by integration tests.".to_owned(),
            },
            CoverageLayer {
                layer: "services/session".to_owned(),
                benchmark_scenarios: vec!["service-session-runtime-cycle".to_owned()],
                validation_commands: vec!["cargo test -p rldyourterm-services --locked".to_owned()],
                notes: "Lifecycle and recoverable boundary orchestration are benchmarked via SessionController and validated by service tests.".to_owned(),
            },
            CoverageLayer {
                layer: "ui".to_owned(),
                benchmark_scenarios: vec!["ui-command-cycle".to_owned()],
                validation_commands: vec!["cargo test -p rldyourterm-ui --locked".to_owned()],
                notes: "UiRuntime command-path control flow is benchmarked headlessly and validated by UI unit tests.".to_owned(),
            },
            CoverageLayer {
                layer: "features/settings".to_owned(),
                benchmark_scenarios: vec![
                    "settings-apply-cycle".to_owned(),
                    "settings-parse-through".to_owned(),
                ],
                validation_commands: vec!["cargo test -p rldyourterm-settings --locked".to_owned()],
                notes: "Palette command parsing plus application is benchmarked through the public SettingsService API.".to_owned(),
            },
            CoverageLayer {
                layer: "features/shell-integration".to_owned(),
                benchmark_scenarios: vec!["shell-resolution-plan".to_owned()],
                validation_commands: vec!["cargo test -p rldyourterm-shell-integration --locked".to_owned()],
                notes: "Shell resolution and launch-plan normalization are benchmarked over deterministic availability cases.".to_owned(),
            },
            CoverageLayer {
                layer: "features/font".to_owned(),
                benchmark_scenarios: vec!["font-cache-mixed-raster".to_owned()],
                validation_commands: vec!["cargo test -p rldyourterm-font --locked".to_owned()],
                notes: "Glyph lookup and raster preparation are benchmarked through GlyphCache, including mixed-width glyph corpora.".to_owned(),
            },
            CoverageLayer {
                layer: "features/render-gpu".to_owned(),
                benchmark_scenarios: vec!["gpu-surface-policy".to_owned()],
                validation_commands: vec!["cargo test -p rldyourterm-render-gpu --locked".to_owned()],
                notes: "Headless GPU policy coverage focuses on deterministic surface recovery and configuration helpers, not live surface presentation.".to_owned(),
            },
            CoverageLayer {
                layer: "features/render-cpu".to_owned(),
                benchmark_scenarios: vec![
                    "cpu-render-full".to_owned(),
                    "cpu-render-delta".to_owned(),
                    "cpu-cycle-ingest-render-delta".to_owned(),
                    "cpu-pixel-raster-delta".to_owned(),
                    "cpu-render-scrollback".to_owned(),
                ],
                validation_commands: vec!["cargo test -p rldyourterm-render-cpu --locked".to_owned()],
                notes: "Canonical CPU frame and raster paths are benchmarked directly and validated by renderer tests.".to_owned(),
            },
        ],
        verified_only_layers: vec![
            CoverageLayer {
                layer: "app".to_owned(),
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-app --locked".to_owned()],
                notes: "App bootstrap and runtime orchestration are correctness-critical but not benchmarked headlessly because they depend on runtime integration behavior.".to_owned(),
            },
            CoverageLayer {
                layer: "foundation".to_owned(),
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-foundation --locked".to_owned()],
                notes: "Foundation ports define contracts and are validated through unit tests and downstream adapter tests rather than throughput benchmarks.".to_owned(),
            },
            CoverageLayer {
                layer: "foundation-platform".to_owned(),
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-foundation-platform --locked".to_owned()],
                notes: "Platform adapters are environment-facing and remain in correctness-only validation lanes to avoid noisy benchmark output.".to_owned(),
            },
            CoverageLayer {
                layer: "observability/diagnostics".to_owned(),
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-diagnostics --locked".to_owned()],
                notes: "Diagnostics emission is correctness- and schema-focused; tests validate typing and correlation behavior instead of throughput benchmarks.".to_owned(),
            },
        ],
    }
}
