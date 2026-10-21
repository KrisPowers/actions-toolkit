//! Line-oriented output for non-interactive use (CI, pipes, `--plain`).

use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::engine::Engine;
use crate::runner::{self, LogKind, RunEvent, RunOptions, Status};
use crate::util::{fmt_duration, Cancel};
use crate::workflow::plan::RunPlan;

struct Paint(bool);
