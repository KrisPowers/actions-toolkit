//! A run: jobs, steps of the selected job, and its logs. Live runs save themselves to the
//! history when they end; saved runs can be reopened read-only.

use std::path::PathBuf;
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
use crate::history::{self, JobRecord, LogLine, Outcome, RunRecord, StepRecord, Store};
use crate::runner::{self, LogKind, RunEvent, RunOptions, Status};
use crate::util::{fmt_duration, Cancel};
use crate::workflow::plan::RunPlan;

pub enum RunAction {
    None,
    Close,
    /// Run the same workflow/job again.
    Rerun,
    /// The user asked to stop a run that is still going; confirm first.
    ConfirmCancel,
}

/// Where a live run came from, so it can be saved to the history and re-run.
pub struct RunMeta {
    pub project: PathBuf,
    pub workflow: PathBuf,
    pub job: Option<String>,
    pub store: Option<Store>,
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
    pub event: String,
    pub workflow: PathBuf,
    pub job: Option<String>,
    project: PathBuf,
    store: Option<Store>,
    id: String,
    /// Unix time the run started.
    started_at: u64,
    /// Set for runs reopened from the history.
    replay: bool,
    saved: bool,
    save_error: Option<String>,
    jobs: Vec<JobView>,
    rx: Option<Receiver<RunEvent>>,
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

fn elapsed(started: Option<Instant>, took: Option<Duration>) -> String {
    match (took, started) {
        (Some(d), _) => fmt_duration(d),
        (None, Some(s)) => fmt_duration(s.elapsed()),
        _ => String::new(),
    }
}

impl RunView {
    pub fn start(
        plan: Arc<RunPlan>,
        engine: Arc<dyn Engine>,
        opts: Arc<RunOptions>,
        meta: RunMeta,
    ) -> Self {
        let jobs = plan
            .jobs
            .iter()
            .map(|j| JobView {
                name: j.label.clone(),
                status: Status::Pending,
                started: None,
                took: None,
                steps: j
                    .step_names
                    .iter()
                    .map(|s| StepView {
                        name: s.clone(),
                        status: Status::Pending,
                        started: None,
                        took: None,
                    })
                    .collect(),
                logs: Vec::new(),
            })
            .collect();
        let cancel = Cancel::new();
        let title = plan.title.clone();
        let engine_name = engine.kind().name().to_string();
        let event = opts.event.clone();
        let (rx, handle) = runner::spawn(plan, engine, opts, cancel.clone());
        Self {
            title,
            engine: engine_name,
            event,
            workflow: meta.workflow,
            job: meta.job,
            project: meta.project,
            store: meta.store,
            id: history::new_id(),
            started_at: history::now(),
            replay: false,
            saved: false,
            save_error: None,
            jobs,
            rx: Some(rx),
            cancel,
            handle: Some(handle),
            started: Instant::now(),
            finished: None,
            job_sel: 0,
            auto_job: true,
            step_cursor: 0,
            step_filter: None,
            focus: Focus::Jobs,
            scroll: 0,
            follow: true,
            log_height: 10,
            tick: 0,
        }
    }

    /// Reopens a saved run, read-only.
    pub fn replay(record: &RunRecord, logs: Vec<LogLine>) -> Self {
        let took = |ms: Option<u64>| ms.map(Duration::from_millis);
        let mut jobs: Vec<JobView> = record
            .jobs
            .iter()
            .map(|j| JobView {
                name: j.name.clone(),
                status: j.status,
                started: None,
                took: took(j.millis),
                steps: j
                    .steps
                    .iter()
                    .map(|s| StepView {
                        name: s.name.clone(),
                        status: s.status,
                        started: None,
                        took: took(s.millis),
                    })
                    .collect(),
                logs: Vec::new(),
            })
            .collect();
        for line in logs {
            if let Some(job) = jobs.get_mut(line.job) {
                job.logs.push((line.step, line.kind, line.text));
            }
        }
        if jobs.is_empty() {
            jobs.push(JobView {
                name: record.title.clone(),
                status: Status::Skipped,
                started: None,
                took: None,
                steps: Vec::new(),
                logs: Vec::new(),
            });
        }
        let cancel = Cancel::new();
        if record.outcome == Outcome::Cancelled {
            cancel.cancel();
        }
        Self {
            title: record.title.clone(),
            engine: record.engine.clone(),
            event: record.event.clone(),
            workflow: record.workflow.clone(),
            job: record.job.clone(),
            project: record.project.clone(),
            store: None,
            id: record.id.clone(),
            started_at: record.started,
            replay: true,
            saved: true,
            save_error: None,
            jobs,
            rx: None,
            cancel,
            handle: None,
            started: Instant::now(),
            finished: Some((
                record.outcome == Outcome::Success,
                Duration::from_millis(record.millis),
            )),
            job_sel: 0,
            auto_job: false,
            step_cursor: 0,
            step_filter: None,
            focus: Focus::Jobs,
            scroll: 0,
            follow: true,
            log_height: 10,
            tick: 0,
        }
    }

    /// Writes the run to the history (once).
    fn save(&mut self) {
        let Some(store) = self.store.clone() else {
            return;
        };
        if self.saved {
            return;
        }
        self.saved = true;
        let (success, took) = self.finished.unwrap_or((false, self.started.elapsed()));
        let outcome = if success {
            Outcome::Success
        } else if self.cancel.is_cancelled() {
            Outcome::Cancelled
        } else {
            Outcome::Failure
        };
        // Anything still pending or running when the run stopped counts as cancelled.
        let settle = |s: Status| if s.is_done() { s } else { Status::Cancelled };
        let millis = |started: Option<Instant>, took: Option<Duration>| {
            took.or(started.map(|s| s.elapsed()))
                .map(|d| d.as_millis() as u64)
        };
        let record = RunRecord {
            id: self.id.clone(),
            project: self.project.clone(),
            title: self.title.clone(),
            workflow: self.workflow.clone(),
            job: self.job.clone(),
            engine: self.engine.clone(),
            event: self.event.clone(),
            started: self.started_at,
            millis: took.as_millis() as u64,
            outcome,
            jobs: self
                .jobs
                .iter()
                .map(|j| JobRecord {
                    name: j.name.clone(),
                    status: settle(j.status),
                    millis: millis(j.started, j.took),
                    steps: j
                        .steps
                        .iter()
                        .map(|s| StepRecord {
                            name: s.name.clone(),
                            status: settle(s.status),
                            millis: millis(s.started, s.took),
                        })
                        .collect(),
                })
                .collect(),
        };
        let logs: Vec<LogLine> = self
            .jobs
            .iter()
            .enumerate()
            .flat_map(|(job, j)| {
                j.logs.iter().map(move |(step, kind, text)| LogLine {
                    job,
                    step: *step,
                    kind: *kind,
                    text: text.clone(),
                })
            })
            .collect();
        if let Err(e) = store.save(&record, &logs) {
            self.save_error = Some(format!("Could not save this run to the history: {e:#}"));
        }
    }

    /// Cancels a run that is still going and saves it as cancelled, for when maverick exits.
    pub fn abandon(&mut self) {
        if self.finished.is_none() {
            self.cancel();
            self.finished = Some((false, self.started.elapsed()));
        }
        self.save();
    }

    pub fn is_finished(&self) -> bool {
        self.finished.is_some()
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Drains pending runner events. Call once per frame.
    pub fn update(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let Some(rx) = &self.rx else {
            return;
        };
        let events: Vec<RunEvent> = rx.try_iter().collect();
        let mut finished = false;
        for event in events {
            match event {
                RunEvent::Job { job, status } => {
                    let j = &mut self.jobs[job];
                    j.status = status;
                    if status == Status::Running {
                        j.started = Some(Instant::now());
                        if self.auto_job {
                            self.select_job(job);
                        }
                    } else if status.is_done() {
                        j.took = j.started.map(|s| s.elapsed());
                    }
                }
                RunEvent::Step { job, step, status } => {
                    let s = &mut self.jobs[job].steps[step];
                    if status == Status::Running {
                        s.started = Some(Instant::now());
                        if job == self.job_sel
                            && self.focus != Focus::Steps
                            && self.step_filter.is_none()
                        {
                            self.step_cursor = step;
                        }
                    } else if status.is_done() {
                        s.took = s.started.map(|t| t.elapsed());
                    }
                    s.status = status;
                }
                RunEvent::Log {
                    job,
                    step,
                    kind,
                    text,
                } => self.jobs[job].logs.push((step, kind, text)),
                RunEvent::Finished { success } => {
                    if let Some(h) = self.handle.take() {
                        let _ = h.join();
                    }
                    self.finished = Some((success, self.started.elapsed()));
                    finished = true;
                }
            }
        }
        if finished {
            self.save();
        }
    }

    fn select_job(&mut self, job: usize) {
        if job != self.job_sel {
            self.job_sel = job;
            self.step_filter = None;
            self.follow = true;
            self.scroll = 0;
            self.step_cursor = self.jobs[job]
                .steps
                .iter()
                .position(|s| s.status == Status::Running)
                .unwrap_or(0);
        }
    }

    fn scroll_up(&mut self, n: usize) {
        self.follow = false;
        self.scroll = self.scroll.saturating_add(n);
    }

    fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
        if self.scroll == 0 {
            self.follow = true;
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> RunAction {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.log_height.max(1);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace => {
                if self.is_finished() {
                    return RunAction::Close;
                }
                if !self.cancel.is_cancelled() {
                    return RunAction::ConfirmCancel;
                }
            }
            KeyCode::Char('c') if !ctrl && !self.is_finished() && !self.cancel.is_cancelled() => {
                return RunAction::ConfirmCancel;
            }
            KeyCode::Char('r') if self.is_finished() => return RunAction::Rerun,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.focus = match self.focus {
                    Focus::Jobs => Focus::Steps,
                    Focus::Steps => Focus::Logs,
                    Focus::Logs => Focus::Jobs,
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.focus = match self.focus {
                    Focus::Jobs => Focus::Logs,
                    Focus::Steps => Focus::Jobs,
                    Focus::Logs => Focus::Steps,
                }
            }
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Jobs => {
                    self.auto_job = false;
                    self.select_job(self.job_sel.saturating_sub(1));
                }
                Focus::Steps => self.step_cursor = self.step_cursor.saturating_sub(1),
                Focus::Logs => self.scroll_up(1),
            },
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Jobs => {
                    self.auto_job = false;
                    self.select_job((self.job_sel + 1).min(self.jobs.len() - 1));
                }
                Focus::Steps => {
                    let max = self.jobs[self.job_sel].steps.len().saturating_sub(1);
                    self.step_cursor = (self.step_cursor + 1).min(max);
                }
                Focus::Logs => self.scroll_down(1),
            },
            KeyCode::Enter => match self.focus {
                Focus::Jobs => self.focus = Focus::Steps,
                Focus::Steps => {
                    self.step_filter = if self.step_filter == Some(self.step_cursor) {
                        None
                    } else {
                        Some(self.step_cursor)
                    };
                    self.follow = true;
                    self.scroll = 0;
                }
                Focus::Logs => {}
            },
            KeyCode::Char('a') => {
                self.step_filter = None;
                self.follow = true;
                self.scroll = 0;
            }
            KeyCode::PageUp => self.scroll_up(page),
            KeyCode::PageDown => self.scroll_down(page),
            KeyCode::Home | KeyCode::Char('g') => {
                self.follow = false;
                self.scroll = usize::MAX;
            }
            KeyCode::End | KeyCode::Char('G') | KeyCode::Char('f') => {
                self.follow = true;
                self.scroll = 0;
            }
            _ => {}
        }
        RunAction::None
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);
        self.draw_header(f, header);

        let longest = self
            .jobs
            .iter()
            .map(|j| j.name.chars().count())
            .chain(
                self.jobs[self.job_sel]
                    .steps
                    .iter()
                    .map(|s| s.name.chars().count()),
            )
            .max()
            .unwrap_or(10);
        let side_w = (longest as u16 + 14).clamp(30, 56).min(body.width / 2);
        let [side, logs] =
            Layout::horizontal([Constraint::Length(side_w), Constraint::Min(0)]).areas(body);
        let jobs_h = (self.jobs.len() as u16 + 2).clamp(3, side.height / 2);
        let [jobs_area, steps_area] =
            Layout::vertical([Constraint::Length(jobs_h), Constraint::Min(0)]).areas(side);

        let tick = self.tick / 2;
        let width = jobs_area.width.saturating_sub(2) as usize;
        let job_rows = self
            .jobs
            .iter()
            .map(|j| {
                theme::row(
                    theme::status_icon(j.status, tick),
                    &j.name,
                    elapsed(j.started, j.took),
                    width,
                    Style::new(),
                )
            })
            .collect();
        let jobs_block = theme::panel(
            &format!("Jobs {}", self.jobs.len()),
            self.focus == Focus::Jobs,
        );
        theme::render_list(
            f,
            jobs_area,
            jobs_block,
            job_rows,
            Some(self.job_sel),
            self.focus == Focus::Jobs,
        );

        let job = &self.jobs[self.job_sel];
        let width = steps_area.width.saturating_sub(2) as usize;
        let step_rows = job
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if Some(i) == self.step_filter {
                    Style::new().fg(ACCENT)
                } else {
                    Style::new()
                };
                theme::row(
                    theme::status_icon(s.status, tick),
                    &s.name,
                    elapsed(s.started, s.took),
                    width,
                    style,
                )
            })
            .collect();
        let steps_block = theme::panel("Steps", self.focus == Focus::Steps);
        theme::render_list(
            f,
            steps_area,
            steps_block,
            step_rows,
            Some(self.step_cursor),
            self.focus == Focus::Steps,
        );

        self.draw_logs(f, logs);
        f.render_widget(Paragraph::new(self.footer()), footer);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let left = Line::from(vec![
            Span::styled(
                " ◆ maverick ",
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("› ", theme::dim()),
            Span::styled(self.title.clone(), theme::bold()),
            Span::styled(format!("   {} · {}", self.engine, self.event), theme::dim()),
        ]);
        let when = if self.replay {
            format!("· {} ", history::ago(self.started_at))
        } else {
            String::new()
        };
        let right = match self.finished {
            None if self.cancel.is_cancelled() => Line::styled(
                format!("{} cancelling… ", theme::spinner(self.tick / 2)),
                Style::new().fg(WARN),
            ),
            None => Line::styled(
                format!(
                    "{} running {} ",
                    theme::spinner(self.tick / 2),
                    fmt_duration(self.started.elapsed())
                ),
                Style::new().fg(WARN),
            ),
            Some((true, d)) => Line::styled(
                format!("✓ succeeded in {} {when}", fmt_duration(d)),
                Style::new().fg(OK).add_modifier(Modifier::BOLD),
            ),
            Some((false, d)) if self.cancel.is_cancelled() => Line::styled(
                format!("■ cancelled after {} {when}", fmt_duration(d)),
                Style::new().fg(ratatui::style::Color::Magenta),
            ),
            Some((false, d)) => Line::styled(
                format!("✗ failed after {} {when}", fmt_duration(d)),
                Style::new().fg(BAD).add_modifier(Modifier::BOLD),
            ),
        };
        f.render_widget(Paragraph::new(left), area);
        f.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
    }

    fn draw_logs(&mut self, f: &mut Frame, area: Rect) {
        let job = &self.jobs[self.job_sel];
        let scope = match self.step_filter {
            Some(s) => format!("{} › {}", job.name, job.steps[s].name),
            None => job.name.clone(),
        };
        let mut title = format!("Logs · {scope}");
        if !self.follow {
            title.push_str(" · paused (f to follow)");
        }
        let block = theme::panel(&title, self.focus == Focus::Logs);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let height = inner.height as usize;
        self.log_height = height;

        let filtered: Vec<&(usize, LogKind, String)> = match self.step_filter {
            Some(s) => job.logs.iter().filter(|(step, _, _)| *step == s).collect(),
            None => job.logs.iter().collect(),
        };
        let max_scroll = filtered.len().saturating_sub(height);
        if self.follow {
            self.scroll = 0;
        }
        self.scroll = self.scroll.min(max_scroll);
        let end = filtered.len() - self.scroll;
        let start = end.saturating_sub(height);

        let lines: Vec<Line> = if filtered.is_empty() {
            let msg = match job.status {
                Status::Pending => "Waiting for dependencies…",
                Status::Skipped => "This job was skipped.",
                Status::Cancelled => "This job was cancelled.",
                _ => "No output yet.",
            };
            vec![Line::styled(format!(" {msg}"), theme::dim())]
        } else {
            filtered[start..end]
                .iter()
                .map(|(_, kind, text)| log_line(*kind, text))
                .collect()
        };
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn footer(&self) -> Line<'static> {
        if let Some(err) = &self.save_error {
            return Line::styled(format!(" {err}"), Style::new().fg(BAD));
        }
        let mut pairs = vec![("↑↓", "select"), ("tab", "panel")];
        if self.focus == Focus::Steps {
            pairs.push(("enter", "filter logs"));
        }
        pairs.extend([("a", "all logs"), ("pgup/pgdn", "scroll"), ("f", "follow")]);
        if self.is_finished() {
            pairs.extend([("r", "run again"), ("⌫", "back")]);
        } else {
            pairs.push(("c", "cancel"));
        }
        theme::hints(&pairs)
    }
}

fn log_line(kind: LogKind, text: &str) -> Line<'static> {
    let text = text.replace('\t', "    ");
    match kind {
        LogKind::Output => Line::raw(format!(" {text}")),
        LogKind::Command => Line::from(vec![
            Span::styled(" │ ", Style::new().fg(ACCENT)),
            Span::styled(text, theme::dim()),
        ]),
        LogKind::System => Line::styled(
            format!(" {text}"),
            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
        ),
        LogKind::Header => Line::styled(
            format!(" ━━ {text}"),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        LogKind::Group => Line::styled(format!(" ▸ {text}"), theme::bold()),
        LogKind::Notice => Line::styled(
            format!(" ℹ {text}"),
            Style::new().fg(ratatui::style::Color::Blue),
        ),
        LogKind::Warning => Line::styled(format!(" ⚠ {text}"), Style::new().fg(WARN)),
        LogKind::Error => Line::styled(
            format!(" ✗ {text}"),
            Style::new().fg(BAD).add_modifier(Modifier::BOLD),
        ),
    }
}
