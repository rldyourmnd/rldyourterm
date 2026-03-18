// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Danil Silantyev, Global CEO NDDev. on.nddev.it.com (OpenNetwork)

mod cli;
mod coverage;
mod data;
mod environment;
mod fixtures;
mod governance;
mod live_display;
mod metrics;
mod report;
mod scenario_registry;
mod scenarios;
mod validate;

use anyhow::Result;
use clap::Parser;
use cli::{Commands, TopLevelCli};
use report::BenchmarkReport;

fn main() -> Result<()> {
    let cli = TopLevelCli::parse();
    if let Some(command) = &cli.command {
        return match command {
            Commands::Validate(args) => validate::run(args),
            Commands::Environment(args) => environment::run(args),
            Commands::Governance(args) => governance::run(args),
        };
    }

    let run = &cli.run;
    if run.iterations == 0 {
        anyhow::bail!("--iterations must be greater than zero");
    };

    let report = match run.suite {
        cli::SuiteArg::CanonicalHeadless => BenchmarkReport::Headless(scenarios::run_suite(run)?),
        cli::SuiteArg::LiveDisplay => BenchmarkReport::LiveDisplay(live_display::run_suite(run)?),
    };
    let rendered = report.render_stdout(run.format)?;
    println!("{rendered}");

    if let Some(output) = &run.output {
        report.write_output(output)?;
    }

    Ok(())
}
