//! Command line: `maverick [PATH]` opens the TUI for that project. Everything else happens
//! inside the TUI.

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use crate::{tui, workflow};

#[derive(Parser)]
#[command(
    name = "maverick",
    version,
    about = "Run CI/CD workflows locally in containers, VMs, or a built-in sandbox"
)]
pub struct Cli {
    /// Project folder (defaults to the current directory)
    pub path: Option<PathBuf>,
}

impl Cli {
    pub fn execute(self) -> Result<ExitCode> {
        let start = match self.path {
            Some(d) => d
                .canonicalize()
                .with_context(|| format!("{} does not exist", d.display()))?,
            None => env::current_dir()?,
        };
        if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
            bail!("maverick needs an interactive terminal");
        }
        tui::dashboard(workflow::find_root(&start))?;
        Ok(ExitCode::SUCCESS)
    }
}
