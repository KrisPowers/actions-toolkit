//! Command line: `maverick` opens settings and the guide, `maverick workflows` the workflow
//! catalog and run history. Everything else happens inside the TUI.

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::tui::{self, Start};
use crate::workflow;

#[derive(Parser)]
#[command(
    name = "maverick",
    version,
    about = "Run CI/CD workflows locally in containers, VMs, or a built-in sandbox",
    after_help = "Run `maverick` for settings and the guide, `maverick workflows` to run workflows."
)]
pub struct Cli {
    /// Project folder (defaults to the current directory)
    #[arg(short = 'C', long = "dir", global = true, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Browse and run your workflows, and see past runs
    Workflows,
}

impl Cli {
    pub fn execute(self) -> Result<ExitCode> {
        let start = match self.dir {
            Some(d) => d
                .canonicalize()
                .with_context(|| format!("{} does not exist", d.display()))?,
            None => env::current_dir()?,
        };
        if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
            bail!("maverick needs an interactive terminal");
        }
        let screen = match self.command {
            None => Start::Settings,
            Some(Command::Workflows) => Start::Workflows,
        };
        tui::open(workflow::find_root(&start), screen)?;
        Ok(ExitCode::SUCCESS)
    }
}
