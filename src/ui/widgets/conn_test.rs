use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{ConnTestStageState, ConnTestState};
use crate::event::ConnTestStage;

use super::{centered, spinner};

pub fn render(frame: &mut Frame, state: &ConnTestState, tick: u64) {
    let area = centered(frame.area(), 68, 11);
    frame.render_widget(Clear, area);

    let mut lines = ConnTestStage::ALL
        .into_iter()
        .map(|stage| stage_line(stage, state.stage(stage), tick))
        .collect::<Vec<_>>();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if state.is_complete() {
            "r 重新測試  Esc 關閉"
        } else {
            "Esc 關閉"
        },
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .title(" 連線測試 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn stage_line(stage: ConnTestStage, state: &ConnTestStageState, tick: u64) -> Line<'static> {
    let (symbol, color, message) = match state {
        ConnTestStageState::Pending => ("·".to_string(), Color::DarkGray, "待執行".into()),
        ConnTestStageState::Running => (
            spinner::frame(tick).to_string(),
            Color::Cyan,
            "測試中…".into(),
        ),
        ConnTestStageState::Passed(message) => ("✓".into(), Color::Green, message.clone()),
        ConnTestStageState::Failed(message) => ("✗".into(), Color::Red, message.clone()),
        ConnTestStageState::Skipped(message) => ("○".into(), Color::DarkGray, message.clone()),
    };
    Line::from(vec![
        Span::styled(
            format!(" {symbol} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<16}", stage_label(stage)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(message, Style::default().fg(color)),
    ])
}

fn stage_label(stage: ConnTestStage) -> &'static str {
    match stage {
        ConnTestStage::Gateway => "LAN（閘道）",
        ConnTestStage::Internet => "網際網路",
        ConnTestStage::Dns => "DNS 解析",
        ConnTestStage::Wifi => "Wi-Fi 無線狀態",
    }
}
