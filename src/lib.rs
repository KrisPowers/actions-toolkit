//! Maverick: run CI/CD workflows locally in containers, VMs, or a built-in sandbox. The
//! `maverick` and `workflows` binaries are thin entry points into [`main`].

mod cli;
mod config;
mod engine;
mod history;
mod runner;
mod tui;
mod util;
mod workflow;

use std::process::ExitCode;

pub use tui::Start;

/// Parses the command line and opens the TUI on `start`.
pub fn main(start: Start) -> ExitCode {
    match cli::run(start) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
