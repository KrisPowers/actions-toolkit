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

pub fn panel(title: &str, focused: bool) -> Block<'static> {
    let (border, title_style) = if focused {
        (
            Style::new().fg(ACCENT),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )
    } else {
        (dim(), Style::new().fg(Color::Gray))
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(format!(" {title} "), title_style))
}

pub fn status_icon(status: Status, tick: usize) -> Span<'static> {
    match status {
        Status::Pending => Span::styled("○", dim()),
        Status::Running => Span::styled(spinner(tick), Style::new().fg(WARN)),
        Status::Success => Span::styled("✓", Style::new().fg(OK)),
        Status::Failure => Span::styled("✗", Style::new().fg(BAD).add_modifier(Modifier::BOLD)),
        Status::Skipped => Span::styled("⊘", dim()),
        Status::Cancelled => Span::styled("■", Style::new().fg(Color::Magenta)),
    }
}

/// A list row: `icon name ........ right`, fitted to `width` columns.
pub fn row(
    icon: Span<'static>,
    name: &str,
    right: String,
    width: usize,
    name_style: Style,
) -> Line<'static> {
    let right_w = right.chars().count();
    // Layout: " " icon " " name pad " " right
    let avail = width.saturating_sub(right_w + icon.width() + 3);
    let name = fit(name, avail);
    let pad = avail.saturating_sub(name.chars().count());
    Line::from(vec![
        Span::raw(" "),
        icon,
        Span::raw(" "),
        Span::styled(name, name_style),
        Span::raw(" ".repeat(pad + 1)),
        Span::styled(right, dim()),
    ])
}

/// Truncates to `width` columns with an ellipsis.
pub fn fit(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else if width == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(width - 1).collect();
        t.push('…');
        t
    }
}

/// First index to render so that `selected` stays visible in `height` rows.
pub fn scroll_start(selected: usize, len: usize, height: usize) -> usize {
    if height == 0 || len <= height {
        0
    } else {
        selected.saturating_sub(height - 1).min(len - height)
    }
}

/// Renders rows inside a panel, highlighting and keeping `selected` in view.
pub fn render_list(
    f: &mut Frame,
    area: Rect,
    block: Block,
    rows: Vec<Line<'static>>,
    selected: Option<usize>,
    focused: bool,
) {
    let inner = block.inner(area);
    f.render_widget(block, area);
    let height = inner.height as usize;
    let start = scroll_start(selected.unwrap_or(0), rows.len(), height);
    let lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(i, line)| {
            if Some(i) == selected && focused {
                line.style(selected_style())
            } else if Some(i) == selected {
                line.style(Style::new().add_modifier(Modifier::BOLD))
            } else {
                line
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn selected_style() -> Style {
    selected()
}

/// A one-line key hint bar: `key action  key action`.
pub fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (key, action) in pairs {
        spans.push(Span::styled(
            key.to_string(),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {action}   "), dim()));
    }
    Line::from(spans)
}
