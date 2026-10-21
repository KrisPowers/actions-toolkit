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
