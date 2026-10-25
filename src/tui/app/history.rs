//! Past runs of this project: reopen one, run it again, or delete it.

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{App, Danger, Modal, Nav, Screen};
use crate::history::{self, Outcome, RunRecord, Store};
use crate::runner::Status;
use crate::tui::dialog::Confirm;
use crate::tui::run_view::RunView;
use crate::tui::theme::{self, BAD, OK};
use crate::util::fmt_duration;

pub struct History {
    runs: Vec<RunRecord>,
    sel: usize,
    /// False when there's no history folder on this system.
    available: bool,
}

impl History {
    pub fn load(store: Option<&Store>, root: &Path) -> Self {
        let mut h = Self {
            runs: Vec::new(),
            sel: 0,
            available: false,
        };
        h.reload(store, root);
        h
    }

    pub fn reload(&mut self, store: Option<&Store>, root: &Path) {
        self.available = store.is_some();
        self.runs = store.map(|s| s.list(root)).unwrap_or_default();
        self.sel = self.sel.min(self.runs.len().saturating_sub(1));
    }
}

fn outcome_icon(outcome: Outcome) -> Span<'static> {
    match outcome {
        Outcome::Success => Span::styled("✓", Style::new().fg(OK)),
        Outcome::Failure => Span::styled("✗", Style::new().fg(BAD)),
        Outcome::Cancelled => Span::styled("■", Style::new().fg(Color::Magenta)),
    }
}

impl App {
    pub(super) fn history_key(&mut self, h: &mut History, key: KeyEvent) -> Nav {
        let selected = h.runs.get(h.sel).cloned();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => h.sel = h.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                h.sel = (h.sel + 1).min(h.runs.len().saturating_sub(1))
            }
            KeyCode::Backspace | KeyCode::Esc | KeyCode::Left => return Nav::Back,
            KeyCode::Enter | KeyCode::Right => {
                if let (Some(run), Some(store)) = (selected, &self.store) {
                    let logs = store.logs(&run.id);
                    return Nav::Push(Screen::Run(Box::new(RunView::replay(&run, logs))));
                }
            }
            KeyCode::Char('r') => {
                if let Some(run) = selected {
                    match self.start_run(&run.workflow, run.job.clone(), &run.event) {
                        Ok(screen) => return Nav::Push(screen),
                        Err(e) => self.say(e, true),
                    }
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(run) = selected {
                    self.modal = Some(Modal::Confirm(Confirm::new(
                        "Delete run?",
                        format!(
                            "Delete \"{}\" from {}?",
                            run.title,
                            history::ago(run.started)
                        ),
                        "Delete",
                        Danger::DeleteRun(run.id),
                    )));
                }
            }
            KeyCode::Char('x') if !h.runs.is_empty() => {
                self.modal = Some(Modal::Confirm(Confirm::new(
                    "Clear history?",
                    format!("Delete all {} runs of this project?", h.runs.len()),
                    "Delete all",
                    Danger::ClearHistory,
                )));
            }
            _ => {}
        }
        Nav::Stay
    }

    pub(super) fn draw_history(
        &self,
        h: &History,
        f: &mut Frame,
        area: Rect,
    ) -> Vec<(&'static str, &'static str)> {
        let [list, details] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(7)]).areas(area);
        let block = theme::panel(&format!("Runs {}", h.runs.len()), true);
        if h.runs.is_empty() {
            let inner = block.inner(list);
            f.render_widget(block, list);
            let msg = if h.available {
                " No runs yet. Run something from the catalog and it shows up here."
            } else {
                " Run history isn't available: this system has no user data folder."
            };
            f.render_widget(
                Paragraph::new(vec![Line::raw(""), Line::styled(msg, theme::dim())]),
                inner,
            );
        } else {
            let width = list.width.saturating_sub(2) as usize;
            let rows = h
                .runs
                .iter()
                .map(|r| {
                    let right = format!(
                        "{} · {} · {} · {}",
                        r.engine,
                        r.event,
                        fmt_duration(std::time::Duration::from_millis(r.millis)),
                        history::ago(r.started)
                    );
                    theme::row(
                        outcome_icon(r.outcome),
                        &r.title,
                        right,
                        width,
                        Style::new(),
                    )
                })
                .collect();
            theme::render_list(f, list, block, rows, Some(h.sel), true);
        }

        let block = theme::panel("Details", false);
        let inner = block.inner(details);
        f.render_widget(block, details);
        if let Some(run) = h.runs.get(h.sel) {
            let tick = self.tick / 2;
            let mut lines: Vec<Line> = run
                .jobs
                .iter()
                .map(|j| {
                    let took = j
                        .millis
                        .map(|m| fmt_duration(std::time::Duration::from_millis(m)))
                        .unwrap_or_default();
                    let status = if j.status == Status::Running {
                        Status::Cancelled
                    } else {
                        j.status
                    };
                    Line::from(vec![
                        theme::status_icon(status, tick),
                        Span::raw(format!(" {}", j.name)),
                        Span::styled(format!("  {took}"), theme::dim()),
                    ])
                })
                .collect();
            lines.truncate(inner.height.saturating_sub(1) as usize);
            lines.push(Line::styled(
                format!(
                    "{}{}",
                    super::display_path(&run.workflow, &self.root),
                    run.job
                        .as_ref()
                        .map(|j| format!(" › {j}"))
                        .unwrap_or_default()
                ),
                theme::dim(),
            ));
            f.render_widget(Paragraph::new(lines), inner);
        }
        let mut hints = vec![("↑↓", "move")];
        if !h.runs.is_empty() {
            hints.extend([
                ("enter", "open"),
                ("r", "run again"),
                ("d", "delete"),
                ("x", "clear all"),
            ]);
        }
        hints.extend([("⌫", "back"), ("q", "quit")]);
        hints
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{demo_app, press, render, wait_for_run};
    use super::super::{Modal, Screen, Start};
    use crate::engine::Kind;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn runs_show_up_in_history_and_can_be_reopened_and_deleted() {
        let (mut app, _dir) = demo_app(Start::Workflows);
        app.active = Some(Kind::Native);
        let path = app.workflows[0].path.clone();
        let run = app.start_run(&path, Some("build".into()), "push").unwrap();
        app.stack.push(run);
        wait_for_run(&mut app);
        press(&mut app, KeyCode::Backspace); // back to the menu

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter); // Runs
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("Runs 1"), "{screen}");
        assert!(screen.contains("Maverick Native · push"), "{screen}");

        press(&mut app, KeyCode::Enter); // reopen
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("succeeded"), "{screen}");
        assert!(screen.contains("just now"), "{screen}");
        press(&mut app, KeyCode::Backspace);

        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(app.modal, Some(Modal::Confirm(_))));
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("can't be undone"), "{screen}");
        press(&mut app, KeyCode::Enter); // default is Cancel
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("Runs 1"), "cancel keeps the run");

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Enter);
        match app.stack.last() {
            Some(Screen::History(h)) => assert!(h.runs.is_empty()),
            _ => panic!("still on the history screen"),
        }
    }
}
