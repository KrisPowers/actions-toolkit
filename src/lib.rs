//! Actions Toolkit (ATK): run CI/CD workflows locally, each job in its own Docker container,
//! from a terminal UI, plain commands, or an AI agent. The `atk` binary is a thin entry
//! point into [`main`].

mod cli;
mod config;
mod engine;
mod headless;
mod history;
mod mcp;
mod report;
mod runner;
mod serve;
mod tui;
mod util;
mod workflow;

use std::process::ExitCode;

/// Parses the command line and runs the command, or opens the TUI.
pub fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
