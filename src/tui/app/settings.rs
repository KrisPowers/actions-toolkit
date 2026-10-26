//! `atk settings`: the guide and settings. Sections on the left, their contents on the right.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::{menu::plural, tilde, App, Danger, Modal, Nav, Purpose, EVENTS};
use crate::config::Config;
use crate::engine;
use crate::tui::dialog::{Confirm, Prompt};
use crate::tui::theme::{self, ACCENT, OK};
use crate::workflow::{self, Location, Source};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Guide,
    Docker,
    Locations,
    Options,
    Data,
}

const SECTIONS: [Section; 5] = [
    Section::Guide,
    Section::Docker,
    Section::Locations,
    Section::Options,
    Section::Data,
];

impl Section {
    fn title(self) -> &'static str {
        match self {
            Section::Guide => "Guide",
            Section::Docker => "Docker",
            Section::Locations => "Workflow locations",
            Section::Options => "Run options",
            Section::Data => "Data",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Section::Guide => "How ATK works, its commands and keys.",
            Section::Docker => "Every job runs in its own Docker container. Is Docker ready?",
            Section::Locations => {
                "Folders and files searched for workflows, for this project or every project."
            }
            Section::Options => "The event runs simulate and how many jobs run at once.",
            Section::Data => "Run history and resetting settings.",
        }
    }
}

#[derive(Default)]
pub struct Settings {
    section: usize,
    /// Keys go to the section's contents instead of the section list.
    inside: bool,
    sel: usize,
}

impl Settings {
    fn section(&self) -> Section {
        SECTIONS[self.section]
    }
}

impl App {
    fn section_len(&self, section: Section) -> usize {
        match section {
            Section::Guide => self.guide().len(),
            Section::Docker => 1,
            Section::Locations => self.locations.len() + 2,
            Section::Options | Section::Data => 2,
        }
    }

    pub(super) fn settings_key(&mut self, s: &mut Settings, key: KeyEvent) -> Nav {
        if !s.inside {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => s.section = s.section.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    s.section = (s.section + 1).min(SECTIONS.len() - 1)
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    s.inside = true;
                    s.sel = 0;
                }
                KeyCode::Backspace | KeyCode::Esc => return Nav::Back,
                _ => {}
            }
            return Nav::Stay;
        }
        let section = s.section();
        let len = self.section_len(section);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => s.sel = s.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => s.sel = (s.sel + 1).min(len.saturating_sub(1)),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Esc => {
                s.inside = false
            }
            KeyCode::Enter => self.activate(section, s.sel),
            KeyCode::Char('a') if section == Section::Locations => {
                self.modal = Some(location_prompt(Source::Project))
            }
            KeyCode::Char('g') if section == Section::Locations => {
                self.modal = Some(location_prompt(Source::Global))
            }
            KeyCode::Char('d') | KeyCode::Delete if section == Section::Locations => {
                self.confirm_remove_location(s.sel)
            }
            _ => {}
        }
        Nav::Stay
    }

    /// Enter on an item inside a section.
    fn activate(&mut self, section: Section, sel: usize) {
        match section {
            Section::Guide => {}
            Section::Docker => {
                self.detect_docker();
                self.say("Checking Docker…", false);
            }
            Section::Locations => {
                let n = self.locations.len();
                if sel == n {
                    self.modal = Some(location_prompt(Source::Project));
                } else if sel == n + 1 {
                    self.modal = Some(location_prompt(Source::Global));
                } else {
                    self.confirm_remove_location(sel);
                }
            }
            Section::Options if sel == 0 => {
                let i = EVENTS
                    .iter()
                    .position(|e| *e == self.event)
                    .map_or(0, |i| i + 1);
                let event = EVENTS[i % EVENTS.len()].to_string();
                match self.edit_user_config(|c| c.event = Some(event.clone())) {
                    Ok(()) => {
                        self.say(format!("Runs simulate `{event}` by default"), false);
                        self.event = event;
                    }
                    Err(e) => self.say(e, true),
                }
            }
            Section::Options => {
                let current = self.cfg.parallel.map(|n| n.to_string()).unwrap_or_default();
                self.modal = Some(Modal::Prompt(Prompt::new(
                    "Jobs at once",
                    "How many jobs may run at the same time. Leave empty for automatic.",
                    &current,
                    Purpose::Parallel,
                )));
            }
            Section::Data if sel == 0 => {
                let runs = self
                    .store
                    .as_ref()
                    .map(|s| s.list(&self.root).len())
                    .unwrap_or(0);
                if runs == 0 {
                    self.say("There are no runs to delete", false);
                } else {
                    self.modal = Some(Modal::Confirm(Confirm::new(
                        "Clear history?",
                        format!("Delete all {} of this project?", plural(runs, "run")),
                        "Delete all",
                        Danger::ClearHistory,
                    )));
                }
            }
            Section::Data => {
                self.modal = Some(Modal::Confirm(Confirm::new(
                    "Reset settings?",
                    "Delete your user settings (default event, jobs at once, global locations)?",
                    "Reset",
                    Danger::ResetSettings,
                )));
            }
        }
    }

    fn confirm_remove_location(&mut self, sel: usize) {
        let Some((loc, _)) = self.locations.get(sel) else {
            return;
        };
        if loc.source == Source::Auto {
            self.say(format!("{} is always searched", loc.entry), true);
            return;
        }
        let scope = match loc.source {
            Source::Global => "every project",
            _ => "this project",
        };
        self.modal = Some(Modal::Confirm(Confirm::new(
            "Remove location?",
            format!("Stop searching {} for {scope}?", loc.entry),
            "Remove",
            Danger::RemoveLocation(loc.source, loc.entry.clone()),
        )));
    }

    /// Saves a new workflow location to the project or user config.
    pub(super) fn add_location(&mut self, source: Source, value: &str) -> Result<String, String> {
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
            "Added {entry} for {scope}: {} found",
            plural(found, "workflow")
        ))
    }

    // -----------------------------------------------------------------------------------------
    // Drawing

    pub(super) fn draw_settings(
        &self,
        s: &Settings,
        f: &mut Frame,
        area: Rect,
    ) -> Vec<(&'static str, &'static str)> {
        let [side, main] =
            Layout::horizontal([Constraint::Length(24), Constraint::Min(0)]).areas(area);
        let [content, details] =
            Layout::vertical([Constraint::Min(5), Constraint::Length(5)]).areas(main);

        let rows = SECTIONS
            .iter()
            .map(|sec| {
                let marker = if *sec == s.section() { "▸" } else { " " };
                Line::from(vec![
                    Span::styled(format!(" {marker} "), Style::new().fg(ACCENT)),
                    Span::raw(sec.title()),
                ])
            })
            .collect();
        theme::render_list(
            f,
            side,
            theme::panel("Settings", !s.inside),
            rows,
            Some(s.section),
            !s.inside,
        );

        let section = s.section();
        let block = theme::panel(section.title(), s.inside);
        let width = content.width.saturating_sub(2) as usize;
        let sel = s.inside.then_some(s.sel);
        match section {
            Section::Guide => {
                let inner = block.inner(content);
                f.render_widget(block, content);
                let lines = self.guide();
                let start = theme::scroll_start(s.sel, lines.len(), inner.height as usize);
                let scroll = if s.inside { start } else { 0 };
                let padded = Rect {
                    x: inner.x + 1,
                    width: inner.width.saturating_sub(2),
                    ..inner
                };
                f.render_widget(
                    Paragraph::new(lines.into_iter().skip(scroll).collect::<Vec<_>>()),
                    padded,
                );
            }
            Section::Docker => {
                let (dot, color, status) = match &self.docker {
                    None => (theme::spinner(self.tick / 2), theme::WARN, "checking…"),
                    Some(i) if i.is_ready() => ("●", OK, i.status_label()),
                    Some(i) => ("○", theme::BAD, i.status_label()),
                };
                let row = theme::row(
                    Span::styled(dot, Style::new().fg(color)),
                    engine::NAME,
                    status.to_string(),
                    width,
                    Style::new(),
                );
                theme::render_list(f, content, block, vec![row], sel, s.inside);
            }
            Section::Locations => {
                let mut rows: Vec<Line> = self
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
                for label in [
                    "Add a location for this project",
                    "Add a location for every project",
                ] {
                    rows.push(theme::row(
                        Span::styled("+", Style::new().fg(ACCENT)),
                        label,
                        String::new(),
                        width,
                        Style::new().fg(ACCENT),
                    ));
                }
                theme::render_list(f, content, block, rows, sel, s.inside);
            }
            Section::Options => {
                let parallel = match self.cfg.parallel {
                    Some(n) => n.to_string(),
                    None => format!(
                        "automatic ({})",
                        std::thread::available_parallelism().map_or(4, |n| n.get())
                    ),
                };
                let rows = vec![
                    setting_row("Default event", &self.event, width),
                    setting_row("Jobs at once", &parallel, width),
                ];
                theme::render_list(f, content, block, rows, sel, s.inside);
            }
            Section::Data => {
                let runs = match &self.store {
                    Some(store) => plural(store.list(&self.root).len(), "run"),
                    None => "unavailable".into(),
                };
                let rows = vec![
                    setting_row("Clear run history", &runs, width),
                    setting_row("Reset settings", "", width),
                ];
                theme::render_list(f, content, block, rows, sel, s.inside);
            }
        }

        let block = theme::panel("Details", false);
        let inner = block.inner(details);
        f.render_widget(block, details);
        let text = if s.inside {
            self.item_help(section, s.sel)
        } else {
            section.blurb().to_string()
        };
        f.render_widget(
            Paragraph::new(Line::styled(text, theme::dim())).wrap(Wrap { trim: false }),
            inner,
        );

        let mut hints = vec![("↑↓", "move")];
        if !s.inside {
            hints.extend([("enter", "open"), ("⌫", "back"), ("q", "quit")]);
            return hints;
        }
        match section {
            Section::Guide => {}
            Section::Docker => hints.push(("enter", "check again")),
            Section::Locations => hints.extend([
                ("enter", "select"),
                ("a", "add"),
                ("g", "add global"),
                ("d", "remove"),
            ]),
            Section::Options | Section::Data => hints.push(("enter", "change")),
        }
        hints.extend([("⌫", "back"), ("q", "quit")]);
        hints
    }

    /// One-line explanation of the selected item.
    fn item_help(&self, section: Section, sel: usize) -> String {
        match section {
            Section::Guide => "↑↓ to scroll, ⌫ to go back to the sections.".into(),
            Section::Docker => match &self.docker {
                Some(info) => {
                    let version = info.version.as_ref().map(|v| format!(" v{v}")).unwrap_or_default();
                    let note = info.note.as_ref().map(|n| format!(": {n}")).unwrap_or_default();
                    format!("Docker{version} is {}{note}. Enter checks again.", info.status_label())
                }
                None => "Checking whether Docker is installed and running…".into(),
            },
            Section::Locations => match self.locations.get(sel) {
                Some((loc, _)) => match loc.source {
                    Source::Auto => format!("{} is always searched.", tilde(&loc.path)),
                    Source::Project => format!(
                        "{}: listed in this project's .atk.toml. Enter or d removes it.",
                        tilde(&loc.path)
                    ),
                    Source::Global => format!(
                        "{}: listed in your user settings, searched in every project. Enter or d removes it.",
                        tilde(&loc.path)
                    ),
                },
                None if sel == self.locations.len() => {
                    "Type a folder or .yml file, relative to the project root. Tab completes.".into()
                }
                None => "Type a folder or .yml file, relative to your home folder. Tab completes.".into(),
            },
            Section::Options if sel == 0 => {
                "The event workflows are triggered as. Enter cycles through push, pull_request, workflow_dispatch, schedule and release.".into()
            }
            Section::Options => "Enter to type a number. Empty means one job per CPU core.".into(),
            Section::Data if sel == 0 => match &self.store {
                Some(store) => format!("Deletes every saved run of this project from {}.", tilde(store.dir())),
                None => "This system has no user data folder, so runs aren't saved.".into(),
            },
            Section::Data => match Config::user_path() {
                Some(p) => format!("Deletes {}. Project settings in .atk.toml are kept.", tilde(&p)),
                None => "Deletes your user settings.".into(),
            },
        }
    }

    fn guide(&self) -> Vec<Line<'static>> {
        let h = |s: &str| {
            Line::styled(
                s.to_string(),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            )
        };
        let t = |s: &str| Line::raw(s.to_string());
        let kv = |k: &str, v: String| {
            Line::from(vec![
                Span::styled(format!("  {k:<20}"), theme::bold()),
                Span::raw(v),
            ])
        };
        let config = Config::user_path()
            .map(|p| tilde(&p))
            .unwrap_or_else(|| "unavailable".into());
        let history = self
            .store
            .as_ref()
            .map(|s| tilde(s.dir()))
            .unwrap_or_else(|| "unavailable".into());
        vec![
            t("ATK (Actions Toolkit) runs your CI/CD workflows on this machine, each job in"),
            t("its own Docker container, instead of on paid hosted runners."),
            t(""),
            h("Commands"),
            kv(
                "atk",
                "your workflow catalog and the history of runs".into(),
            ),
            kv("atk settings", "this page: settings and the guide".into()),
            kv(
                "atk run <workflow>",
                "run without the TUI (--detach to background it)".into(),
            ),
            kv("atk watch <id>", "follow a run until it ends".into()),
            kv("atk status <id>", "a run's jobs, steps and failures".into()),
            kv(
                "atk mcp",
                "let AI agents run, watch and review workflows".into(),
            ),
            kv("atk --help", "every command".into()),
            t(""),
            h("Getting started"),
            t("  1. Docker: install it and make sure it is running."),
            t("  2. Workflow locations: .github/workflows and .atk/workflows are always"),
            t("     searched. Add other folders or files for this project or every project."),
            t("  3. Run `atk`, open Catalog, pick a workflow or job, press Enter."),
            t(""),
            h("Keys"),
            kv("↑ ↓", "move".into()),
            kv("Enter / →", "select or open".into()),
            kv("⌫ / ←", "back (leaving the first screen quits)".into()),
            kv("q / Ctrl-C", "quit".into()),
            t(""),
            h("Confirmations and text fields"),
            t("  Deleting runs, clearing history, removing a location, cancelling a run and"),
            t("  resetting settings ask first. Cancel is selected; pick with ← → and Enter."),
            t("  In text fields, type, use Tab to complete paths, Enter to save, Esc to cancel."),
            t(""),
            h("Files"),
            kv("settings", config),
            kv("project settings", ".atk.toml in the project root".into()),
            kv(
                "secrets",
                ".secrets in the project root (NAME=value per line)".into(),
            ),
            kv("run history", history),
        ]
    }
}

fn setting_row(name: &str, value: &str, width: usize) -> Line<'static> {
    theme::row(Span::raw(" "), name, value.to_string(), width, Style::new())
}

fn location_prompt(source: Source) -> Modal {
    let (title, help) = match source {
        Source::Global => (
            "Add a location for every project",
            "Folder or .yml file searched in every project (relative to your home folder)",
        ),
        _ => (
            "Add a location for this project",
            "Folder or .yml file searched in this project (relative to the project root)",
        ),
    };
    Modal::Prompt(Prompt::new(title, help, "", Purpose::AddLocation(source)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::tests::{press, render};
    use super::super::{App, Modal, Start};
    use crate::history::Store;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn settings_open_on_the_guide() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/demo");
        let mut app = App::with_store(root, Start::Settings, None).unwrap();
        let screen = render(&mut app, 110, 30);
        assert!(screen.contains("Settings"), "{screen}");
        assert!(screen.contains("workflows "), "{screen}");
        assert!(screen.contains("Workflow locations"), "{screen}");
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
        let store = Store::at(root.join("runs"));
        let mut app = App::with_store(root.clone(), Start::Settings, Some(store)).unwrap();
        assert!(app.workflows.is_empty());

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down); // Workflow locations
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        for c in "pip".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Enter);
        let screen = render(&mut app, 110, 24);
        assert!(screen.contains("project  pipelines"), "{screen}");
        assert_eq!(app.workflows.len(), 1);
        let saved = std::fs::read_to_string(root.join(".atk.toml")).unwrap();
        assert!(saved.contains("pipelines"), "{saved}");

        let row = app
            .locations
            .iter()
            .position(|(l, _)| l.entry == "pipelines")
            .unwrap();
        for _ in 0..row {
            press(&mut app, KeyCode::Down);
        }
        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(app.modal, Some(Modal::Confirm(_))));
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Enter);
        assert!(app.workflows.is_empty());
    }
}
