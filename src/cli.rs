//! Command line: `maverick` opens settings and the guide, `workflows` the workflow catalog
//! and run history. Everything else happens inside the TUI.

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser};

use crate::tui::{self, Start};
use crate::workflow;

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Project folder (defaults to the current directory)
    #[arg(short = 'C', long = "dir", value_name = "DIR")]
    dir: Option<PathBuf>,
}

pub fn run(screen: Start) -> Result<ExitCode> {
    let (name, about) = match screen {
        Start::Settings => (
            "maverick",
            "Maverick settings and guide. Run `workflows` to browse and run your workflows.",
        ),
        Start::Workflows => (
            "workflows",
            "Browse and run your CI/CD workflows locally, and see past runs.",
        ),
    };
    let matches = Cli::command()
        .name(name)
        .bin_name(name)
        .about(about)
        .get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let start = match cli.dir {
        Some(d) => d
            .canonicalize()
            .with_context(|| format!("{} does not exist", d.display()))?,
        None => env::current_dir()?,
    };
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!("{name} needs an interactive terminal");
    }
    tui::open(workflow::find_root(&start), screen)?;
    Ok(ExitCode::SUCCESS)
}
