//! The dashboard: pick a workflow or job on the left, an engine on the right, press enter.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::run_view::{RunAction, RunView};
use super::theme::{self, ACCENT, BAD, OK, WARN};
use super::FRAME;
use crate::config::Config;
use crate::engine::{self, Availability, EngineInfo, Family, Kind};
use crate::runner::RunOptions;
use crate::workflow::{self, plan::RunPlan, WorkflowFile};

const EVENTS: [&str; 5] = [
    "push",
    "pull_request",
    "workflow_dispatch",
    "schedule",
    "release",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Workflows,
    Engines,
}

enum Row {
    Workflow(usize),
    Job(usize, String),
    Broken(usize),
}

pub struct App {
    root: PathBuf,
    cfg: Config,
    workflows: Vec<WorkflowFile>,
    rows: Vec<Row>,
    row_sel: usize,
    engines: Vec<EngineInfo>,
    engine_sel: usize,
    detect_rx: Option<Receiver<Vec<EngineInfo>>>,
    active: Option<Kind>,
    focus: Focus,
    event_idx: usize,
    flash: Option<(String, bool, Instant)>,
    run: Option<RunView>,
    tick: usize,
    quit: bool,
}
