//! Colours, symbols and small rendering helpers shared by the TUI screens.
//! Only the 16 basic colours are used so every terminal (including legacy Windows consoles)
//! renders the same palette.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use crate::runner::Status;
