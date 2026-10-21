//! Process, filesystem and string helpers shared across engines.

use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// Cooperative cancellation flag shared between the UI, the scheduler and running processes.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecOutcome {
    pub code: i32,
    pub timed_out: bool,
    pub cancelled: bool,
}

impl ExecOutcome {
    pub fn success(&self) -> bool {
        self.code == 0 && !self.timed_out && !self.cancelled
    }
}

pub struct Captured {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

const POLL: Duration = Duration::from_millis(40);
