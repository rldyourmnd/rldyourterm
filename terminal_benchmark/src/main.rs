mod cli;
mod data;
mod metrics;
mod report;
mod scenarios;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.iterations == 0 {
        anyhow::bail!("--iterations must be greater than zero");
    }

    let report = scenarios::run_suite(&cli)?;
    let rendered = report.render_stdout(cli.format)?;
    println!("{rendered}");

    if let Some(output) = &cli.output {
        report.write_output(output)?;
    }

    Ok(())
}
