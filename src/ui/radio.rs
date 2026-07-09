//! Radio tab: two toggle rows for global networking and the Wi-Fi radio.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let (networking, wifi) = match app.radio {
        Some(r) => (Some(r.networking), Some(r.wifi)),
        None => (None, None),
    };

    let lines = vec![
        Line::from(""),
        toggle_row("Networking", networking, app.radio_sel == 0),
        Line::from(""),
        toggle_row("Wi-Fi radio", wifi, app.radio_sel == 1),
        Line::from(""),
        Line::from(Span::styled(
            "  ↑/↓ select   Enter/Space toggle",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default().title(" Radio ").borders(Borders::ALL);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn toggle_row(label: &str, state: Option<bool>, selected: bool) -> Line<'static> {
    let marker = if selected { "› " } else { "  " };
    let (text, color) = match state {
        Some(true) => ("[ on  ]", Color::Green),
        Some(false) => ("[ off ]", Color::Red),
        None => ("[ ... ]", Color::DarkGray),
    };
    let label_style = if selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(marker, Style::default().fg(Color::Cyan)),
        Span::styled(format!("{label:<14}"), label_style),
        Span::styled(text.to_string(), Style::default().fg(color)),
    ])
}
