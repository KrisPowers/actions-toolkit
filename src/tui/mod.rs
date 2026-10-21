//! Terminal UI: settings and guide (`maverick`), the workflow catalog and run history
//! (`maverick workflows`), and the live run view.

mod app;
mod dialog;
mod input;
mod run_view;
mod theme;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

pub use app::Start;

const FRAME: Duration = Duration::from_millis(50);

/// Opens the TUI on `start` for the project at `root`.
pub fn open(root: PathBuf, start: Start) -> Result<()> {
    let mut app = app::App::new(root, start)?;
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
