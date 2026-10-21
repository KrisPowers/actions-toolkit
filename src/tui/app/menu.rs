//! `maverick workflows`: choose between the workflow catalog and the run history.

use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{catalog::Catalog, history::History, App, Nav, Screen};
use crate::history::Store;
use crate::tui::dialog::centered;
use crate::tui::theme::{self, ACCENT};

pub struct Menu {
    sel: usize,
    /// Saved runs for this project (`None` without a history folder).
    pub runs: Option<usize>,
}

impl Menu {
    pub fn new(store: Option<&Store>, root: &Path) -> Self {
        Self {
            sel: 0,
            runs: store.map(|s| s.list(root).len()),
        }
    }
}

const OPTIONS: [(&str, &str); 2] = [
    (
        "Catalog",
        "Browse the workflows in this project and run them",
    ),
    (
        "Runs",
        "Past runs: reopen their logs, run them again or delete them",
    ),
];

impl App {
    pub(super) fn menu_key(&mut self, menu: &mut Menu, key: KeyEvent) -> Nav {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => menu.sel = menu.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => menu.sel = (menu.sel + 1).min(OPTIONS.len() - 1),
            KeyCode::Enter | KeyCode::Right => {
                return Nav::Push(match menu.sel {
                    0 => Screen::Catalog(Catalog::new(&self.workflows)),
                    _ => Screen::History(History::load(self.store.as_ref(), &self.root)),
                })
            }
            KeyCode::Backspace | KeyCode::Esc | KeyCode::Left => return Nav::Back,
            _ => {}
        }
        Nav::Stay
    }

    pub(super) fn draw_menu(
        &self,
        menu: &Menu,
        f: &mut Frame,
        area: Rect,
    ) -> Vec<(&'static str, &'static str)> {
        let runs = menu.runs;
        let workflows = self
            .workflows
            .iter()
            .filter(|w| w.workflow().is_some())
            .count();
        let counts = [
            plural(workflows, "workflow"),
            runs.map(|n| plural(n, "run"))
                .unwrap_or_else(|| "no history folder".into()),
        ];
        let card = centered(area, 64, 12);
        let block = theme::panel("Workflows", true);
        let inner = block.inner(card);
        f.render_widget(block, card);
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(4),
        ])
        .split(inner);
        for (i, ((title, help), count)) in OPTIONS.iter().zip(counts).enumerate() {
            let selected = i == menu.sel;
            let marker = if selected { "▸ " } else { "  " };
            let title_style = if selected {
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                theme::bold()
            };
            let lines = vec![
                Line::from(vec![
                    Span::styled(format!(" {marker}"), Style::new().fg(ACCENT)),
                    Span::styled(title.to_string(), title_style),
                    Span::styled(format!("   {count}"), theme::dim()),
                ]),
                Line::styled(format!("    {help}"), theme::dim()),
            ];
            let mut p = Paragraph::new(lines);
            if selected {
                p = p.style(theme::selected());
            }
            let area = rows[i + 1];
            f.render_widget(p, Rect { height: 2, ..area });
        }
        vec![
            ("↑↓", "choose"),
            ("enter", "open"),
            ("⌫", "back"),
            ("q", "quit"),
        ]
    }
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}
