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

/// Runs `cmd`, streaming stdout/stderr line by line to `out`, honouring cancellation and timeouts.
pub fn stream(
    mut cmd: Command,
    cancel: &Cancel,
    timeout: Option<Duration>,
    out: &mut dyn FnMut(String),
) -> Result<ExecOutcome> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_process_group(&mut cmd);
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to start `{program}`"))?;

    let (tx, rx) = mpsc::channel();
    spawn_reader(child.stdout.take(), tx.clone());
    spawn_reader(child.stderr.take(), tx);

    let started = Instant::now();
    let mut outcome = ExecOutcome::default();
    let mut status = None;
    let mut exited_at: Option<Instant> = None;
    let mut killed = false;
    loop {
        let disconnected = match rx.recv_timeout(POLL) {
            Ok(line) => {
                out(line);
                false
            }
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            Err(mpsc::RecvTimeoutError::Disconnected) => true,
        };
        if status.is_none() {
            if let Some(s) = child.try_wait()? {
                status = Some(s);
                exited_at = Some(Instant::now());
            } else if !killed {
                if cancel.is_cancelled() {
                    outcome.cancelled = true;
                    kill_tree(&mut child);
                    killed = true;
                } else if timeout.is_some_and(|t| started.elapsed() > t) {
                    outcome.timed_out = true;
                    kill_tree(&mut child);
                    killed = true;
                }
            }
        }
        if disconnected {
            if status.is_some() {
                break;
            }
            thread::sleep(POLL);
        }
        // A background process may inherit our pipes and keep them open; don't wait on it forever.
        if exited_at.is_some_and(|t| t.elapsed() > Duration::from_millis(300)) {
            break;
        }
    }
    while let Ok(line) = rx.try_recv() {
        out(line);
    }
    let status = match status {
        Some(s) => s,
        None => child.wait()?,
    };
    outcome.code = status.code().unwrap_or(if killed { 137 } else { 1 });
    Ok(outcome)
}

fn spawn_reader<R: Read + Send + 'static>(src: Option<R>, tx: mpsc::Sender<String>) {
    let Some(src) = src else { return };
    thread::spawn(move || {
        let mut reader = BufReader::new(src);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buf);
                    let text = text.trim_end_matches(['\n', '\r']);
                    // Progress bars redraw with `\r`; keep only the final frame.
                    let text = text.rsplit('\r').next().unwrap_or_default();
                    if tx.send(strip_ansi(text)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}
