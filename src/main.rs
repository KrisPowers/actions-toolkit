mod cli;
mod config;
mod engine;
mod plain;
mod runner;
mod tui;
mod util;
mod workflow;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    match cli::Cli::parse().execute() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
