mod ast;
mod cli;
mod cli_tests;
mod core;
mod lockfile;
mod manifest;
mod parser;
mod symlink_tests;

#[cfg(test)]
mod tests;

use anyhow::Result;
use tracing_subscriber::{FmtSubscriber, filter::LevelFilter};
use clap::Parser;

use cli::{Cli, run_cli};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let level = if cli.quiet {
        LevelFilter::OFF
    } else {
        match cli.verbose {
            0 => LevelFilter::WARN,
            1 => LevelFilter::INFO,
            2 => LevelFilter::DEBUG,
            _ => LevelFilter::TRACE,
        }
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    run_cli(cli)
}
