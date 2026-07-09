//! Hotspot tab: shows the active hotspot profile (if any) and how to create
//! or stop one. The creation form itself is a modal (see `widgets::form`).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    match app.hotspot_profile() {
        Some(c) => {
            let status = if c.active { "active" } else { "inactive" };
            let color = if c.active { Color::Green } else { Color::Gray };
            lines.push(Line::from(vec![
                Span::raw("Hotspot profile: "),
                Span::styled(c.name.clone(), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  ("),
                Span::styled(status, Style::default().fg(color)),
                Span::raw(")"),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press x to stop and remove the hotspot.",
                Style::default().fg(Color::Gray),
            )));
        }
        None => {
            lines.push(Line::from("No hotspot is running."));
            lines.push(Line::from(""));
            let wifi = app.wifi_device_names();
            if wifi.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No wifi device available — a hotspot needs one.",
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("Available wifi interfaces: "),
                    Span::styled(wifi.join(", "), Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Press a to create a hotspot.",
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }

    let block = Block::default().title(" Hotspot ").borders(Borders::ALL);
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
