//! Yes/No confirmation modal.

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::ui::widgets::centered;

pub fn render(frame: &mut Frame, prompt: &str) {
    let area = centered(frame.area(), 54, 7);
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from(prompt),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", Style::default().fg(Color::Green)),
            Span::raw(" yes    "),
            Span::styled("[n]", Style::default().fg(Color::Red)),
            Span::raw(" no"),
        ]),
    ];
    let block = Block::default()
        .title(" Confirm ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}
