//! A single-line text input with cursor movement and file path completion.

use std::fs;
use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme;

pub enum InputAction {
    None,
    Submit,
    Cancel,
}

#[derive(Default)]
pub struct TextInput {
    text: String,
    /// Cursor position in chars.
    cursor: usize,
}

impl TextInput {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.chars().count(),
        }
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    fn byte(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    /// Handles a key. `complete_base`, when set, enables Tab completion of paths relative to it.
    pub fn on_key(&mut self, key: KeyEvent, complete_base: Option<&Path>) -> InputAction {
        let len = self.text.chars().count();
        match key.code {
            KeyCode::Enter => return InputAction::Submit,
            KeyCode::Esc => return InputAction::Cancel,
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) => {
                let at = self.byte(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Delete if self.cursor < len => {
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(len),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = len,
            KeyCode::Tab => {
                if let Some(base) = complete_base {
                    if let Some(done) = complete_path(&self.text, base) {
                        self.cursor = done.chars().count();
                        self.text = done;
                    }
                }
            }
            _ => {}
        }
        InputAction::None
    }

    /// `> text` with a block cursor.
    pub fn line(&self) -> Line<'static> {
        let at = self.byte(self.cursor);
        let (before, rest) = self.text.split_at(at);
        let mut chars = rest.chars();
        let under = chars.next().map(String::from).unwrap_or_else(|| " ".into());
        Line::from(vec![
            Span::styled("> ", Style::new().fg(theme::ACCENT)),
            Span::raw(before.to_string()),
            Span::styled(under, Style::new().add_modifier(Modifier::REVERSED)),
            Span::raw(chars.collect::<String>()),
        ])
    }
}

/// Completes the last path segment of `text` against the file system: a unique match is
/// filled in (with a trailing `/` for folders), several matches extend to their common prefix.
pub fn complete_path(text: &str, base: &Path) -> Option<String> {
    let split = text.rfind(['/', '\\']).map(|i| i + 1).unwrap_or(0);
    let (dir_part, prefix) = text.split_at(split);
    let dir = crate::workflow::resolve(if dir_part.is_empty() { "." } else { dir_part }, base);
    let lower = prefix.to_lowercase();
    let mut matches: Vec<(String, bool)> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.path().is_dir();
            let starts = name.to_lowercase().starts_with(&lower);
            // Hidden entries only when asked for, so `.github` needs a leading dot.
            let visible = !name.starts_with('.') || prefix.starts_with('.');
            (starts && visible).then_some((name, is_dir))
        })
        .collect();
    matches.sort();
    match matches.as_slice() {
        [] => None,
        [(name, is_dir)] => Some(format!(
            "{dir_part}{name}{}",
            if *is_dir { "/" } else { "" }
        )),
        [(first, _), rest @ ..] => {
            let mut common: String = first.clone();
            for (name, _) in rest {
                let n = common
                    .chars()
                    .zip(name.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                common = common.chars().take(n).collect();
            }
            (common.chars().count() > prefix.chars().count()).then(|| format!("{dir_part}{common}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEvent;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn edits_at_the_cursor() {
        let mut input = TextInput::new("ac");
        input.on_key(key(KeyCode::Left), None);
        input.on_key(key(KeyCode::Char('b')), None);
        assert_eq!(input.value(), "abc");
        input.on_key(key(KeyCode::End), None);
        input.on_key(key(KeyCode::Backspace), None);
        assert_eq!(input.value(), "ab");
    }

    #[test]
    fn completes_paths() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pipelines/deploy")).unwrap();
        fs::write(dir.path().join("pipelines/ci.yml"), "").unwrap();
        fs::write(dir.path().join("pipelines/cd.yml"), "").unwrap();
        assert_eq!(
            complete_path("pip", dir.path()).as_deref(),
            Some("pipelines/")
        );
        assert_eq!(
            complete_path("pipelines/de", dir.path()).as_deref(),
            Some("pipelines/deploy/")
        );
        assert_eq!(complete_path("pipelines/c", dir.path()), None);
        assert_eq!(
            complete_path("pipelines/ci", dir.path()).as_deref(),
            Some("pipelines/ci.yml")
        );
    }
}
