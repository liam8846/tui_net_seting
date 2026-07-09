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
            let status = if c.active { "使用中" } else { "未啟用" };
            let color = if c.active { Color::Green } else { Color::Gray };
            lines.push(Line::from(vec![
                Span::raw("熱點設定檔："),
                Span::styled(
                    c.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  ("),
                Span::styled(status, Style::default().fg(color)),
                Span::raw(")"),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "按 x 停止並移除熱點。",
                Style::default().fg(Color::Gray),
            )));
        }
        None => {
            lines.push(Line::from("目前沒有執行中的熱點。"));
            lines.push(Line::from(""));
            let wifi = app.wifi_device_names();
            if wifi.is_empty() {
                lines.push(Line::from(Span::styled(
                    "沒有可用的 wifi 裝置 - 熱點需要一個 wifi 裝置。",
                    Style::default().fg(Color::Red),
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("可用的 wifi 介面："),
                    Span::styled(wifi.join(", "), Style::default().fg(Color::Cyan)),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "按 a 建立熱點。",
                    Style::default().fg(Color::Gray),
                )));
            }
        }
    }

    let block = Block::default().title(" 熱點 ").borders(Borders::ALL);
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
