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
