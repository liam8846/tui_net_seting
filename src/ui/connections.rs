//! Connections tab: profile table with active/autoconnect markers.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::app::App;
use crate::model::ConnectionType;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(["名稱", "類型", "裝置", "啟用", "自動"]).style(
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.connections.items.iter().map(|c| {
        let active = if c.active {
            Span::styled("● 是", Style::default().fg(Color::Green))
        } else {
            Span::styled("  否", Style::default().fg(Color::DarkGray))
        };
        Row::new(vec![
            Cell::from(c.name.clone()),
            Cell::from(type_label(c.ctype)),
            Cell::from(c.device.clone().unwrap_or_else(|| "--".into())),
            Cell::from(active),
            Cell::from(if c.autoconnect { "是" } else { "否" }),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(7),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol("› ")
    .block(block(app));

    let mut state = TableState::default();
    if !app.connections.items.is_empty() {
        state.select(Some(app.connections.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
    if app.connections.is_empty() {
        let text = if app.loading.connections {
            "載入中…"
        } else {
            "找不到連線設定檔 — 按 a 新增或 r 重新整理"
        };
        super::widgets::empty_state(frame, area, text);
    }
}

fn type_label(t: ConnectionType) -> String {
    t.label().to_string()
}

fn block(app: &App) -> Block<'static> {
    let title = if app.loading.connections {
        " 連線 (載入中…) "
    } else {
        " 連線 "
    };
    Block::default().title(title).borders(Borders::ALL)
}
