//! Live view of a run: jobs, steps of the selected job, and its logs.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::{self, ACCENT, BAD, DIM, OK, WARN};
use crate::engine::Engine;
use crate::runner::{self, LogKind, RunEvent, RunOptions, Status};
use crate::util::{fmt_duration, Cancel};
use crate::workflow::plan::RunPlan;

pub enum RunAction {
    None,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Jobs,
    Steps,
    Logs,
}

struct StepView {
    name: String,
    status: Status,
    started: Option<Instant>,
    took: Option<Duration>,
}

struct JobView {
    name: String,
    status: Status,
    started: Option<Instant>,
    took: Option<Duration>,
    steps: Vec<StepView>,
    logs: Vec<(usize, LogKind, String)>,
}

pub struct RunView {
    title: String,
    engine: String,
    event: String,
    jobs: Vec<JobView>,
    rx: Receiver<RunEvent>,
    cancel: Cancel,
    handle: Option<JoinHandle<bool>>,
    started: Instant,
    finished: Option<(bool, Duration)>,
    job_sel: usize,
    /// Follow whichever job most recently started until the user picks one.
    auto_job: bool,
    step_cursor: usize,
    step_filter: Option<usize>,
    focus: Focus,
    /// Lines scrolled up from the bottom of the log.
    scroll: usize,
    follow: bool,
    log_height: usize,
    tick: usize,
}
