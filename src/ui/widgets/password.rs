//! Masked Wi-Fi password prompt modal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::PasswordPrompt;
use crate::ui::widgets::centered;

pub fn render(frame: &mut Frame, prompt: &PasswordPrompt) {
    let area = centered(frame.area(), 56, if prompt.error.is_some() { 8 } else { 7 });
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" 連接到「{}」 ", prompt.ssid))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut constraints = vec![
        Constraint::Length(1), // label
        Constraint::Length(1), // input
        Constraint::Length(1), // spacer / error
    ];
    if prompt.error.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1)); // hint
    let rows = Layout::vertical(constraints).split(inner);

    frame.render_widget(Paragraph::new("密碼："), rows[0]);
    let masked = "•".repeat(prompt.input.chars().count());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(masked, Style::default().fg(Color::White)),
            Span::styled("▌", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]))
        .block(Block::default().borders(Borders::BOTTOM)),
        rows[1],
    );
    if let Some(err) = &prompt.error {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("✗ {err}"),
                Style::default().fg(Color::Red),
            )),
            rows[3],
        );
    }
    let hint_row = rows.len() - 1;
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Enter 連接   Esc 取消",
            Style::default().fg(Color::DarkGray),
        )),
        rows[hint_row],
    );
}
