// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    pub benchmarked_layers: Vec<CoverageLayer>,
    pub verified_only_layers: Vec<CoverageLayer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageLayer {
    pub layer: &'static str,
    pub benchmark_scenarios: Vec<&'static str>,
    pub validation_commands: Vec<&'static str>,
    pub notes: &'static str,
}

pub fn benchmark_coverage_summary() -> CoverageSummary {
    CoverageSummary {
        benchmarked_layers: vec![
            CoverageLayer {
                layer: "core",
                benchmark_scenarios: vec![
                    "core-ingest-burst",
                    "core-scrollback-flood",
                    "core-parser-throughput",
                    "core-grid-scroll",
                ],
                validation_commands: vec![
                    "cargo test -p rldyourterm-core --locked",
                    "cargo test -p rldyourterm-integration-tests --locked",
                ],
                notes: "Domain ingest, parser, grid, and scrollback hot paths are benchmarked headlessly and verified by integration tests.",
            },
            CoverageLayer {
                layer: "services/session",
                benchmark_scenarios: vec!["service-session-runtime-cycle"],
                validation_commands: vec!["cargo test -p rldyourterm-services --locked"],
                notes: "Lifecycle and recoverable boundary orchestration are benchmarked via SessionController and validated by service tests.",
            },
            CoverageLayer {
                layer: "ui",
                benchmark_scenarios: vec!["ui-command-cycle"],
                validation_commands: vec!["cargo test -p rldyourterm-ui --locked"],
                notes: "UiRuntime command-path control flow is benchmarked headlessly and validated by UI unit tests.",
            },
            CoverageLayer {
                layer: "features/settings",
                benchmark_scenarios: vec!["settings-apply-cycle"],
                validation_commands: vec!["cargo test -p rldyourterm-settings --locked"],
                notes: "Palette command parsing plus application is benchmarked through the public SettingsService API.",
            },
            CoverageLayer {
                layer: "features/shell-integration",
                benchmark_scenarios: vec!["shell-resolution-plan"],
                validation_commands: vec!["cargo test -p rldyourterm-shell-integration --locked"],
                notes: "Shell resolution and launch-plan normalization are benchmarked over deterministic availability cases.",
            },
            CoverageLayer {
                layer: "features/font",
                benchmark_scenarios: vec!["font-cache-mixed-raster"],
                validation_commands: vec!["cargo test -p rldyourterm-font --locked"],
                notes: "Glyph lookup and raster preparation are benchmarked through GlyphCache, including mixed-width glyph corpora.",
            },
            CoverageLayer {
                layer: "features/render-gpu",
                benchmark_scenarios: vec!["gpu-surface-policy"],
                validation_commands: vec!["cargo test -p rldyourterm-render-gpu --locked"],
                notes: "Headless GPU policy coverage focuses on deterministic surface recovery and configuration helpers, not live surface presentation.",
            },
            CoverageLayer {
                layer: "features/render-cpu",
                benchmark_scenarios: vec![
                    "cpu-render-full",
                    "cpu-render-delta",
                    "cpu-cycle-ingest-render-delta",
                    "cpu-pixel-raster-delta",
                ],
                validation_commands: vec!["cargo test -p rldyourterm-render-cpu --locked"],
                notes: "Canonical CPU frame and raster paths are benchmarked directly and validated by renderer tests.",
            },
        ],
        verified_only_layers: vec![
            CoverageLayer {
                layer: "app",
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-app --locked"],
                notes: "App bootstrap and runtime orchestration are correctness-critical but not benchmarked headlessly because they depend on runtime integration behavior.",
            },
            CoverageLayer {
                layer: "foundation",
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-foundation --locked"],
                notes: "Foundation ports define contracts and are validated through unit tests and downstream adapter tests rather than throughput benchmarks.",
            },
            CoverageLayer {
                layer: "foundation-platform",
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-foundation-platform --locked"],
                notes: "Platform adapters are environment-facing and remain in correctness-only validation lanes to avoid noisy benchmark output.",
            },
            CoverageLayer {
                layer: "features/diagnostics",
                benchmark_scenarios: Vec::new(),
                validation_commands: vec!["cargo test -p rldyourterm-diagnostics --locked"],
                notes: "Diagnostics emission is correctness- and schema-focused; tests validate typing and correlation behavior instead of throughput benchmarks.",
            },
        ],
    }
}
