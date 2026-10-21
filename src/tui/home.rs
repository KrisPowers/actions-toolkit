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

impl App {
    pub fn new(root: PathBuf) -> Result<Self> {
        let cfg = Config::load(&root)?;
        let active = cfg.engine.as_deref().and_then(Kind::parse);
        let mut app = Self {
            root,
            cfg,
            workflows: Vec::new(),
            rows: Vec::new(),
            row_sel: 0,
            engines: Vec::new(),
            engine_sel: 0,
            detect_rx: None,
            active,
            focus: Focus::Workflows,
            event_idx: 0,
            flash: None,
            run: None,
            tick: 0,
            quit: false,
        };
        app.reload();
        Ok(app)
    }

    /// Rescans workflows and kicks off engine detection in the background.
    fn reload(&mut self) {
        self.workflows = workflow::discover(&self.root);
        self.rows = self
            .workflows
            .iter()
            .enumerate()
            .flat_map(|(i, f)| match f.workflow() {
                Some(wf) => std::iter::once(Row::Workflow(i))
                    .chain(wf.jobs.keys().map(move |k| Row::Job(i, k.clone())))
                    .collect::<Vec<_>>(),
                None => vec![Row::Broken(i)],
            })
            .collect();
        self.row_sel = self.row_sel.min(self.rows.len().saturating_sub(1));
        let (tx, rx) = mpsc::channel();
        let cfg = self.cfg.clone();
        thread::spawn(move || {
            let _ = tx.send(engine::detect_all(&cfg));
        });
        self.detect_rx = Some(rx);
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            self.update();
            terminal.draw(|f| self.draw(f))?;
            if event::poll(FRAME)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.on_key(key);
                    }
                }
            }
        }
        if let Some(run) = &self.run {
            run.cancel();
        }
        Ok(())
    }

    fn update(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(run) = &mut self.run {
            run.update();
        }
        if let Some(rx) = &self.detect_rx {
            if let Ok(mut infos) = rx.try_recv() {
                infos.sort_by_key(|i| {
                    (
                        availability_order(&i.availability),
                        family_order(i.kind.family()),
                    )
                });
                self.engines = infos;
                self.detect_rx = None;
                if self.active.is_none() {
                    self.active = [Kind::Docker, Kind::Podman, Kind::Native]
                        .into_iter()
                        .find(|k| self.engines.iter().any(|i| i.kind == *k && i.is_ready()));
                }
                if let Some(pos) = self
                    .engines
                    .iter()
                    .position(|i| Some(i.kind) == self.active)
                {
                    self.engine_sel = pos;
                }
            }
        }
        if self
            .flash
            .as_ref()
            .is_some_and(|(_, _, t)| t.elapsed() > Duration::from_secs(6))
        {
            self.flash = None;
        }
    }

    fn say(&mut self, msg: impl Into<String>, error: bool) {
        self.flash = Some((msg.into(), error, Instant::now()));
    }

    fn event(&self) -> &'static str {
        EVENTS[self.event_idx % EVENTS.len()]
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(run) = &mut self.run {
            if ctrl_c && !run.is_finished() {
                run.cancel();
            } else if let RunAction::Close = run.on_key(key) {
                self.run = None;
            }
            return;
        }
        match key.code {
            _ if ctrl_c => self.quit = true,
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h')
            | KeyCode::Char('l') => {
                self.focus = match self.focus {
                    Focus::Workflows => Focus::Engines,
                    Focus::Engines => Focus::Workflows,
                }
            }
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Workflows => self.row_sel = self.row_sel.saturating_sub(1),
                Focus::Engines => self.engine_sel = self.engine_sel.saturating_sub(1),
            },
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Workflows => {
                    self.row_sel = (self.row_sel + 1).min(self.rows.len().saturating_sub(1))
                }
                Focus::Engines => {
                    self.engine_sel =
                        (self.engine_sel + 1).min(self.engines.len().saturating_sub(1))
                }
            },
            KeyCode::Char('e') => self.event_idx = (self.event_idx + 1) % EVENTS.len(),
            KeyCode::Char('r') => {
                self.reload();
                self.say("Rescanned workflows and engines", false);
            }
            KeyCode::Enter => match self.focus {
                Focus::Workflows => self.start_selected(),
                Focus::Engines => self.choose_engine(),
            },
            _ => {}
        }
    }

    fn choose_engine(&mut self) {
        let Some(info) = self.engines.get(self.engine_sel).cloned() else {
            return;
        };
        if !info.is_ready() {
            let hint = info
                .note
                .clone()
                .unwrap_or_else(|| format!("{} is {}", info.kind.name(), info.status_label()));
            self.say(format!("{}: {hint}", info.kind.name()), true);
            return;
        }
        self.active = Some(info.kind);
        let saved = Config::load_user().and_then(|mut user| {
            user.engine = Some(info.kind.id().to_string());
            user.save_user()
        });
        match saved {
            Ok(_) => self.say(
                format!("{} is now the default engine", info.kind.name()),
                false,
            ),
            Err(e) => self.say(
                format!("Using {} (could not save config: {e})", info.kind.name()),
                true,
            ),
        }
    }

    fn start_selected(&mut self) {
        let (file_idx, job) = match self.rows.get(self.row_sel) {
            Some(Row::Workflow(i)) => (*i, None),
            Some(Row::Job(i, key)) => (*i, Some(key.clone())),
            Some(Row::Broken(i)) => {
                let err = self.workflows[*i]
                    .parsed
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_default();
                self.say(
                    format!(
                        "{} is invalid: {}",
                        self.workflows[*i].file_name,
                        first_line(&err)
                    ),
                    true,
                );
                return;
            }
            None => return,
        };
        let Some(kind) = self.active else {
            self.say("No engine selected yet; pick one on the right", true);
            return;
        };
        let file = &self.workflows[file_idx];
        let Some(wf) = file.workflow() else { return };
        let plan = match RunPlan::build(&[(wf.clone(), file.display_name())], job.as_deref()) {
            Ok(p) => p,
            Err(e) => return self.say(format!("{e:#}"), true),
        };
        let engine = match engine::build(kind, &self.cfg) {
            Ok(e) => e,
            Err(e) => return self.say(format!("{e:#}"), true),
        };
        let opts = RunOptions::new(&self.root, &self.cfg, self.event());
        self.run = Some(RunView::start(Arc::new(plan), engine, Arc::new(opts)));
    }

    // -----------------------------------------------------------------------------------------
    // Drawing

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        if let Some(run) = &mut self.run {
            run.draw(f, area);
            return;
        }
        let [header, body, details, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .areas(area);
        self.draw_header(f, header);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
                .areas(body);
        self.draw_workflows(f, left);
        self.draw_engines(f, right);
        self.draw_details(f, details);
        self.draw_footer(f, footer);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect) {
        let home = dirs::home_dir();
        let path = match home.as_ref().and_then(|h| self.root.strip_prefix(h).ok()) {
            Some(rel) => format!("~/{}", rel.display()),
            None => self.root.display().to_string(),
        };
        let left = Line::from(vec![
            Span::styled(
                " ◆ maverick ",
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(path, theme::dim()),
        ]);
        let engine = match self.active {
            Some(k) => Span::styled(format!("● {}", k.name()), Style::new().fg(OK)),
            None if self.detect_rx.is_some() => Span::styled(
                format!("{} detecting…", theme::spinner(self.tick / 2)),
                Style::new().fg(WARN),
            ),
            None => Span::styled("○ none", Style::new().fg(BAD)),
        };
        let right = Line::from(vec![
            Span::styled("event ", theme::dim()),
            Span::styled(self.event(), theme::bold()),
            Span::styled("   engine ", theme::dim()),
            engine,
            Span::raw(" "),
        ]);
        f.render_widget(Paragraph::new(left), area);
        f.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
    }

    fn draw_workflows(&self, f: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Workflows;
        let block = theme::panel("Workflows", focused);
        if self.rows.is_empty() {
            let inner = block.inner(area);
            f.render_widget(block, area);
            let text = vec![
                Line::raw(""),
                Line::styled(" No workflows found.", theme::bold()),
                Line::styled(
                    " Add one under .github/workflows/ and press r.",
                    theme::dim(),
                ),
            ];
            f.render_widget(Paragraph::new(text), inner);
            return;
        }
        let width = area.width.saturating_sub(2) as usize;
        let event = self.event();
        let rows = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Workflow(i) => {
                    let file = &self.workflows[*i];
                    let events = file.events();
                    let triggered = events.iter().any(|e| e == event);
                    let icon = Span::styled(
                        "▾",
                        Style::new().fg(if triggered { ACCENT } else { theme::DIM }),
                    );
                    theme::row(
                        icon,
                        &file.display_name(),
                        file.file_name.clone(),
                        width,
                        theme::bold(),
                    )
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
            })
            .collect();
        theme::render_list(f, area, block, rows, Some(self.row_sel), focused);
    }

    fn draw_engines(&self, f: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Engines;
        let block = theme::panel("Engines", focused);
        if self.engines.is_empty() {
            let inner = block.inner(area);
            f.render_widget(block, area);
            let text = Line::styled(
                format!(" {} Detecting engines…", theme::spinner(self.tick / 2)),
                Style::new().fg(WARN),
            );
            f.render_widget(Paragraph::new(vec![Line::raw(""), text]), inner);
            return;
        }
        let width = area.width.saturating_sub(2) as usize;
        let rows = self
            .engines
            .iter()
            .map(|info| {
                let (dot, color) = match info.availability {
                    Availability::Ready => ("●", OK),
                    Availability::NotRunning | Availability::NeedsSetup => ("◐", WARN),
                    Availability::Missing => ("○", theme::DIM),
                };
                let mut name = info.kind.name().to_string();
                if Some(info.kind) == self.active {
                    name.push_str(" ★");
                }
                let right = format!(
                    "{:<4} {}",
                    match info.kind.family() {
                        Family::Container => "ctr",
                        Family::VirtualMachine => "vm",
                        Family::BuiltIn => "host",
                    },
                    info.status_label()
                );
                let style = if info.availability == Availability::Missing {
                    theme::dim()
                } else {
                    Style::new()
                };
                theme::row(
                    Span::styled(dot, Style::new().fg(color)),
                    &name,
                    right,
                    width,
                    style,
                )
            })
            .collect();
        theme::render_list(f, area, block, rows, Some(self.engine_sel), focused);
    }

    fn draw_details(&self, f: &mut Frame, area: Rect) {
        let block = theme::panel("Details", false);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let label = |s: &str| Span::styled(format!("{s:<10}"), theme::dim());
        let lines: Vec<Line> = match self.focus {
            Focus::Engines => match self.engines.get(self.engine_sel) {
                Some(info) => vec![
                    Line::from(vec![
                        label("engine"),
                        Span::styled(info.kind.name(), theme::bold()),
                        Span::styled(format!("  ({})", info.kind.id()), theme::dim()),
                    ]),
                    Line::from(vec![
                        label("status"),
                        Span::raw(info.status_label()),
                        Span::styled(
                            info.version
                                .as_ref()
                                .map(|v| format!("  v{v}"))
                                .unwrap_or_default(),
                            theme::dim(),
                        ),
                    ]),
                    Line::from(vec![
                        label("info"),
                        Span::raw(info.note.clone().unwrap_or_else(|| "—".into())),
                    ]),
                    Line::styled(
                        if info.is_ready() {
                            "Enter: make this the default engine"
                        } else {
                            "Not usable yet; follow the hint above"
                        },
                        Style::new().fg(ACCENT),
                    ),
                ],
                None => vec![],
            },
            Focus::Workflows => match self.rows.get(self.row_sel) {
                Some(Row::Workflow(i)) => {
                    let file = &self.workflows[*i];
                    let rel = file.path.strip_prefix(&self.root).unwrap_or(&file.path);
                    let events = file.events();
                    let jobs = file.workflow().map(|w| w.jobs.len()).unwrap_or(0);
                    let note = if events.iter().any(|e| e == self.event()) {
                        Span::raw("")
                    } else {
                        Span::styled(
                            format!("  (not normally triggered by {})", self.event()),
                            Style::new().fg(WARN),
                        )
                    };
                    vec![
                        Line::from(vec![
                            label("workflow"),
                            Span::styled(file.display_name(), theme::bold()),
                            Span::styled(format!("  {}", rel.display()), theme::dim()),
                        ]),
                        Line::from(vec![label("on"), Span::raw(events.join(", ")), note]),
                        Line::from(vec![label("jobs"), Span::raw(jobs.to_string())]),
                        Line::styled(
                            format!("Enter: run all jobs as `{}`", self.event()),
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
                                "—".into()
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
                    "Workflows live in .github/workflows/*.yml",
                    theme::dim(),
                )],
            },
        };
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        if let Some((msg, error, _)) = &self.flash {
            let style = if *error {
                Style::new().fg(BAD)
            } else {
                Style::new().fg(OK)
            };
            f.render_widget(Paragraph::new(Line::styled(format!(" {msg}"), style)), area);
            return;
        }
        let enter = if self.focus == Focus::Engines {
            "use engine"
        } else {
            "run"
        };
        let line = theme::hints(&[
            ("↑↓", "move"),
            ("tab", "switch panel"),
            ("enter", enter),
            ("e", "event"),
            ("r", "rescan"),
            ("q", "quit"),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }
}

fn family_order(f: Family) -> u8 {
    match f {
        Family::Container => 0,
        Family::VirtualMachine => 1,
        Family::BuiltIn => 2,
    }
}

fn availability_order(a: &Availability) -> u8 {
    match a {
        Availability::Ready => 0,
        Availability::NotRunning => 1,
        Availability::NeedsSetup => 2,
        Availability::Missing => 3,
    }
}
