//! The catalog: every workflow found for this project, its jobs and their steps, with the
//! selection's details beside it. Enter folds a workflow or job; `r` runs the selection.

use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{display_path, first_line, App, Nav, EVENTS};
use crate::tui::theme::{self, ACCENT, BAD, WARN};
use crate::workflow::model::{scalar, Job, Step};
use crate::workflow::{self, Source, WorkflowFile};

enum Row {
    Workflow(usize),
    Job(usize, String),
    Step(usize, String, usize),
    Broken(usize),
}

impl Row {
    fn file(&self) -> usize {
        match self {
            Row::Workflow(i) | Row::Job(i, _) | Row::Step(i, _, _) | Row::Broken(i) => *i,
        }
    }
}

/// A foldable row: a workflow (`None`) or one of its jobs, keyed by path so folds survive a rescan.
type Fold = (PathBuf, Option<String>);

pub struct Catalog {
    rows: Vec<Row>,
    sel: usize,
    folded: HashSet<Fold>,
}

impl Catalog {
    pub fn new(workflows: &[WorkflowFile]) -> Self {
        let mut c = Self {
            rows: Vec::new(),
            sel: 0,
            folded: HashSet::new(),
        };
        c.rebuild(workflows);
        c
    }

    /// Recomputes the rows after the workflow list or a fold changed.
    pub fn rebuild(&mut self, workflows: &[WorkflowFile]) {
        self.rows.clear();
        for (i, f) in workflows.iter().enumerate() {
            let Some(wf) = f.workflow() else {
                self.rows.push(Row::Broken(i));
                continue;
            };
            self.rows.push(Row::Workflow(i));
            if self.folded.contains(&(f.path.clone(), None)) {
                continue;
            }
            for (key, job) in &wf.jobs {
                self.rows.push(Row::Job(i, key.clone()));
                if !self.folded.contains(&(f.path.clone(), Some(key.clone()))) {
                    let steps = (0..job.steps.len()).map(|s| Row::Step(i, key.clone(), s));
                    self.rows.extend(steps);
                }
            }
        }
        self.sel = self.sel.min(self.rows.len().saturating_sub(1));
    }

    /// The selected row's fold: a workflow, or a job that has steps.
    fn fold(&self, workflows: &[WorkflowFile]) -> Option<Fold> {
        match self.rows.get(self.sel)? {
            Row::Workflow(i) => Some((workflows[*i].path.clone(), None)),
            Row::Job(i, key) => workflows[*i]
                .workflow()
                .is_some_and(|w| !w.jobs[key].steps.is_empty())
                .then(|| (workflows[*i].path.clone(), Some(key.clone()))),
            _ => None,
        }
    }

    /// Folds or unfolds the selected row, which keeps its place since only rows below it change.
    fn set_folded(&mut self, workflows: &[WorkflowFile], fold: Fold, folded: bool) {
        if folded {
            self.folded.insert(fold);
        } else {
            self.folded.remove(&fold);
        }
        self.rebuild(workflows);
    }

    fn all_folded(&self, workflows: &[WorkflowFile]) -> bool {
        workflows
            .iter()
            .filter(|f| f.workflow().is_some())
            .all(|f| self.folded.contains(&(f.path.clone(), None)))
    }

    /// Folds every workflow, or unfolds everything if they all are already.
    fn fold_all(&mut self, workflows: &[WorkflowFile]) {
        if self.all_folded(workflows) {
            self.folded.clear();
        } else {
            self.folded.extend(
                workflows
                    .iter()
                    .filter(|f| f.workflow().is_some())
                    .map(|f| (f.path.clone(), None)),
            );
        }
        let file = self.rows.get(self.sel).map(Row::file);
        self.rebuild(workflows);
        self.sel = file
            .and_then(|i| self.rows.iter().position(|r| r.file() == i))
            .unwrap_or(0);
    }

    /// Moves the selection from a step to its job, or from a job to its workflow.
    fn select_parent(&mut self) {
        let before = &self.rows[..self.sel];
        let parent = match self.rows.get(self.sel) {
            Some(Row::Step(..)) => before.iter().rposition(|r| matches!(r, Row::Job(..))),
            Some(Row::Job(..)) => before.iter().rposition(|r| matches!(r, Row::Workflow(_))),
            _ => None,
        };
        if let Some(p) = parent {
            self.sel = p;
        }
    }
}

impl App {
    pub(super) fn catalog_key(&mut self, c: &mut Catalog, key: KeyEvent) -> Nav {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => c.sel = c.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                c.sel = (c.sel + 1).min(c.rows.len().saturating_sub(1))
            }
            KeyCode::Backspace | KeyCode::Esc => return Nav::Back,
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(fold) = c.fold(&self.workflows) {
                    let folded = !c.folded.contains(&fold);
                    c.set_folded(&self.workflows, fold, folded);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(fold) = c.fold(&self.workflows) {
                    c.set_folded(&self.workflows, fold, false);
                }
            }
            KeyCode::Left | KeyCode::Char('h') => match c.fold(&self.workflows) {
                Some(fold) if !c.folded.contains(&fold) => {
                    c.set_folded(&self.workflows, fold, true)
                }
                _ => c.select_parent(),
            },
            KeyCode::Char('c') => c.fold_all(&self.workflows),
            KeyCode::Char('e') => {
                let i = EVENTS
                    .iter()
                    .position(|e| *e == self.event)
                    .map_or(0, |i| i + 1);
                self.event = EVENTS[i % EVENTS.len()].to_string();
            }
            KeyCode::F(5) => {
                self.reload_workflows();
                c.rebuild(&self.workflows);
                self.say("Rescanned workflows", false);
            }
            KeyCode::Char('r') => {
                let (file, job) = match c.rows.get(c.sel) {
                    Some(Row::Workflow(i)) => (*i, None),
                    Some(Row::Job(i, key) | Row::Step(i, key, _)) => (*i, Some(key.clone())),
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
        let [list, details] = Layout::horizontal([Constraint::Fill(1); 2]).areas(area);
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
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
        } else {
            let width = list.width.saturating_sub(2) as usize;
            let rows = c
                .rows
                .iter()
                .map(|row| self.catalog_row(c, row, width))
                .collect();
            theme::render_list(f, list, block, rows, Some(c.sel), true);
        }
        self.draw_catalog_details(c, f, details);

        let mut hints = vec![("↑↓", "move")];
        match c.fold(&self.workflows) {
            Some(fold) if c.folded.contains(&fold) => hints.push(("enter", "expand")),
            Some(_) => hints.push(("enter", "collapse")),
            None => {}
        }
        hints.push(if c.all_folded(&self.workflows) {
            ("c", "expand all")
        } else {
            ("c", "collapse all")
        });
        hints.extend([
            ("r", "run"),
            ("e", "event"),
            ("f5", "rescan"),
            ("esc", "back"),
            ("q", "quit"),
        ]);
        hints
    }

    fn catalog_row(&self, c: &Catalog, row: &Row, width: usize) -> Line<'static> {
        let fold_icon = |fold: Fold| if c.folded.contains(&fold) { "▸" } else { "▾" };
        match row {
            Row::Workflow(i) => {
                let file = &self.workflows[*i];
                let triggered = file.events().contains(&self.event);
                let icon = Span::styled(
                    fold_icon((file.path.clone(), None)),
                    Style::new().fg(if triggered { ACCENT } else { theme::DIM }),
                );
                let right = match file.source {
                    Source::Auto => file.file_name.clone(),
                    other => format!("{} · {}", other.label(), file.file_name),
                };
                theme::row(icon, &file.display_name(), right, width, theme::bold())
            }
            Row::Job(i, key) => {
                let file = &self.workflows[*i];
                let job = &file.workflow().expect("parsed").jobs[key];
                let icon = if job.steps.is_empty() {
                    "·"
                } else {
                    fold_icon((file.path.clone(), Some(key.clone())))
                };
                let matrix = workflow::plan::expand_matrix(job)
                    .map(|c| c.len())
                    .unwrap_or(1);
                let mut right = job.runs_on().join(",");
                if matrix > 1 {
                    right = format!("×{matrix}  {right}");
                }
                theme::row(
                    Span::styled(format!("  {icon}"), theme::dim()),
                    &job_name(key, job),
                    right,
                    width,
                    Style::new(),
                )
            }
            Row::Step(i, key, s) => {
                let job = &self.workflows[*i].workflow().expect("parsed").jobs[key];
                let step = &job.steps[*s];
                theme::row(
                    Span::styled("    ·", theme::dim()),
                    &step_name(step),
                    String::new(),
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
        let field = |k: &str, v: String| {
            Line::from(vec![
                Span::styled(format!("{k:<10}"), theme::dim()),
                Span::raw(v),
            ])
        };
        let action = |s: String| Line::styled(s, Style::new().fg(ACCENT));
        let lines: Vec<Line> = match c.rows.get(c.sel) {
            Some(Row::Workflow(i)) => {
                let file = &self.workflows[*i];
                let wf = file.workflow().expect("parsed");
                let events = file.events();
                let jobs: Vec<&str> = wf.jobs.keys().map(String::as_str).collect();
                let mut lines = vec![
                    Line::styled(file.display_name(), theme::bold()),
                    Line::styled(
                        format!(
                            "{}  ({})",
                            display_path(&file.path, &self.root),
                            file.source.label()
                        ),
                        theme::dim(),
                    ),
                    Line::raw(""),
                    field("on", events.join(", ")),
                ];
                if !events.contains(&self.event) {
                    lines.push(Line::styled(
                        format!("{:10}not normally triggered by {}", "", self.event),
                        Style::new().fg(WARN),
                    ));
                }
                lines.extend([
                    field("jobs", jobs.join(", ")),
                    Line::raw(""),
                    action(format!("r: run all jobs as `{}`", self.event)),
                ]);
                lines
            }
            Some(Row::Job(i, key)) => {
                let file = &self.workflows[*i];
                let job = &file.workflow().expect("parsed").jobs[key];
                let needs = job.needs();
                let matrix = workflow::plan::expand_matrix(job)
                    .map(|c| c.len().to_string())
                    .unwrap_or_else(|e| e);
                let needs = if needs.is_empty() {
                    "none".into()
                } else {
                    needs.join(", ")
                };
                let place = format!("job {key} in {}", file.display_name());
                let mut lines = vec![
                    Line::styled(job_name(key, job), theme::bold()),
                    Line::styled(place, theme::dim()),
                    Line::raw(""),
                    field("runs-on", job.runs_on().join(", ")),
                    field("needs", needs),
                    field("matrix", matrix),
                ];
                if let Some(cond) = &job.condition {
                    lines.push(field("if", scalar(cond)));
                }
                if let Some(container) = &job.container {
                    lines.push(field("container", container.spec().image));
                }
                if let Some(uses) = &job.uses {
                    lines.push(field("uses", uses.clone()));
                }
                lines.extend([
                    field("steps", job.steps.len().to_string()),
                    Line::raw(""),
                    action(format!("r: run just this job as `{}`", self.event)),
                ]);
                lines
            }
            Some(Row::Step(i, key, s)) => {
                let file = &self.workflows[*i];
                let job = &file.workflow().expect("parsed").jobs[key];
                let step = &job.steps[*s];
                let place = format!("step {} of {}, {key}", s + 1, job.steps.len());
                let timeout = step
                    .timeout_minutes
                    .as_ref()
                    .map(|t| format!("{} min", scalar(t)));
                let mut lines = vec![
                    Line::styled(step_name(step), theme::bold()),
                    Line::styled(place, theme::dim()),
                    Line::raw(""),
                ];
                let optional = [
                    ("id", step.id.clone()),
                    ("uses", step.uses.clone()),
                    ("if", step.condition.as_ref().map(scalar)),
                    ("shell", step.shell.clone()),
                    ("directory", step.working_directory.clone()),
                    ("may fail", step.continue_on_error.as_ref().map(scalar)),
                    ("timeout", timeout),
                ];
                lines.extend(
                    optional
                        .into_iter()
                        .filter_map(|(k, v)| v.map(|v| field(k, v))),
                );
                for (n, (k, v)) in step.with.iter().enumerate() {
                    let label = if n == 0 { "with" } else { "" };
                    lines.push(field(label, format!("{k}: {}", scalar(v))));
                }
                lines.extend([
                    Line::raw(""),
                    action(format!("r: run its job, {key}, as `{}`", self.event)),
                ]);
                if let Some(run) = &step.run {
                    lines.push(Line::raw(""));
                    lines.push(Line::styled("run", theme::dim()));
                    lines.extend(run.trim_end().lines().map(|l| Line::raw(format!("  {l}"))));
                }
                lines
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

/// A job's `name:`, unless it depends on the matrix, else its id.
fn job_name(key: &str, job: &Job) -> String {
    job.name
        .clone()
        .filter(|n| !n.contains("${{"))
        .unwrap_or_else(|| key.to_string())
}

/// A step's `name:`, else what it runs.
fn step_name(step: &Step) -> String {
    match (&step.name, &step.uses, &step.run) {
        (Some(name), _, _) => name.clone(),
        (_, Some(uses), _) => format!("Run {uses}"),
        (_, _, Some(run)) => format!("Run {}", first_line(run.trim_start())),
        _ => "Step".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{demo_app, docker_ready, press, render, wait_for_run};
    use super::super::{Screen, Start};
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn catalog_shows_steps_and_enter_folds_without_running() {
        let (mut app, _dir) = demo_app(Start::Workflows);
        press(&mut app, KeyCode::Enter); // Catalog
        let screen = render(&mut app, 140, 30);
        assert!(screen.contains("Details"), "{screen}");
        assert!(screen.contains("Compute version"), "{screen}"); // steps show right away

        press(&mut app, KeyCode::Down); // build
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.stack.last(), Some(Screen::Catalog(_)))); // nothing ran
        let screen = render(&mut app, 140, 30);
        assert!(!screen.contains("Compute version"), "{screen}"); // build is collapsed
        assert!(screen.contains("Matrix"), "{screen}"); // test stays open

        press(&mut app, KeyCode::Char('c')); // collapse all
        let screen = render(&mut app, 140, 30);
        assert!(!screen.contains("Matrix"), "{screen}");
        press(&mut app, KeyCode::Char('c')); // expand all
        let screen = render(&mut app, 140, 30);
        assert!(screen.contains("Compute version"), "{screen}");
    }

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
        press(&mut app, KeyCode::Char('r'));
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
