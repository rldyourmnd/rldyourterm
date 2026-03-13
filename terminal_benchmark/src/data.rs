// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

use crate::cli::ScaleArg;
use rldyourterm_services::render_mode::{GpuFailureKind, RenderMode};
use rldyourterm_services::session::SessionBoundary;
use rldyourterm_shell_integration::{ShellAvailability, ShellTarget};
use rldyourterm_ui::{SINGLE_WINDOW_BASELINE, UiRuntimeCommand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkloadScale {
    pub burst_lines: usize,
    pub scrollback_lines: usize,
    pub render_seed_lines: usize,
    pub delta_lines_per_iteration: usize,
    pub session_cycles: usize,
    pub ui_batch_repetitions: usize,
    pub settings_rounds: usize,
    pub shell_rounds: usize,
    pub font_passes: usize,
    pub surface_rounds: usize,
}

impl WorkloadScale {
    pub const fn from_arg(arg: ScaleArg) -> Self {
        match arg {
            ScaleArg::Quick => Self {
                burst_lines: 1_000,
                scrollback_lines: 4_000,
                render_seed_lines: 320,
                delta_lines_per_iteration: 8,
                session_cycles: 48,
                ui_batch_repetitions: 24,
                settings_rounds: 32,
                shell_rounds: 48,
                font_passes: 12,
                surface_rounds: 24,
            },
            ScaleArg::Standard => Self {
                burst_lines: 4_000,
                scrollback_lines: 16_000,
                render_seed_lines: 1_200,
                delta_lines_per_iteration: 16,
                session_cycles: 192,
                ui_batch_repetitions: 96,
                settings_rounds: 128,
                shell_rounds: 192,
                font_passes: 48,
                surface_rounds: 96,
            },
            ScaleArg::Stress => Self {
                burst_lines: 12_000,
                scrollback_lines: 48_000,
                render_seed_lines: 2_400,
                delta_lines_per_iteration: 24,
                session_cycles: 768,
                ui_batch_repetitions: 384,
                settings_rounds: 512,
                shell_rounds: 768,
                font_passes: 192,
                surface_rounds: 384,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellCase {
    pub requested: ShellTarget,
    pub availability: ShellAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyCase {
    AcquireTimeout,
    AcquireOutdated,
    AcquireLost,
    AcquireOutOfMemory,
    AcquireOther,
    ConfigureZeroWidth,
    ConfigureZeroHeight,
    ExtentNominal {
        width: u32,
        height: u32,
        max_texture_dimension_2d: u32,
    },
    ExtentClamped {
        width: u32,
        height: u32,
        max_texture_dimension_2d: u32,
    },
    FrameLatency {
        desired_maximum_frame_latency: u32,
    },
}

#[derive(Debug, Clone)]
pub struct Workload {
    pub ai_burst: Vec<u8>,
    pub scrollback_flood: Vec<u8>,
    pub render_seed: Vec<u8>,
    pub delta_batches: Vec<Vec<u8>>,
    pub session_cycles: usize,
    pub session_boundaries: Vec<SessionBoundary>,
    pub ui_command_batches: Vec<Vec<UiRuntimeCommand>>,
    pub ui_batch_repetitions: usize,
    pub settings_palette_inputs: Vec<&'static str>,
    pub settings_rounds: usize,
    pub shell_cases: Vec<ShellCase>,
    pub shell_rounds: usize,
    pub font_glyphs: Vec<char>,
    pub font_passes: usize,
    pub surface_policy_cases: Vec<SurfacePolicyCase>,
    pub surface_rounds: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadSummary {
    pub ai_burst_bytes: usize,
    pub scrollback_flood_bytes: usize,
    pub render_seed_bytes: usize,
    pub delta_batches: usize,
    pub delta_batch_bytes: usize,
    pub session_cycles: usize,
    pub session_boundaries_per_cycle: usize,
    pub ui_batch_repetitions: usize,
    pub ui_batch_templates: usize,
    pub ui_commands_per_template: usize,
    pub settings_inputs: usize,
    pub settings_rounds: usize,
    pub shell_cases: usize,
    pub shell_rounds: usize,
    pub font_glyphs: usize,
    pub font_passes: usize,
    pub surface_policy_cases: usize,
    pub surface_rounds: usize,
}

impl Workload {
    pub fn generate(cols: u16, scale: WorkloadScale) -> Self {
        let ai_burst = build_ai_burst(cols, scale.burst_lines);
        let scrollback_flood = build_scrollback_flood(cols, scale.scrollback_lines);
        let render_seed = build_render_seed(cols, scale.render_seed_lines);
        let delta_batches = build_delta_batches(cols, scale.delta_lines_per_iteration);
        let session_boundaries = build_session_boundaries();
        let ui_command_batches = build_ui_command_batches();
        let settings_palette_inputs = build_settings_palette_inputs();
        let shell_cases = build_shell_cases();
        let font_glyphs = build_font_glyphs();
        let surface_policy_cases = build_surface_policy_cases();
        Self {
            ai_burst,
            scrollback_flood,
            render_seed,
            delta_batches,
            session_cycles: scale.session_cycles,
            session_boundaries,
            ui_command_batches,
            ui_batch_repetitions: scale.ui_batch_repetitions,
            settings_palette_inputs,
            settings_rounds: scale.settings_rounds,
            shell_cases,
            shell_rounds: scale.shell_rounds,
            font_glyphs,
            font_passes: scale.font_passes,
            surface_policy_cases,
            surface_rounds: scale.surface_rounds,
        }
    }

    pub fn summary(&self) -> WorkloadSummary {
        WorkloadSummary {
            ai_burst_bytes: self.ai_burst.len(),
            scrollback_flood_bytes: self.scrollback_flood.len(),
            render_seed_bytes: self.render_seed.len(),
            delta_batches: self.delta_batches.len(),
            delta_batch_bytes: self.delta_batches.first().map_or(0, Vec::len),
            session_cycles: self.session_cycles,
            session_boundaries_per_cycle: self.session_boundaries.len(),
            ui_batch_repetitions: self.ui_batch_repetitions,
            ui_batch_templates: self.ui_command_batches.len(),
            ui_commands_per_template: self.ui_command_batches.first().map_or(0, Vec::len),
            settings_inputs: self.settings_palette_inputs.len(),
            settings_rounds: self.settings_rounds,
            shell_cases: self.shell_cases.len(),
            shell_rounds: self.shell_rounds,
            font_glyphs: self.font_glyphs.len(),
            font_passes: self.font_passes,
            surface_policy_cases: self.surface_policy_cases.len(),
            surface_rounds: self.surface_rounds,
        }
    }
}

fn build_ai_burst(cols: u16, lines: usize) -> Vec<u8> {
    let width = usize::from(cols.max(32));
    let mut output = String::with_capacity(lines.saturating_mul(width + 64));
    for index in 0..lines {
        let phase = index % 4;
        let body = match phase {
            0 => format!(
                "assistant step={index:05} tokens={} latency={}ms reasoning=stabilize-runtime-boundaries unicode=Δλ中",
                128 + (index % 512),
                12 + (index % 53),
            ),
            1 => format!(
                "tool apply diff file=src/module_{:03}.rs status=ok diagnostics={} warnings={}",
                index % 97,
                index % 11,
                index % 3,
            ),
            2 => format!(
                "shell prompt=fish cwd=/workspace/session/{:03} command='cargo test --workspace --locked'",
                index % 173,
            ),
            _ => format!(
                "summary rows={} cols={} scrollback={} retries={} palette=ctrl-shift-p",
                48 + (index % 5),
                width,
                50_000,
                index % 4,
            ),
        };
        let visible = fit_visible_text(&body, width.saturating_sub(16));
        let decorated = format!(
            "\x1b[1;32m{:05}\x1b[0m \x1b[38;5;81m{visible}\x1b[0m\r\n",
            index,
        );
        output.push_str(&decorated);
    }
    output.into_bytes()
}

fn build_scrollback_flood(cols: u16, lines: usize) -> Vec<u8> {
    let width = usize::from(cols.max(32));
    let mut output = String::with_capacity(lines.saturating_mul(width + 32));
    for index in 0..lines {
        let text = fit_visible_text(
            &format!(
                "scrollback line {:06} {} {}",
                index,
                "#".repeat(width / 3),
                "data".repeat(3),
            ),
            width,
        );
        output.push_str(&text);
        output.push_str("\r\n");
    }
    output.into_bytes()
}

fn build_render_seed(cols: u16, lines: usize) -> Vec<u8> {
    let width = usize::from(cols.max(32));
    let mut output = String::with_capacity(lines.saturating_mul(width + 32));
    for index in 0..lines {
        let body = if index % 6 == 0 {
            format!(
                "┌ frame {:04} {} ┐",
                index,
                "─".repeat(width.saturating_sub(20))
            )
        } else if index % 6 == 5 {
            format!(
                "└ frame {:04} {} ┘",
                index,
                "─".repeat(width.saturating_sub(20))
            )
        } else {
            format!(
                "│ row {:04} color={} bold={} italic={} text={} │",
                index,
                index % 256,
                index % 2,
                (index / 2) % 2,
                fit_visible_text("render-seed-glyphs-abcdef123456", width.saturating_sub(32)),
            )
        };
        let decorated = format!(
            "\x1b[38;5;{}m{}\x1b[0m\r\n",
            16 + (index % 200),
            fit_visible_text(&body, width),
        );
        output.push_str(&decorated);
    }
    output.into_bytes()
}

fn build_delta_batches(cols: u16, lines_per_iteration: usize) -> Vec<Vec<u8>> {
    let width = usize::from(cols.max(24));
    let mut batches = Vec::with_capacity(3);
    for batch_index in 0..3 {
        let mut output = String::new();
        for line_index in 0..lines_per_iteration {
            let absolute = batch_index * lines_per_iteration + line_index;
            let body = fit_visible_text(
                &format!(
                    "delta batch={batch_index} row={line_index} tick={absolute:04} cpu-path={} markers={}",
                    if batch_index % 2 == 0 {
                        "steady"
                    } else {
                        "burst"
                    },
                    "<>[]{}".repeat(2),
                ),
                width,
            );
            output.push_str(&format!("\x1b[1;3{}m{}\x1b[0m\r\n", absolute % 8, body));
        }
        batches.push(output.into_bytes());
    }
    batches
}

fn build_session_boundaries() -> Vec<SessionBoundary> {
    vec![
        SessionBoundary::PtyWrite,
        SessionBoundary::PtyResize,
        SessionBoundary::PtyWriterAcquire,
    ]
}

fn build_ui_command_batches() -> Vec<Vec<UiRuntimeCommand>> {
    vec![
        vec![
            UiRuntimeCommand::AssertSingleWindow {
                requested: SINGLE_WINDOW_BASELINE,
            },
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::RecoverableBoundary(SessionBoundary::PtyWrite),
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::ResyncCadence {
                refresh_rate_millihz: 144_000,
            },
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SurfaceError,
                observed_at_millis: 1_000,
            },
            UiRuntimeCommand::GpuFramePresented,
            UiRuntimeCommand::RequestStop,
            UiRuntimeCommand::MarkStopped,
        ],
        vec![
            UiRuntimeCommand::AssertSingleWindow {
                requested: SINGLE_WINDOW_BASELINE,
            },
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::SetRenderMode(RenderMode::Cpu),
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SubmitError,
                observed_at_millis: 1_500,
            },
            UiRuntimeCommand::GpuFramePresented,
            UiRuntimeCommand::ResyncCadenceAfterTransfer {
                refresh_rate_millihz: 60_000,
            },
            UiRuntimeCommand::RequestStop,
            UiRuntimeCommand::MarkStopped,
        ],
        vec![
            UiRuntimeCommand::AssertSingleWindow {
                requested: SINGLE_WINDOW_BASELINE,
            },
            UiRuntimeCommand::Tick,
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SurfaceError,
                observed_at_millis: 1_000,
            },
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SubmitError,
                observed_at_millis: 1_500,
            },
            UiRuntimeCommand::GpuFailure {
                kind: GpuFailureKind::SwapchainOutOfDate,
                observed_at_millis: 2_000,
            },
            UiRuntimeCommand::SetRenderMode(RenderMode::Auto),
            UiRuntimeCommand::RequestStop,
            UiRuntimeCommand::MarkStopped,
        ],
    ]
}

fn build_settings_palette_inputs() -> Vec<&'static str> {
    vec![
        "mode cpu",
        "mode cpu",
        "shell zsh",
        "shell auto-init on",
        "shell auto",
        "shell auto-init on",
        "render cadence monitor-auto",
        "theme set aurora",
        "profile throughput",
        "debug on",
        "theme set neon",
        "  MODE\tGPU  ",
    ]
}

fn build_shell_cases() -> Vec<ShellCase> {
    vec![
        ShellCase {
            requested: ShellTarget::Fish,
            availability: ShellAvailability {
                fish_available: true,
                starship_available: true,
                zsh_available: true,
            },
        },
        ShellCase {
            requested: ShellTarget::Fish,
            availability: ShellAvailability {
                fish_available: true,
                starship_available: false,
                zsh_available: true,
            },
        },
        ShellCase {
            requested: ShellTarget::Auto,
            availability: ShellAvailability {
                fish_available: false,
                starship_available: true,
                zsh_available: true,
            },
        },
        ShellCase {
            requested: ShellTarget::Auto,
            availability: ShellAvailability {
                fish_available: true,
                starship_available: false,
                zsh_available: true,
            },
        },
        ShellCase {
            requested: ShellTarget::Zsh,
            availability: ShellAvailability {
                fish_available: true,
                starship_available: true,
                zsh_available: false,
            },
        },
        ShellCase {
            requested: ShellTarget::Fish,
            availability: ShellAvailability {
                fish_available: false,
                starship_available: false,
                zsh_available: false,
            },
        },
    ]
}

fn build_font_glyphs() -> Vec<char> {
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 Δλ中┌─│└█░▀▄→←"
        .chars()
        .collect()
}

fn build_surface_policy_cases() -> Vec<SurfacePolicyCase> {
    vec![
        SurfacePolicyCase::AcquireTimeout,
        SurfacePolicyCase::AcquireOutdated,
        SurfacePolicyCase::AcquireLost,
        SurfacePolicyCase::AcquireOutOfMemory,
        SurfacePolicyCase::AcquireOther,
        SurfacePolicyCase::ConfigureZeroWidth,
        SurfacePolicyCase::ConfigureZeroHeight,
        SurfacePolicyCase::ExtentNominal {
            width: 1280,
            height: 720,
            max_texture_dimension_2d: 4096,
        },
        SurfacePolicyCase::ExtentClamped {
            width: 16_384,
            height: 12_288,
            max_texture_dimension_2d: 4096,
        },
        SurfacePolicyCase::FrameLatency {
            desired_maximum_frame_latency: 3,
        },
    ]
}

fn fit_visible_text(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars.max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::{SurfacePolicyCase, Workload, WorkloadScale};

    #[test]
    fn generated_workload_is_deterministic() {
        let first = Workload::generate(120, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        let second = Workload::generate(120, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        assert_eq!(first.ai_burst, second.ai_burst);
        assert_eq!(first.scrollback_flood, second.scrollback_flood);
        assert_eq!(first.render_seed, second.render_seed);
        assert_eq!(first.delta_batches, second.delta_batches);
        assert_eq!(first.session_boundaries, second.session_boundaries);
        assert_eq!(first.ui_command_batches, second.ui_command_batches);
        assert_eq!(
            first.settings_palette_inputs,
            second.settings_palette_inputs
        );
        assert_eq!(first.shell_cases, second.shell_cases);
        assert_eq!(first.font_glyphs, second.font_glyphs);
        assert_eq!(first.surface_policy_cases, second.surface_policy_cases);
    }

    #[test]
    fn workload_summary_reports_extended_coverage_counts() {
        let workload =
            Workload::generate(100, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        let summary = workload.summary();
        assert!(summary.ai_burst_bytes > 0);
        assert!(summary.scrollback_flood_bytes > summary.ai_burst_bytes / 2);
        assert!(summary.render_seed_bytes > 0);
        assert_eq!(summary.delta_batches, 3);
        assert!(summary.delta_batch_bytes > 0);
        assert!(summary.session_cycles > 0);
        assert!(summary.ui_batch_templates >= 3);
        assert!(summary.settings_inputs >= 8);
        assert!(summary.shell_cases >= 4);
        assert!(summary.font_glyphs > 0);
        assert!(summary.surface_policy_cases >= 8);
    }

    #[test]
    fn surface_policy_workload_includes_clamped_extent_case() {
        let workload =
            Workload::generate(100, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        assert!(
            workload
                .surface_policy_cases
                .iter()
                .any(|case| matches!(case, SurfacePolicyCase::ExtentClamped { .. }))
        );
    }
}
