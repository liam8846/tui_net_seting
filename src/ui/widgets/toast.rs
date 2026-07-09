//! The most-recent toast, rendered as a coloured span in the status bar.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::app::{Toast, ToastLevel};

pub fn span(toast: &Toast) -> Span<'_> {
    let (color, mark) = match toast.level {
        ToastLevel::Info => (Color::Cyan, "•"),
        ToastLevel::Success => (Color::Green, "✓"),
        ToastLevel::Error => (Color::Red, "✗"),
    };
    Span::styled(
        format!("{mark} {}", toast.text),
        Style::default().fg(color),
    )
}
