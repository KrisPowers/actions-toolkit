//! Colours, symbols and small rendering helpers shared by the TUI screens.
//! Only the 16 basic colours are used so every terminal (including legacy Windows consoles)
//! renders the same palette.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::runner::Status;

pub const ACCENT: Color = Color::Cyan;
pub const DIM: Color = Color::DarkGray;
pub const OK: Color = Color::Green;
pub const WARN: Color = Color::Yellow;
pub const BAD: Color = Color::Red;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn spinner(tick: usize) -> &'static str {
    SPINNER[tick % SPINNER.len()]
}

pub fn dim() -> Style {
    Style::new().fg(DIM)
}

pub fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

pub fn selected() -> Style {
    Style::new()
        .bg(Color::Indexed(236))
        .add_modifier(Modifier::BOLD)
}
