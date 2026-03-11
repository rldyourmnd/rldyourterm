// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Danil Silantyev (rldyourmnd), NDDev OpenNetwork

mod cli;
mod coverage;
mod data;
mod fixtures;
mod live_display;
mod metrics;
mod report;
mod scenario_registry;
mod scenarios;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use report::BenchmarkReport;

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.iterations == 0 {
        anyhow::bail!("--iterations must be greater than zero");
    }

    let report = match cli.suite {
        cli::SuiteArg::CanonicalHeadless => BenchmarkReport::Headless(scenarios::run_suite(&cli)?),
        cli::SuiteArg::LiveDisplay => BenchmarkReport::LiveDisplay(live_display::run_suite(&cli)?),
    };
    let rendered = report.render_stdout(cli.format)?;
    println!("{rendered}");

    if let Some(output) = &cli.output {
        report.write_output(output)?;
    }

    Ok(())
}
