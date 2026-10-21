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

/// Shows a single run full-screen; returns whether it succeeded.
pub fn run_once(
    plan: Arc<RunPlan>,
    engine: Arc<dyn Engine>,
    opts: Arc<RunOptions>,
) -> Result<bool> {
    let mut terminal = ratatui::init();
    let mut view = RunView::start(plan, engine, opts);
    let result = (|| -> Result<()> {
        loop {
            view.update();
            terminal.draw(|f| view.draw(f, f.area()))?;
            if event::poll(FRAME)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let ctrl_c = key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl_c && !view.is_finished() {
                        view.cancel();
                        continue;
                    }
                    if let RunAction::Close = view.on_key(key) {
                        return Ok(());
                    }
                }
            }
        }
    })();
    ratatui::restore();
    result?;
    Ok(view.success())
}
