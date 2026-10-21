//! The TUI application: a stack of screens sharing project state, plus popups for text input
//! and confirmations. `Backspace` goes back one screen; leaving the first screen quits.

mod catalog;
mod history;
mod menu;
mod settings;

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
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};

use super::dialog::{Answer, Confirm, Prompt};
use super::input::InputAction;
use super::run_view::{RunAction, RunMeta, RunView};
use super::theme::{self, ACCENT, BAD, OK, WARN};
use super::FRAME;
use crate::config::{Config, EngineConfig};
use crate::engine::{self, Availability, EngineInfo, Family, Kind};
use crate::history::Store;
use crate::runner::RunOptions;
use crate::workflow::{self, plan::RunPlan, Location, Source, WorkflowFile};

/// Events a run can simulate.
pub const EVENTS: [&str; 5] = [
    "push",
    "pull_request",
    "workflow_dispatch",
    "schedule",
    "release",
];

/// The first screen to show.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// `maverick`: settings and the guide.
    Settings,
    /// `maverick workflows`: the catalog / runs menu.
    Workflows,
}

enum Screen {
    Settings(settings::Settings),
    Menu(menu::Menu),
    Catalog(catalog::Catalog),
    History(history::History),
    Run(Box<RunView>),
}

/// What a screen's key handler wants to happen next.
enum Nav {
    Stay,
    Back,
    Push(Screen),
    /// Swap the current screen for another one.
    Replace(Screen),
}

/// What a text prompt's answer is for.
enum Purpose {
    AddLocation(Source),
    /// Instance name or `user@host[:port]` for a VM engine.
    EngineTarget(Kind),
    /// SSH key for a VM engine reached over SSH.
    EngineKey(Kind),
    Parallel,
}

/// Actions that destroy something and need a confirmation first.
enum Danger {
    DeleteRun(String),
    ClearHistory,
    RemoveLocation(Source, String),
    ResetSettings,
    CancelRun,
    Quit,
}

enum Modal {
    Prompt(Prompt<Purpose>),
    Confirm(Confirm<Danger>),
}

pub struct App {
    root: PathBuf,
    cfg: Config,
    store: Option<Store>,
    workflows: Vec<WorkflowFile>,
    /// Every searched location and how many workflows it holds (`None` if it doesn't exist).
    locations: Vec<(Location, Option<usize>)>,
    engines: Vec<EngineInfo>,
    detect_rx: Option<Receiver<Vec<EngineInfo>>>,
    /// Engine runs use: the configured default, else the first ready of Docker/Podman/Native.
    active: Option<Kind>,
    event: String,
    stack: Vec<Screen>,
    modal: Option<Modal>,
    flash: Option<(String, bool, Instant)>,
    tick: usize,
    quit: bool,
}

impl App {
    pub fn new(root: PathBuf, start: Start) -> Result<Self> {
        Self::with_store(root, start, Store::open_default())
    }

    fn with_store(root: PathBuf, start: Start, store: Option<Store>) -> Result<Self> {
        let cfg = Config::load(&root)?;
        let mut app = Self {
            root,
            active: cfg.engine.as_deref().and_then(Kind::parse),
            event: cfg.event.clone().unwrap_or_else(|| "push".into()),
            cfg,
            store,
            workflows: Vec::new(),
            locations: Vec::new(),
            engines: Vec::new(),
            detect_rx: None,
            stack: Vec::new(),
            modal: None,
            flash: None,
            tick: 0,
            quit: false,
        };
        app.reload_workflows();
        app.detect_engines();
        let first = match start {
            Start::Settings => Screen::Settings(settings::Settings::default()),
            Start::Workflows => Screen::Menu(menu::Menu::new(app.store.as_ref(), &app.root)),
        };
        app.stack.push(first);
        Ok(app)
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
        self.abandon_runs();
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Shared state

    fn reload_workflows(&mut self) {
        self.locations = workflow::locations(&self.root)
            .into_iter()
            .map(|l| {
                let found = l.path.exists().then(|| workflow::scan(&l).len());
                (l, found)
            })
            .collect();
        self.workflows = workflow::discover(&self.root);
        for screen in &mut self.stack {
            if let Screen::Catalog(c) = screen {
                c.rebuild(&self.workflows);
            }
        }
    }

    fn detect_engines(&mut self) {
        let (tx, rx) = mpsc::channel();
        let cfg = self.cfg.clone();
        thread::spawn(move || {
            let _ = tx.send(engine::detect_all(&cfg));
        });
        self.detect_rx = Some(rx);
    }

    fn update(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        for screen in &mut self.stack {
            if let Screen::Run(run) = screen {
                run.update();
            }
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
                self.active = self
                    .cfg
                    .engine
                    .as_deref()
                    .and_then(Kind::parse)
                    .or_else(|| {
                        [Kind::Docker, Kind::Podman, Kind::Native]
                            .into_iter()
                            .find(|k| self.engines.iter().any(|i| i.kind == *k && i.is_ready()))
                    });
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

    fn running(&self) -> bool {
        self.stack
            .iter()
            .any(|s| matches!(s, Screen::Run(r) if !r.is_finished()))
    }

    /// Stops and saves any run still going, for when maverick exits.
    fn abandon_runs(&mut self) {
        for screen in &mut self.stack {
            if let Screen::Run(run) = screen {
                run.abandon();
            }
        }
    }

    /// Starts `workflow` (or one `job` of it) on the active engine.
    fn start_run(&self, path: &Path, job: Option<String>, event: &str) -> Result<Screen, String> {
        let Some(kind) = self.active else {
            return Err(if self.detect_rx.is_some() {
                "Still detecting engines, try again in a moment".into()
            } else {
                "No engine is ready. Pick one in settings (run `maverick`)".into()
            });
        };
        let (wf, name) = match self.workflows.iter().find(|f| f.path == path) {
            Some(file) => (
                file.parsed
                    .clone()
                    .map_err(|e| format!("{} is invalid: {}", file.file_name, first_line(&e)))?,
                file.display_name(),
            ),
            None => {
                let wf = workflow::read(path)?;
                let name = wf.name.clone().unwrap_or_else(|| {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                (wf, name)
            }
        };
        let plan = RunPlan::build(&[(wf, name)], job.as_deref()).map_err(|e| format!("{e:#}"))?;
        let engine = engine::build(kind, &self.cfg).map_err(|e| format!("{e:#}"))?;
        let opts = RunOptions::new(&self.root, &self.cfg, event);
        let meta = RunMeta {
            project: self.root.clone(),
            workflow: path.to_path_buf(),
            job,
            store: self.store.clone(),
        };
        Ok(Screen::Run(Box::new(RunView::start(
            Arc::new(plan),
            engine,
            Arc::new(opts),
            meta,
        ))))
    }

    /// Saves a change to the user config and reloads the merged config.
    fn edit_user_config(&mut self, edit: impl FnOnce(&mut Config)) -> Result<(), String> {
        let mut user = Config::load_user().map_err(|e| e.to_string())?;
        edit(&mut user);
        user.save_user().map_err(|e| e.to_string())?;
        self.cfg = Config::load(&self.root).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Changes one engine's settings in the user config and re-detects engines.
    fn edit_engine(
        &mut self,
        kind: Kind,
        edit: impl FnOnce(&mut EngineConfig),
    ) -> Result<(), String> {
        self.edit_user_config(|user| {
            let entry = user.engines.entry(kind.id().to_string()).or_default();
            edit(entry);
            if entry.instance.is_none() && entry.ssh.is_none() && entry.identity.is_none() {
                user.engines.remove(kind.id());
            }
        })?;
        self.detect_engines();
        Ok(())
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

    // -----------------------------------------------------------------------------------------
    // Keys

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl_c =
            key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if let Some(modal) = self.modal.take() {
            self.modal = if ctrl_c {
                None
            } else {
                self.on_modal_key(modal, key)
            };
            return;
        }
        let in_run = matches!(self.stack.last(), Some(Screen::Run(_)));
        if ctrl_c || (key.code == KeyCode::Char('q') && !in_run) {
            self.request_quit();
            return;
        }
        let Some(mut top) = self.stack.pop() else {
            self.quit = true;
            return;
        };
        let nav = match &mut top {
            Screen::Settings(s) => self.settings_key(s, key),
            Screen::Menu(m) => self.menu_key(m, key),
            Screen::Catalog(c) => self.catalog_key(c, key),
            Screen::History(h) => self.history_key(h, key),
            Screen::Run(r) => self.run_key(r, key),
        };
        match nav {
            Nav::Stay => self.stack.push(top),
            Nav::Back => {
                if self.stack.is_empty() {
                    self.quit = true;
                }
                self.resume();
            }
            Nav::Push(next) => {
                self.stack.push(top);
                self.stack.push(next);
            }
            Nav::Replace(next) => self.stack.push(next),
        }
    }

    /// Refreshes the screen that just came back into view.
    fn resume(&mut self) {
        let store = self.store.clone();
        let root = self.root.clone();
        match self.stack.last_mut() {
            Some(Screen::History(h)) => h.reload(store.as_ref(), &root),
            Some(Screen::Menu(m)) => m.runs = store.map(|s| s.list(&root).len()),
            _ => {}
        }
    }

    fn request_quit(&mut self) {
        if self.running() {
            self.modal = Some(Modal::Confirm(Confirm::new(
                "Quit?",
                "A run is still going. Stop it and quit?",
                "Stop and quit",
                Danger::Quit,
            )));
        } else {
            self.quit = true;
        }
    }

    fn run_key(&mut self, run: &mut RunView, key: KeyEvent) -> Nav {
        match run.on_key(key) {
            RunAction::None => Nav::Stay,
            RunAction::Close => Nav::Back,
            RunAction::ConfirmCancel => {
                self.modal = Some(Modal::Confirm(Confirm::new(
                    "Cancel run?",
                    "Stop this run? Running steps are killed.",
                    "Stop run",
                    Danger::CancelRun,
                )));
                Nav::Stay
            }
            RunAction::Rerun => {
                let (path, job, event) = (run.workflow.clone(), run.job.clone(), run.event.clone());
                match self.start_run(&path, job, &event) {
                    Ok(screen) => Nav::Replace(screen),
                    Err(e) => {
                        self.say(e, true);
                        Nav::Stay
                    }
                }
            }
        }
    }

    fn on_modal_key(&mut self, modal: Modal, key: KeyEvent) -> Option<Modal> {
        match modal {
            Modal::Confirm(mut confirm) => match confirm.on_key(key) {
                Answer::Pending => Some(Modal::Confirm(confirm)),
                Answer::No => None,
                Answer::Yes => {
                    self.apply(confirm.action);
                    None
                }
            },
            Modal::Prompt(mut prompt) => {
                let base = match prompt.purpose {
                    Purpose::AddLocation(Source::Global) | Purpose::EngineKey(_) => {
                        dirs::home_dir()
                    }
                    Purpose::AddLocation(_) => Some(self.root.clone()),
                    _ => None,
                };
                match prompt.input.on_key(key, base.as_deref()) {
                    InputAction::None => Some(Modal::Prompt(prompt)),
                    InputAction::Cancel => None,
                    InputAction::Submit => {
                        let value = prompt.input.value().trim().to_string();
                        match self.submit(&prompt.purpose, &value) {
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

    /// Applies a prompt's answer; returns a message and optionally a follow-up popup.
    fn submit(
        &mut self,
        purpose: &Purpose,
        value: &str,
    ) -> Result<(String, Option<Modal>), String> {
        match purpose {
            Purpose::AddLocation(source) => self.add_location(*source, value).map(|m| (m, None)),
            Purpose::EngineTarget(kind) => self.set_target(*kind, value),
            Purpose::EngineKey(kind) => {
                let home = dirs::home_dir().unwrap_or_default();
                if !value.is_empty() && !workflow::resolve(value, &home).is_file() {
                    return Err(format!("{value} is not a file"));
                }
                self.edit_engine(*kind, |e| {
                    e.identity = (!value.is_empty()).then(|| value.to_string())
                })?;
                Ok((format!("{} SSH settings saved", kind.name()), None))
            }
            Purpose::Parallel => {
                let parallel = match value {
                    "" | "auto" => None,
                    v => match v.parse::<usize>() {
                        Ok(n) if n >= 1 => Some(n),
                        _ => {
                            return Err(
                                "Enter a whole number of 1 or more, or leave it empty".into()
                            )
                        }
                    },
                };
                self.edit_user_config(|c| c.parallel = parallel)?;
                let msg = match parallel {
                    Some(n) => format!("Up to {n} jobs run at once"),
                    None => "Jobs at once: automatic".into(),
                };
                Ok((msg, None))
            }
        }
    }

    /// Carries out a confirmed dangerous action.
    fn apply(&mut self, action: Danger) {
        let result: Result<String, String> = match action {
            Danger::DeleteRun(id) => match &self.store {
                Some(store) => store
                    .delete(&id)
                    .map(|_| "Run deleted".to_string())
                    .map_err(|e| format!("{e:#}")),
                None => Err("No history folder on this system".into()),
            },
            Danger::ClearHistory => match &self.store {
                Some(store) => store
                    .clear(&self.root)
                    .map(|n| format!("Deleted {n} run{}", if n == 1 { "" } else { "s" }))
                    .map_err(|e| format!("{e:#}")),
                None => Err("No history folder on this system".into()),
            },
            Danger::RemoveLocation(source, entry) => self
                .edit_locations(source, |list| {
                    list.retain(|e| *e != entry);
                    Ok(())
                })
                .map(|_| format!("Removed {entry}")),
            Danger::ResetSettings => self.reset_settings(),
            Danger::CancelRun => {
                if let Some(Screen::Run(run)) = self.stack.last() {
                    run.cancel();
                }
                Ok("Cancelling…".into())
            }
            Danger::Quit => {
                self.abandon_runs();
                self.quit = true;
                Ok(String::new())
            }
        };
        match result {
            Ok(msg) if !msg.is_empty() => self.say(msg, false),
            Ok(_) => {}
            Err(e) => self.say(e, true),
        }
        self.resume();
    }

    fn reset_settings(&mut self) -> Result<String, String> {
        if let Some(path) = Config::user_path() {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
        }
        self.cfg = Config::load(&self.root).map_err(|e| e.to_string())?;
        self.active = None;
        self.event = self.cfg.event.clone().unwrap_or_else(|| "push".into());
        self.reload_workflows();
        self.detect_engines();
        Ok("Settings reset to defaults".into())
    }

    // -----------------------------------------------------------------------------------------
    // Drawing

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        if let Some(Screen::Run(run)) = self.stack.last_mut() {
            run.draw(f, area);
        } else {
            let [header, body, footer] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .areas(area);
            let (crumbs, hints) = match self.stack.last() {
                Some(Screen::Settings(s)) => (vec!["settings"], self.draw_settings(s, f, body)),
                Some(Screen::Menu(m)) => (vec!["workflows"], self.draw_menu(m, f, body)),
                Some(Screen::Catalog(c)) => {
                    (vec!["workflows", "catalog"], self.draw_catalog(c, f, body))
                }
                Some(Screen::History(h)) => {
                    (vec!["workflows", "runs"], self.draw_history(h, f, body))
                }
                _ => (vec![], vec![]),
            };
            self.draw_header(f, header, &crumbs);
            self.draw_footer(f, footer, &hints);
        }
        match &self.modal {
            Some(Modal::Prompt(p)) => p.draw(f, area),
            Some(Modal::Confirm(c)) => c.draw(f, area),
            None => {}
        }
    }

    fn draw_header(&self, f: &mut Frame, area: Rect, crumbs: &[&str]) {
        let engine = match self.active {
            Some(k) => Span::styled(format!("● {}", k.name()), Style::new().fg(OK)),
            None if self.detect_rx.is_some() => Span::styled(
                format!("{} detecting…", theme::spinner(self.tick / 2)),
                Style::new().fg(WARN),
            ),
            None => Span::styled("○ no engine", Style::new().fg(BAD)),
        };
        let right = Line::from(vec![
            Span::styled("event ", theme::dim()),
            Span::styled(self.event.clone(), theme::bold()),
            Span::styled("   engine ", theme::dim()),
            engine,
            Span::raw(" "),
        ]);
        let mut left = vec![Span::styled(
            " ◆ maverick",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )];
        for c in crumbs {
            left.push(Span::styled(" › ", theme::dim()));
            left.push(Span::styled(c.to_string(), theme::bold()));
        }
        // The project path gets whatever room is left between the two sides.
        let used = Line::from(left.clone()).width() + right.width() + 5;
        let room = (area.width as usize).saturating_sub(used);
        let path = tilde(&self.root);
        let path = if path.chars().count() > room {
            let tail: String = path.chars().rev().take(room.saturating_sub(1)).collect();
            format!("…{}", tail.chars().rev().collect::<String>())
        } else {
            path
        };
        if room > 1 {
            left.push(Span::styled(format!("   {path}"), theme::dim()));
        }
        f.render_widget(Paragraph::new(Line::from(left)), area);
        f.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
        if let Some((msg, error, _)) = &self.flash {
            let style = Style::new().fg(if *error { BAD } else { OK });
            f.render_widget(Paragraph::new(Line::styled(format!(" {msg}"), style)), area);
            return;
        }
        f.render_widget(Paragraph::new(theme::hints(hints)), area);
    }

    /// An engine as a list row: status dot, name (★ if active), type and status.
    fn engine_row(&self, info: &EngineInfo, width: usize) -> Line<'static> {
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

/// `path` relative to `root` if inside it, else as `tilde` shows it.
fn display_path(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => tilde(path),
    }
}

/// `path` with the home directory shortened to `~`.
fn tilde(path: &Path) -> String {
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

    pub fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let screen = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if std::env::var("MAVERICK_PRINT").is_ok() {
            println!("{screen}\n");
        }
        screen
    }

    pub fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// An app for the demo project with a throwaway history folder, engines detected.
    pub fn demo_app(start: Start) -> (App, tempfile::TempDir) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/demo");
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut app = App::with_store(root, start, Some(store)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while app.detect_rx.is_some() && Instant::now() < deadline {
            app.update();
            thread::sleep(Duration::from_millis(20));
        }
        (app, dir)
    }

    /// Updates the app until the run on top of the stack finishes.
    pub fn wait_for_run(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            app.update();
            match app.stack.last() {
                Some(Screen::Run(r)) if !r.is_finished() => {
                    thread::sleep(Duration::from_millis(20))
                }
                _ => return,
            }
        }
    }

    #[test]
    fn backspace_from_the_first_screen_quits() {
        let (mut app, _dir) = demo_app(Start::Workflows);
        press(&mut app, KeyCode::Backspace);
        assert!(app.quit);
    }

    #[test]
    fn quitting_during_a_run_asks_first() {
        let (mut app, _dir) = demo_app(Start::Workflows);
        app.active = Some(Kind::Native);
        let path = app.workflows[0].path.clone();
        let run = app.start_run(&path, Some("build".into()), "push").unwrap();
        app.stack.push(run);
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        if app.running() {
            assert!(matches!(app.modal, Some(Modal::Confirm(_))));
            assert!(!app.quit);
            press(&mut app, KeyCode::Esc);
            assert!(app.modal.is_none());
        }
        wait_for_run(&mut app);
    }
}
