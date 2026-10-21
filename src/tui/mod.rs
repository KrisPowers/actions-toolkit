//! Terminal UI: the dashboard and the live run view.

mod home;
mod input;
mod run_view;
mod theme;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

const FRAME: Duration = Duration::from_millis(50);

/// Opens the interactive dashboard.
pub fn dashboard(root: PathBuf) -> Result<()> {
    let mut app = home::App::new(root)?;
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
