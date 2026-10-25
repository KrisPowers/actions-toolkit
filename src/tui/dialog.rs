//! Popups drawn over a screen: a text prompt and a confirmation for dangerous actions.

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

use super::input::TextInput;
use super::theme::{self, BAD};

/// Asks for a line of text. `purpose` says what the answer is for.
pub struct Prompt<P> {
    pub title: String,
    pub help: String,
    pub input: TextInput,
    pub purpose: P,
    pub error: Option<String>,
}

impl<P> Prompt<P> {
    pub fn new(title: impl Into<String>, help: impl Into<String>, value: &str, purpose: P) -> Self {
        Self {
            title: title.into(),
            help: help.into(),
            input: TextInput::new(value),
            purpose,
            error: None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let popup = centered(area, 76, 8);
        f.render_widget(Clear, popup);
        let block = theme::panel(&self.title, true);
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let error = match &self.error {
            Some(e) => Line::styled(e.clone(), Style::new().fg(BAD)),
            None => Line::raw(""),
        };
        let lines = vec![
            Line::styled(self.help.clone(), theme::dim()),
            Line::raw(""),
            self.input.line(),
            error,
            Line::raw(""),
            theme::hints(&[("tab", "complete"), ("enter", "save"), ("esc", "cancel")]),
        ];
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
}

pub enum Answer {
    Pending,
    Yes,
    No,
}

/// Asks before doing something that can't be undone. "Cancel" is selected by default.
pub struct Confirm<A> {
    pub title: String,
    pub message: String,
    /// Label of the dangerous button, e.g. "Delete".
    pub verb: String,
    pub action: A,
    yes: bool,
}

impl<A> Confirm<A> {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        verb: impl Into<String>,
        action: A,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            verb: verb.into(),
            action,
            yes: false,
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Answer {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                self.yes = !self.yes;
                Answer::Pending
            }
            KeyCode::Enter if self.yes => Answer::Yes,
            KeyCode::Char('y') => Answer::Yes,
            KeyCode::Enter | KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('n') => Answer::No,
            _ => Answer::Pending,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let popup = centered(area, 60, 9);
        f.render_widget(Clear, popup);
        let block = theme::panel(&self.title, true).border_style(Style::new().fg(BAD));
        let inner = block.inner(popup);
        f.render_widget(block, popup);
        let button = |label: &str, selected: bool, danger: bool| {
            let base = if danger {
                Style::new().fg(BAD)
            } else {
                Style::new()
            };
            let style = if selected {
                Style::new()
                    .fg(Color::Black)
                    .bg(if danger { BAD } else { Color::Gray })
                    .add_modifier(Modifier::BOLD)
            } else {
                base
            };
            Span::styled(format!("  {label}  "), style)
        };
        let lines = vec![
            Line::raw(""),
            Line::raw(self.message.clone()),
            Line::styled("This can't be undone.", theme::dim()),
            Line::raw(""),
            Line::from(vec![
                button("Cancel", !self.yes, false),
                Span::raw("   "),
                button(&self.verb, self.yes, true),
            ])
            .alignment(Alignment::Center),
            Line::raw(""),
            theme::hints(&[("←→", "choose"), ("enter", "confirm"), ("esc", "cancel")])
                .alignment(Alignment::Center),
        ];
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
}

/// A `width` x `height` rectangle centred in `area` (clamped to fit).
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn confirm_defaults_to_cancel() {
        let mut c = Confirm::new("Delete run?", "Delete it?", "Delete", ());
        assert!(matches!(c.on_key(key(KeyCode::Enter)), Answer::No));
        let mut c = Confirm::new("Delete run?", "Delete it?", "Delete", ());
        assert!(matches!(c.on_key(key(KeyCode::Right)), Answer::Pending));
        assert!(matches!(c.on_key(key(KeyCode::Enter)), Answer::Yes));
        assert!(matches!(c.on_key(key(KeyCode::Backspace)), Answer::No));
    }
}
