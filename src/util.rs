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

#[cfg(unix)]
fn own_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn own_process_group(_cmd: &mut Command) {}

/// Kills a child and everything it spawned.
pub fn kill_tree(child: &mut Child) {
    let pid = child.id();
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

fn kill_pid(pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-s", "KILL", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Runs `cmd` to completion and captures its output, killing it after `timeout`.
pub fn capture(mut cmd: Command, timeout: Duration) -> Result<Captured> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start `{program}`"))?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(out)) => Ok(Captured {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Ok(Err(err)) => Err(err).with_context(|| format!("`{program}` failed")),
        Err(_) => {
            kill_pid(pid);
            bail!("`{program}` timed out after {}s", timeout.as_secs())
        }
    }
}

/// Convenience wrapper: capture `program args...`, returning `None` if it can't run.
pub fn probe<S: AsRef<OsStr>>(program: S, args: &[&str]) -> Option<Captured> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    capture(cmd, Duration::from_secs(6)).ok()
}

/// Runs `cmd` while `feed` writes to its stdin. Stdout is discarded.
pub fn pipe_into(
    mut cmd: Command,
    feed: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<Captured> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start `{program}`"))?;
    let stderr = child.stderr.take();
    let err_reader = thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let fed = {
        let mut stdin = child.stdin.take().context("stdin unavailable")?;
        feed(&mut stdin)
    };
    let status = child.wait()?;
    let stderr = err_reader.join().unwrap_or_default();
    fed.with_context(|| format!("streaming data to `{program}` failed: {}", stderr.trim()))?;
    Ok(Captured {
        success: status.success(),
        stdout: String::new(),
        stderr,
    })
}

fn walker(src: &Path, respect_ignores: bool) -> ignore::Walk {
    let mut builder = ignore::WalkBuilder::new(src);
    builder.hidden(false).follow_links(false);
    if respect_ignores {
        builder
            .git_ignore(true)
            .git_exclude(true)
            .git_global(false)
            .ignore(true)
            .parents(false)
            .require_git(false);
    } else {
        builder.standard_filters(false);
    }
    builder.build()
}

/// Copies a project into a sandbox, skipping anything `.gitignore`d (build output, node_modules, ...).
pub fn copy_workspace(src: &Path, dst: &Path) -> Result<usize> {
    copy_with(src, dst, true)
}

/// Copies a directory verbatim.
pub fn copy_tree(src: &Path, dst: &Path) -> Result<usize> {
    copy_with(src, dst, false)
}

fn copy_with(src: &Path, dst: &Path, respect_ignores: bool) -> Result<usize> {
    fs::create_dir_all(dst)?;
    let mut files = 0;
    for entry in walker(src, respect_ignores) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(rel);
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            fs::create_dir_all(&target)?;
        } else if ft.is_symlink() {
            copy_symlink(entry.path(), &target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
            files += 1;
        }
    }
    Ok(files)
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    let link = fs::read_link(src)?;
    let _ = fs::remove_file(dst);
    std::os::unix::fs::symlink(link, dst)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    if src.is_file() {
        fs::copy(src, dst)?;
    }
    Ok(())
}

/// Writes a tar stream of `src` into `out`.
pub fn tar_dir(src: &Path, out: &mut dyn Write, respect_ignores: bool) -> Result<()> {
    let mut builder = tar::Builder::new(out);
    builder.follow_symlinks(false);
    for entry in walker(src, respect_ignores) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        // tar paths always use forward slashes, even when built on Windows.
        let name = rel.to_string_lossy().replace('\\', "/");
        builder.append_path_with_name(entry.path(), name)?;
    }
    builder.finish()?;
    Ok(())
}

/// Removes ANSI escape sequences so logs render cleanly inside the TUI.
pub fn strip_ansi(s: &str) -> String {
    if !s.contains('\x1b') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}
