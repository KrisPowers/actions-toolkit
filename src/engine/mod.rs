//! Isolation engines: where a job's steps actually execute.
//!
//! Every engine hands out a [`Session`]: an isolated environment holding a copy of the
//! project, in which the runner can execute commands and exchange small files.

mod container;
mod native;
mod vm;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::config::Config;
use crate::util::{self, Cancel, ExecOutcome};
