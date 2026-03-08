use crate::cli::ScaleArg;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkloadScale {
    pub burst_lines: usize,
    pub scrollback_lines: usize,
    pub render_seed_lines: usize,
    pub delta_lines_per_iteration: usize,
}

impl WorkloadScale {
    pub const fn from_arg(arg: ScaleArg) -> Self {
        match arg {
            ScaleArg::Quick => Self {
                burst_lines: 1_000,
                scrollback_lines: 4_000,
                render_seed_lines: 320,
                delta_lines_per_iteration: 8,
            },
            ScaleArg::Standard => Self {
                burst_lines: 4_000,
                scrollback_lines: 16_000,
                render_seed_lines: 1_200,
                delta_lines_per_iteration: 16,
            },
            ScaleArg::Stress => Self {
                burst_lines: 12_000,
                scrollback_lines: 48_000,
                render_seed_lines: 2_400,
                delta_lines_per_iteration: 24,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Workload {
    pub ai_burst: Vec<u8>,
    pub scrollback_flood: Vec<u8>,
    pub render_seed: Vec<u8>,
    pub delta_batches: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WorkloadSummary {
    pub ai_burst_bytes: usize,
    pub scrollback_flood_bytes: usize,
    pub render_seed_bytes: usize,
    pub delta_batches: usize,
    pub delta_batch_bytes: usize,
}

impl Workload {
    pub fn generate(cols: u16, scale: WorkloadScale) -> Self {
        let ai_burst = build_ai_burst(cols, scale.burst_lines);
        let scrollback_flood = build_scrollback_flood(cols, scale.scrollback_lines);
        let render_seed = build_render_seed(cols, scale.render_seed_lines);
        let delta_batches = build_delta_batches(cols, scale.delta_lines_per_iteration);
        Self {
            ai_burst,
            scrollback_flood,
            render_seed,
            delta_batches,
        }
    }

    pub fn summary(&self) -> WorkloadSummary {
        WorkloadSummary {
            ai_burst_bytes: self.ai_burst.len(),
            scrollback_flood_bytes: self.scrollback_flood.len(),
            render_seed_bytes: self.render_seed.len(),
            delta_batches: self.delta_batches.len(),
            delta_batch_bytes: self.delta_batches.first().map_or(0, Vec::len),
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
    let mut batches = Vec::with_capacity(6);
    for batch_index in 0..6 {
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

fn fit_visible_text(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars.max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::{Workload, WorkloadScale};

    #[test]
    fn generated_workload_is_deterministic() {
        let first = Workload::generate(120, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        let second = Workload::generate(120, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        assert_eq!(first.ai_burst, second.ai_burst);
        assert_eq!(first.scrollback_flood, second.scrollback_flood);
        assert_eq!(first.render_seed, second.render_seed);
        assert_eq!(first.delta_batches, second.delta_batches);
    }

    #[test]
    fn workload_summary_reports_batch_sizes() {
        let workload =
            Workload::generate(100, WorkloadScale::from_arg(crate::cli::ScaleArg::Quick));
        let summary = workload.summary();
        assert!(summary.ai_burst_bytes > 0);
        assert!(summary.scrollback_flood_bytes > summary.ai_burst_bytes / 2);
        assert!(summary.render_seed_bytes > 0);
        assert_eq!(summary.delta_batches, 6);
        assert!(summary.delta_batch_bytes > 0);
    }
}
