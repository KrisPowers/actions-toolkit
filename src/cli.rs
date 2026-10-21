//! Command-line interface.

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::config::Config;
use crate::engine::{self, Kind};
use crate::runner::RunOptions;
use crate::workflow::{self, plan::RunPlan};
use crate::{plain, tui};

#[derive(Parser)]
#[command(
    name = "maverick",
    version,
    about = "Run CI/CD workflows locally in isolated containers, VMs, or a built-in sandbox",
    after_help = "Run `maverick` with no arguments to open the interactive dashboard."
)]
pub struct Cli {
    /// Project directory (defaults to the current directory)
    #[arg(short = 'C', long = "dir", global = true, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // parsed once per process
pub enum Command {
    /// Open the interactive dashboard (default)
    Tui,
    /// Run a workflow, or a single job of it
    Run(RunArgs),
    /// List workflows and jobs in this project
    #[command(alias = "ls")]
    List,
    /// Show which isolation engines are available on this machine
    Engines {
        /// Print machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// View or change configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Print the config file locations
    Path,
    /// Print the effective configuration
    Show,
    /// Set the default engine (e.g. docker, podman, native, lima)
    SetEngine { engine: String },
    /// Point a VM engine at an instance name or an SSH target (user@host[:port])
    SetTarget {
        engine: String,
        target: String,
        /// SSH private key to use with an SSH target
        #[arg(long)]
        identity: Option<String>,
    },
}

#[derive(Args, Clone, Default)]
pub struct RunArgs {
    /// Workflow file, file stem, or name. Defaults to every workflow triggered by --event.
    pub workflow: Option<String>,

    /// Run only this job (by id or name)
    #[arg(short, long)]
    pub job: Option<String>,

    /// Event that triggers the run
    #[arg(short, long, default_value = "push")]
    pub event: String,

    /// JSON file used as the event payload (github.event)
    #[arg(long, value_name = "FILE")]
    pub event_file: Option<PathBuf>,

    /// Engine to use (overrides config): docker, podman, native, lima, multipass, wsl, ...
    #[arg(long)]
    pub engine: Option<String>,

    /// Stream plain logs instead of the interactive view (automatic when not a terminal)
    #[arg(long)]
    pub plain: bool,

    /// Map a runs-on label to a container image
    #[arg(short = 'P', long = "platform", value_name = "LABEL=IMAGE")]
    pub platforms: Vec<String>,

    /// Secret available as secrets.KEY (`-s KEY` reads $KEY from the environment)
    #[arg(short = 's', long = "secret", value_name = "KEY[=VALUE]")]
    pub secrets: Vec<String>,

    /// dotenv-style file of secrets
    #[arg(long, value_name = "FILE")]
    pub secret_file: Option<PathBuf>,

    /// Variable available as vars.KEY
    #[arg(long = "var", value_name = "KEY=VALUE")]
    pub vars: Vec<String>,

    /// Input available as inputs.KEY (workflow_dispatch)
    #[arg(short = 'i', long = "input", value_name = "KEY=VALUE")]
    pub inputs: Vec<String>,

    /// Use the project directory in place instead of an isolated copy
    #[arg(long)]
    pub bind: bool,

    /// Always pull images and re-download actions
    #[arg(long)]
    pub pull: bool,

    /// Maximum number of jobs to run at once
    #[arg(long, value_name = "N")]
    pub parallel: Option<usize>,
}
