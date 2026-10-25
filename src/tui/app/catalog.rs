//! The catalog: every workflow found for this project and its jobs. Enter runs one.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{display_path, first_line, App, Nav, EVENTS};
use crate::tui::theme::{self, ACCENT, BAD, WARN};
use crate::workflow::{self, Source, WorkflowFile};

enum Row {
    Workflow(usize),
    Job(usize, String),
    Broken(usize),
}

pub struct Catalog {
    rows: Vec<Row>,
    sel: usize,
}

impl Catalog {
    pub fn new(workflows: &[WorkflowFile]) -> Self {
        let mut c = Self {
            rows: Vec::new(),
            sel: 0,
        };
        c.rebuild(workflows);
        c
    }

    /// Recomputes the rows after the workflow list changed.
    pub fn rebuild(&mut self, workflows: &[WorkflowFile]) {
        self.rows = workflows
            .iter()
            .enumerate()
            .flat_map(|(i, f)| match f.workflow() {
                Some(wf) => std::iter::once(Row::Workflow(i))
                    .chain(wf.jobs.keys().map(move |k| Row::Job(i, k.clone())))
                    .collect::<Vec<_>>(),
                None => vec![Row::Broken(i)],
            })
            .collect();
        self.sel = self.sel.min(self.rows.len().saturating_sub(1));
    }
}

impl App {
    pub(super) fn catalog_key(&mut self, c: &mut Catalog, key: KeyEvent) -> Nav {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => c.sel = c.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                c.sel = (c.sel + 1).min(c.rows.len().saturating_sub(1))
            }
            KeyCode::Backspace | KeyCode::Esc | KeyCode::Left => return Nav::Back,
            KeyCode::Char('e') => {
                let i = EVENTS
                    .iter()
                    .position(|e| *e == self.event)
                    .map_or(0, |i| i + 1);
                self.event = EVENTS[i % EVENTS.len()].to_string();
            }
            KeyCode::Char('r') => {
                self.reload_workflows();
                c.rebuild(&self.workflows);
                self.say("Rescanned workflows", false);
            }
            KeyCode::Enter | KeyCode::Right => {
                let (file, job) = match c.rows.get(c.sel) {
                    Some(Row::Workflow(i)) => (*i, None),
                    Some(Row::Job(i, key)) => (*i, Some(key.clone())),
                    Some(Row::Broken(i)) => {
                        let file = &self.workflows[*i];
                        let err = file.parsed.as_ref().err().cloned().unwrap_or_default();
                        let msg = format!("{} is invalid: {}", file.file_name, first_line(&err));
                        self.say(msg, true);
                        return Nav::Stay;
                    }
                    None => return Nav::Stay,
                };
                let path = self.workflows[file].path.clone();
                let event = self.event.clone();
                match self.start_run(&path, job, &event) {
                    Ok(run) => return Nav::Push(run),
                    Err(e) => self.say(e, true),
                }
            }
            _ => {}
        }
        Nav::Stay
    }

    pub(super) fn draw_catalog(
        &self,
        c: &Catalog,
        f: &mut Frame,
        area: Rect,
    ) -> Vec<(&'static str, &'static str)> {
        let [list, details] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(6)]).areas(area);
        let block = theme::panel("Catalog", true);
        if c.rows.is_empty() {
            let inner = block.inner(list);
            f.render_widget(block, list);
            let text = vec![
                Line::raw(""),
                Line::styled(" No workflows found.", theme::bold()),
                Line::styled(
                    " Add a folder or file to search under Workflow locations in `atk settings`.",
                    theme::dim(),
                ),
            ];
            f.render_widget(Paragraph::new(text), inner);
        } else {
            let width = list.width.saturating_sub(2) as usize;
            let rows = c
                .rows
                .iter()
                .map(|row| self.catalog_row(row, width))
                .collect();
            theme::render_list(f, list, block, rows, Some(c.sel), true);
        }
        self.draw_catalog_details(c, f, details);
        vec![
            ("↑↓", "move"),
            ("enter", "run"),
            ("e", "event"),
            ("r", "rescan"),
            ("⌫", "back"),
            ("q", "quit"),
        ]
    }

    fn catalog_row(&self, row: &Row, width: usize) -> Line<'static> {
        match row {
            Row::Workflow(i) => {
                let file = &self.workflows[*i];
                let triggered = file.events().contains(&self.event);
                let icon = Span::styled(
                    "▾",
                    Style::new().fg(if triggered { ACCENT } else { theme::DIM }),
                );
                let right = match file.source {
                    Source::Auto => file.file_name.clone(),
                    other => format!("{} · {}", other.label(), file.file_name),
                };
                theme::row(icon, &file.display_name(), right, width, theme::bold())
            }
            Row::Job(i, key) => {
                let wf = self.workflows[*i]
                    .workflow()
                    .expect("job rows only exist for parsed workflows");
                let job = &wf.jobs[key];
                let matrix = workflow::plan::expand_matrix(job)
                    .map(|c| c.len())
                    .unwrap_or(1);
                let mut right = job.runs_on().join(",");
                if matrix > 1 {
                    right = format!("×{matrix}  {right}");
                }
                let name = job
                    .name
                    .clone()
                    .filter(|n| !n.contains("${{"))
                    .unwrap_or_else(|| key.clone());
                theme::row(
                    Span::styled("  ├", theme::dim()),
                    &name,
                    right,
                    width,
                    Style::new(),
                )
            }
            Row::Broken(i) => theme::row(
                Span::styled("✗", Style::new().fg(BAD)),
                &self.workflows[*i].file_name,
                "invalid".into(),
                width,
                Style::new().fg(BAD),
            ),
        }
    }

    fn draw_catalog_details(&self, c: &Catalog, f: &mut Frame, area: Rect) {
        let block = theme::panel("Details", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let label = |s: &str| Span::styled(format!("{s:<10}"), theme::dim());
        let lines: Vec<Line> = match c.rows.get(c.sel) {
            Some(Row::Workflow(i)) => {
                let file = &self.workflows[*i];
                let events = file.events();
                let jobs = file.workflow().map(|w| w.jobs.len()).unwrap_or(0);
                let note = if events.contains(&self.event) {
                    Span::raw("")
                } else {
                    Span::styled(
                        format!("  (not normally triggered by {})", self.event),
                        Style::new().fg(WARN),
                    )
                };
                vec![
                    Line::from(vec![
                        label("workflow"),
                        Span::styled(file.display_name(), theme::bold()),
                        Span::styled(
                            format!(
                                "  {}  ({})",
                                display_path(&file.path, &self.root),
                                file.source.label()
                            ),
                            theme::dim(),
                        ),
                    ]),
                    Line::from(vec![label("on"), Span::raw(events.join(", ")), note]),
                    Line::from(vec![label("jobs"), Span::raw(jobs.to_string())]),
                    Line::styled(
                        format!("Enter: run all jobs as `{}`", self.event),
                        Style::new().fg(ACCENT),
                    ),
                ]
            }
            Some(Row::Job(i, key)) => {
                let wf = self.workflows[*i].workflow().expect("parsed");
                let job = &wf.jobs[key];
                let needs = job.needs();
                let matrix = workflow::plan::expand_matrix(job)
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|e| e);
                vec![
                    Line::from(vec![
                        label("job"),
                        Span::styled(key.clone(), theme::bold()),
                        Span::styled(
                            format!("  runs-on {}", job.runs_on().join(", ")),
                            theme::dim(),
                        ),
                    ]),
                    Line::from(vec![
                        label("steps"),
                        Span::raw(job.steps.len().to_string()),
                        Span::styled(format!("   matrix {matrix}"), theme::dim()),
                    ]),
                    Line::from(vec![
                        label("needs"),
                        Span::raw(if needs.is_empty() {
                            "none".into()
                        } else {
                            needs.join(", ")
                        }),
                    ]),
                    Line::styled("Enter: run just this job", Style::new().fg(ACCENT)),
                ]
            }
            Some(Row::Broken(i)) => {
                let err = self.workflows[*i]
                    .parsed
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_default();
                vec![Line::styled(err, Style::new().fg(BAD))]
            }
            None => vec![Line::styled(
                format!(
                    "Searched {} automatically, plus any locations added in settings.",
                    workflow::AUTO_DIRS.join(" and ")
                ),
                theme::dim(),
            )],
        };
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{demo_app, docker_ready, press, render, wait_for_run};
    use super::super::{Screen, Start};
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn catalog_lists_and_runs_a_job() {
        let (mut app, store_dir) = demo_app(Start::Workflows);
        if !docker_ready(&app) {
            return;
        }
        press(&mut app, KeyCode::Enter); // Catalog
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("Catalog"), "{screen}");
        assert!(screen.contains("Demo"), "{screen}");
        assert!(screen.contains("×2"), "matrix size is shown");

        press(&mut app, KeyCode::Down); // first job: build
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.stack.last(), Some(Screen::Run(_))));
        wait_for_run(&mut app);
        let screen = render(&mut app, 110, 26);
        assert!(screen.contains("succeeded"), "{screen}");
        assert!(screen.contains("Compute version"), "{screen}");

        let saved = std::fs::read_dir(store_dir.path()).unwrap().count();
        assert_eq!(saved, 2, "the run and its logs were saved");
        press(&mut app, KeyCode::Backspace);
        assert!(matches!(app.stack.last(), Some(Screen::Catalog(_))));
    }
}
