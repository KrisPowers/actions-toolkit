//! Terminal UI: the dashboard (`maverick`) and the live run view (`maverick run`).

mod home;
mod run_view;
mod theme;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::engine::Engine;
use crate::runner::RunOptions;
use crate::workflow::plan::RunPlan;
use run_view::{RunAction, RunView};

const FRAME: Duration = Duration::from_millis(50);

/// Opens the interactive dashboard.
pub fn dashboard(root: PathBuf) -> Result<()> {
    let mut app = home::App::new(root)?;
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
