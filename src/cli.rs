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
