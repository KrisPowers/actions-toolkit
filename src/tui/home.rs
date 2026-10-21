//! The dashboard: pick a workflow or job on the left, an engine on the right, press enter.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use super::input::{InputAction, TextInput};
use super::run_view::{RunAction, RunView};
use super::theme::{self, ACCENT, BAD, OK, WARN};
use super::FRAME;
use crate::config::Config;
use crate::engine::{self, Availability, EngineInfo, Family, Kind};
use crate::runner::RunOptions;
use crate::workflow::{self, plan::RunPlan, Location, Source, WorkflowFile};

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

/// What a text prompt's answer is for.
enum Purpose {
    AddLocation(Source),
    /// Instance name or `user@host[:port]` for a VM engine.
    EngineTarget(Kind),
    /// SSH key for a VM engine reached over SSH.
    EngineKey(Kind),
}

struct Prompt {
    title: String,
    help: String,
    input: TextInput,
    purpose: Purpose,
    error: Option<String>,
}

/// A popup drawn over the dashboard that takes all key presses.
enum Modal {
    Locations,
    Prompt(Prompt),
}

pub struct App {
    root: PathBuf,
    cfg: Config,
    workflows: Vec<WorkflowFile>,
    /// Every searched location and how many workflows it holds (`None` if it doesn't exist).
    locations: Vec<(Location, Option<usize>)>,
    loc_sel: usize,
    modal: Option<Modal>,
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
            locations: Vec::new(),
            loc_sel: 0,
            modal: None,
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
        self.reload_workflows();
        self.detect_engines();
    }

    fn reload_workflows(&mut self) {
        self.locations = workflow::locations(&self.root)
            .into_iter()
            .map(|l| {
                let found = l.path.exists().then(|| workflow::scan(&l).len());
                (l, found)
            })
            .collect();
        self.loc_sel = self.loc_sel.min(self.locations.len().saturating_sub(1));
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
    }

    fn detect_engines(&mut self) {
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
        if let Some(modal) = self.modal.take() {
            if ctrl_c {
                return;
            }
            self.modal = self.on_modal_key(modal, key);
            return;
        }
        match key.code {
            _ if ctrl_c => self.quit = true,
            KeyCode::Char('w') => self.modal = Some(Modal::Locations),
            KeyCode::Char('t') if self.focus == Focus::Engines => self.target_prompt(),
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

    /// Handles a key inside a popup; returns the popup to show next.
    fn on_modal_key(&mut self, modal: Modal, key: KeyEvent) -> Option<Modal> {
        match modal {
            Modal::Locations => match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('w') => None,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.loc_sel = self.loc_sel.saturating_sub(1);
                    Some(Modal::Locations)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.loc_sel = (self.loc_sel + 1).min(self.locations.len().saturating_sub(1));
                    Some(Modal::Locations)
                }
                KeyCode::Char('a') => Some(location_prompt(Source::Project)),
                KeyCode::Char('g') => Some(location_prompt(Source::Global)),
                KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                    self.remove_location();
                    Some(Modal::Locations)
                }
                _ => Some(Modal::Locations),
            },
            Modal::Prompt(mut prompt) => {
                let base = match prompt.purpose {
                    Purpose::AddLocation(Source::Global) | Purpose::EngineKey(_) => {
                        dirs::home_dir()
                    }
                    Purpose::AddLocation(_) => Some(self.root.clone()),
                    Purpose::EngineTarget(_) => None,
                };
                match prompt.input.on_key(key, base.as_deref()) {
                    InputAction::None => Some(Modal::Prompt(prompt)),
                    InputAction::Cancel => match prompt.purpose {
                        Purpose::AddLocation(_) => Some(Modal::Locations),
                        _ => None,
                    },
                    InputAction::Submit => {
                        let value = prompt.input.value().trim().to_string();
                        let result = match prompt.purpose {
                            Purpose::AddLocation(source) => self
                                .add_location(source, &value)
                                .map(|msg| (msg, Some(Modal::Locations))),
                            Purpose::EngineTarget(kind) => self.set_target(kind, &value),
                            Purpose::EngineKey(kind) => self.set_key(kind, &value),
                        };
                        match result {
                            Ok((msg, next)) => {
                                self.say(msg, false);
                                next
                            }
                            Err(e) => {
                                prompt.error = Some(e);
                                Some(Modal::Prompt(prompt))
                            }
                        }
                    }
                }
            }
        }
    }

    fn target_prompt(&mut self) {
        let Some(info) = self.engines.get(self.engine_sel) else {
            return;
        };
        let kind = info.kind;
        if kind.family() != Family::VirtualMachine {
            self.say(format!("{} doesn't need a target", kind.name()), true);
            return;
        }
        let current = self.cfg.engine(kind.id());
        let value = current.ssh.or(current.instance).unwrap_or_default();
        self.modal = Some(Modal::Prompt(Prompt {
            title: format!("{} target", kind.name()),
            help: "VM/instance name, or user@host[:port] to connect over SSH. Empty resets it."
                .into(),
            input: TextInput::new(&value),
            purpose: Purpose::EngineTarget(kind),
            error: None,
        }));
    }

    /// Saves a VM engine's instance name or SSH target; an SSH target asks for a key next.
    fn set_target(&mut self, kind: Kind, value: &str) -> Result<(String, Option<Modal>), String> {
        let ssh = value.contains('@');
        self.edit_engine(kind, |e| {
            e.instance = (!ssh && !value.is_empty()).then(|| value.to_string());
            e.ssh = ssh.then(|| value.to_string());
            if !ssh {
                e.identity = None;
            }
        })?;
        let msg = match value {
            "" => format!("{} uses its default target", kind.name()),
            v => format!("{} will use {v}", kind.name()),
        };
        let next = ssh.then(|| {
            let key = self.cfg.engine(kind.id()).identity.unwrap_or_default();
            Modal::Prompt(Prompt {
                title: format!("{} SSH key", kind.name()),
                help:
                    "Private key file for the SSH connection. Leave empty to use your SSH defaults."
                        .into(),
                input: TextInput::new(&key),
                purpose: Purpose::EngineKey(kind),
                error: None,
            })
        });
        Ok((msg, next))
    }

    fn set_key(&mut self, kind: Kind, value: &str) -> Result<(String, Option<Modal>), String> {
        if !value.is_empty()
            && !workflow::resolve(value, &dirs::home_dir().unwrap_or_default()).is_file()
        {
            return Err(format!("{value} is not a file"));
        }
        self.edit_engine(kind, |e| {
            e.identity = (!value.is_empty()).then(|| value.to_string())
        })?;
        Ok((format!("{} SSH settings saved", kind.name()), None))
    }

    /// Changes one engine's settings in the user config, saves, and re-detects engines.
    fn edit_engine(
        &mut self,
        kind: Kind,
        edit: impl FnOnce(&mut crate::config::EngineConfig),
    ) -> Result<(), String> {
        let mut user = Config::load_user().map_err(|e| e.to_string())?;
        let entry = user.engines.entry(kind.id().to_string()).or_default();
        edit(entry);
        if entry.instance.is_none() && entry.ssh.is_none() && entry.identity.is_none() {
            user.engines.remove(kind.id());
        }
        user.save_user().map_err(|e| e.to_string())?;
        self.cfg = Config::load(&self.root).map_err(|e| e.to_string())?;
        self.detect_engines();
        Ok(())
    }

    /// Saves a new workflow location to the project or user config.
    fn add_location(&mut self, source: Source, value: &str) -> Result<String, String> {
        if value.is_empty() {
            return Err("Type a folder or a .yml file".into());
        }
        let base = match source {
            Source::Global => dirs::home_dir().ok_or("could not find the home directory")?,
            _ => self.root.clone(),
        };
        let path = workflow::resolve(value, &base);
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        // Project entries are stored relative to the root so the config stays portable.
        let entry = match source {
            Source::Project => match path.strip_prefix(&self.root) {
                Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
                Ok(rel) => rel.to_string_lossy().into_owned(),
                Err(_) => path.to_string_lossy().into_owned(),
            },
            _ => value.trim_end_matches(['/', '\\']).to_string(),
        };
        let found = workflow::scan(&Location {
            source,
            entry: entry.clone(),
            path,
        })
        .len();
        self.edit_locations(source, |list| {
            if list.contains(&entry) {
                return Err(format!("{entry} is already listed"));
            }
            list.push(entry.clone());
            Ok(())
        })?;
        let scope = match source {
            Source::Global => "every project",
            _ => "this project",
        };
        Ok(format!(
            "Added {entry} for {scope}: {found} workflow{} found",
            if found == 1 { "" } else { "s" }
        ))
    }

    fn remove_location(&mut self) {
        let Some((loc, _)) = self.locations.get(self.loc_sel).cloned() else {
            return;
        };
        if loc.source == Source::Auto {
            self.say(format!("{} is always searched", loc.entry), true);
            return;
        }
        let entry = loc.entry.clone();
        match self.edit_locations(loc.source, |list| {
            list.retain(|e| *e != entry);
            Ok(())
        }) {
            Ok(()) => self.say(format!("Removed {entry}"), false),
            Err(e) => self.say(e, true),
        }
    }

    /// Changes the location list in the project or user config, saves it and rescans.
    fn edit_locations(
        &mut self,
        source: Source,
        edit: impl FnOnce(&mut Vec<String>) -> Result<(), String>,
    ) -> Result<(), String> {
        let saved = match source {
            Source::Global => Config::load_user()
                .map_err(|e| e.to_string())
                .and_then(|mut c| {
                    edit(&mut c.workflows)?;
                    c.save_user().map_err(|e| e.to_string())
                }),
            _ => Config::load_project(&self.root)
                .map_err(|e| e.to_string())
                .and_then(|mut c| {
                    edit(&mut c.workflows)?;
                    c.save_project(&self.root).map_err(|e| e.to_string())
                }),
        };
        saved?;
        self.reload_workflows();
        Ok(())
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
        match &self.modal {
            Some(Modal::Locations) => self.draw_locations(f, area),
            Some(Modal::Prompt(prompt)) => draw_prompt(f, area, prompt),
            None => {}
        }
    }

    fn draw_locations(&self, f: &mut Frame, area: Rect) {
        let height = (self.locations.len() as u16 + 5).min(area.height);
        let popup = centered(area, 76, height);
        f.render_widget(Clear, popup);
        let block = theme::panel("Workflow locations", true);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let [list, hints] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
        let width = list.width as usize;
        let rows = self
            .locations
            .iter()
            .map(|(loc, found)| {
                let (dot, color, right) = match found {
                    None => ("○", theme::DIM, "missing".to_string()),
                    Some(0) => ("○", theme::DIM, "none".to_string()),
                    Some(n) => ("●", OK, format!("{n} found")),
                };
                let name = format!("{:<8} {}", loc.source.label(), loc.entry);
                theme::row(
                    Span::styled(dot, Style::new().fg(color)),
                    &name,
                    right,
                    width,
                    Style::new(),
                )
            })
            .collect();
        theme::render_list(
            f,
            list,
            ratatui::widgets::Block::new(),
            rows,
            Some(self.loc_sel),
            true,
        );
        let line = theme::hints(&[
            ("a", "add to project"),
            ("g", "add global"),
            ("d", "remove"),
            ("esc", "close"),
        ]);
        f.render_widget(Paragraph::new(vec![Line::raw(""), line]), hints);
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
                Line::styled(" Press w to add a folder or file to search.", theme::dim()),
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
                    let rel = display_path(&file.path, &self.root);
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
                            Span::styled(
                                format!("  {rel}  ({})", file.source.label()),
                                theme::dim(),
                            ),
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
                    format!(
                        "Searched {} automatically. Press w to manage workflow locations.",
                        workflow::AUTO_DIRS.join(" and ")
                    ),
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
        let vm = self.focus == Focus::Engines
            && self
                .engines
                .get(self.engine_sel)
                .is_some_and(|i| i.kind.family() == Family::VirtualMachine);
        let mut keys = vec![("↑↓", "move"), ("tab", "switch panel"), ("enter", enter)];
        if vm {
            keys.push(("t", "set target"));
        }
        keys.extend([
            ("w", "locations"),
            ("e", "event"),
            ("r", "rescan"),
            ("q", "quit"),
        ]);
        let line = theme::hints(&keys);
        f.render_widget(Paragraph::new(line), area);
    }
}

fn location_prompt(source: Source) -> Modal {
    let (title, help) = match source {
        Source::Global => (
            "Add a global location",
            "Folder or .yml file searched in every project (relative to your home folder)",
        ),
        _ => (
            "Add a project location",
            "Folder or .yml file searched in this project (relative to the project root)",
        ),
    };
    Modal::Prompt(Prompt {
        title: title.into(),
        help: help.into(),
        input: TextInput::default(),
        purpose: Purpose::AddLocation(source),
        error: None,
    })
}

fn draw_prompt(f: &mut Frame, area: Rect, prompt: &Prompt) {
    let popup = centered(area, 76, 8);
    f.render_widget(Clear, popup);
    let block = theme::panel(&prompt.title, true);
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let error = match &prompt.error {
        Some(e) => Line::styled(e.clone(), Style::new().fg(BAD)),
        None => Line::raw(""),
    };
    let lines = vec![
        Line::styled(prompt.help.clone(), theme::dim()),
        Line::raw(""),
        prompt.input.line(),
        error,
        Line::raw(""),
        theme::hints(&[("tab", "complete"), ("enter", "save"), ("esc", "cancel")]),
    ];
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// A `width` x `height` rectangle centred in `area` (clamped to fit).
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
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

/// `path` relative to the project root, or to the home directory as `~/...`.
fn display_path(path: &Path, root: &Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        return rel.display().to_string();
    }
    match dirs::home_dir().and_then(|h| path.strip_prefix(h).ok().map(|r| r.to_path_buf())) {
        Some(rel) => format!("~/{}", rel.display()),
        None => path.display().to_string(),
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn demo_app() -> App {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
        let mut app = App::new(root).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.detect_rx.is_some() && Instant::now() < deadline {
            app.update();
            thread::sleep(Duration::from_millis(20));
        }
        app
    }

    #[test]
    fn dashboard_lists_workflows_and_engines() {
        let mut app = demo_app();
        let screen = render(&mut app, 110, 24);
        if std::env::var("MAVERICK_PRINT").is_ok() {
            println!("{screen}");
        }
        assert!(screen.contains("Workflows"));
        assert!(screen.contains("Demo"));
        assert!(screen.contains("×2"), "matrix size is shown");
        assert!(screen.contains("Maverick Native"));
    }

    #[test]
    fn adding_and_removing_a_project_location() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("pipelines")).unwrap();
        std::fs::write(
            root.join("pipelines/build.yml"),
            "name: Build\non: push\njobs:\n  compile: { steps: [{ run: echo }] }\n",
        )
        .unwrap();
        let mut app = App::new(root.clone()).unwrap();
        assert!(app.workflows.is_empty());

        app.on_key(KeyEvent::from(KeyCode::Char('w')));
        app.on_key(KeyEvent::from(KeyCode::Char('a')));
        for c in "pip".chars() {
            app.on_key(KeyEvent::from(KeyCode::Char(c)));
        }
        app.on_key(KeyEvent::from(KeyCode::Tab));
        app.on_key(KeyEvent::from(KeyCode::Enter));
        let screen = render(&mut app, 100, 24);
        if std::env::var("MAVERICK_PRINT").is_ok() {
            println!("{screen}");
        }
        assert!(screen.contains("Workflow locations"), "{screen}");
        assert!(screen.contains("project  pipelines"), "{screen}");
        assert_eq!(app.workflows.len(), 1);
        let saved = std::fs::read_to_string(root.join(".maverick.toml")).unwrap();
        assert!(saved.contains("pipelines"), "{saved}");

        app.loc_sel = app
            .locations
            .iter()
            .position(|(l, _)| l.source == Source::Project)
            .unwrap();
        app.on_key(KeyEvent::from(KeyCode::Char('d')));
        assert!(app.workflows.is_empty());
        app.on_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.modal.is_none());
    }

    #[test]
    fn running_a_job_from_the_dashboard() {
        let mut app = demo_app();
        app.active = Some(Kind::Native);
        app.row_sel = 1; // first job: build
        app.start_selected();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !app.run.as_ref().unwrap().is_finished() && Instant::now() < deadline {
            app.update();
            thread::sleep(Duration::from_millis(20));
        }
        let screen = render(&mut app, 110, 26);
        if std::env::var("MAVERICK_PRINT").is_ok() {
            println!("{screen}");
        }
        let run = app.run.as_ref().unwrap();
        assert!(run.success(), "{screen}");
        assert!(screen.contains("succeeded"));
        assert!(screen.contains("Compute version"));
    }
}
