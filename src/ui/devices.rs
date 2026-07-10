//! Devices tab: a status table with an optional detail side-pane parsed at
//! draw time straight from the stored raw buffer (fully zero-copy).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};

use crate::app::App;
use crate::model::{DeviceState, DeviceType};
use crate::parse::device::parse_kv_lines;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let areas = pane_areas(area, app.detail.is_some());
    if let Some(table) = areas.table {
        render_table(frame, app, table);
    }
    if let Some(detail) = areas.detail {
        render_detail(frame, app, detail);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PaneAreas {
    pub table: Option<Rect>,
    pub detail: Option<Rect>,
}

pub(crate) fn pane_areas(area: Rect, detail_open: bool) -> PaneAreas {
    if !detail_open {
        return PaneAreas {
            table: Some(area),
            detail: None,
        };
    }
    if area.width < 80 {
        return PaneAreas {
            table: None,
            detail: Some(area),
        };
    }
    let [table, detail] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);
    PaneAreas {
        table: Some(table),
        detail: Some(detail),
    }
}

fn render_table(frame: &mut Frame, app: &App, area: Rect) {
    let compact = area.width < 80;
    let header = if compact {
        Row::new(["裝置", "狀態", "連線"])
    } else {
        Row::new(["裝置", "類型", "狀態", "連線"])
    }
    .style(super::widgets::table_header_style());

    let rows = app.devices.items.iter().map(|d| {
        let name = Cell::from(d.name.clone());
        let state = Cell::from(Span::styled(
            d.state.label().to_string(),
            state_style(&d.state),
        ));
        let connection = Cell::from(d.connection.clone().unwrap_or_else(|| "--".into()));
        if compact {
            Row::new(vec![name, state, connection])
        } else {
            Row::new(vec![
                name,
                Cell::from(type_label(&d.dtype)),
                state,
                connection,
            ])
        }
    });

    let widths = if compact {
        vec![
            Constraint::Min(12),
            Constraint::Length(14),
            Constraint::Length(16),
        ]
    } else {
        vec![
            Constraint::Min(14),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Length(18),
        ]
    };
    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ")
        .block(block(app, "裝置"));

    let mut state = TableState::default();
    if !app.devices.items.is_empty() {
        state.select(Some(app.devices.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
    if app.devices.is_empty() {
        let text = if app.loading.devices {
            "載入中…"
        } else {
            "找不到網路裝置 — 按 r 重新整理"
        };
        super::widgets::empty_state(frame, area, text);
    }
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let Some(pane) = &app.detail else { return };
    // Reparse the stored buffer on every draw — a few microseconds, no alloc.
    let lines: Vec<Line> = parse_kv_lines(&pane.raw)
        .filter(|kv| is_interesting(kv.key))
        .map(|kv| {
            Line::from(vec![
                Span::styled(
                    format!("{:<22}", pretty_key(kv.key)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(kv.value.into_owned()),
            ])
        })
        .collect();

    let viewport = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(viewport) as u16;
    let scroll = app.detail_scroll.min(max_scroll);
    let remaining = lines
        .len()
        .saturating_sub(scroll as usize)
        .saturating_sub(viewport);
    let hint = if remaining > 0 {
        format!(" J↓ 還有 {remaining} 行")
    } else if scroll > 0 {
        " K↑ 返回上方".to_string()
    } else {
        String::new()
    };
    let block = Block::default()
        .title(Line::from(vec![
            Span::raw(format!(" {} 詳細資訊 ", pane.device)),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Keep the fields users care about; the full `device show` dump is noisy.
fn is_interesting(key: &str) -> bool {
    const KEEP: [&str; 9] = [
        "GENERAL.DEVICE",
        "GENERAL.TYPE",
        "GENERAL.HWADDR",
        "GENERAL.MTU",
        "GENERAL.STATE",
        "GENERAL.CONNECTION",
        "CAPABILITIES.SPEED",
        "IP4.GATEWAY",
        "IP6.GATEWAY",
    ];
    KEEP.contains(&key)
        || key.starts_with("IP4.ADDRESS")
        || key.starts_with("IP4.DNS")
        || key.starts_with("IP6.ADDRESS")
}

fn pretty_key(key: &str) -> &str {
    key.split_once('.').map(|(_, k)| k).unwrap_or(key)
}

fn type_label(t: &DeviceType) -> String {
    t.label().to_string()
}

fn state_style(state: &DeviceState) -> Style {
    let color = match state {
        DeviceState::Connected => Color::Green,
        DeviceState::Connecting | DeviceState::Deactivating => Color::Yellow,
        DeviceState::Disconnected => Color::DarkGray,
        DeviceState::Unavailable | DeviceState::Unmanaged => Color::DarkGray,
        DeviceState::Other(_) => return Style::default(),
    };
    Style::default().fg(color)
}

fn block(app: &App, title: &str) -> Block<'static> {
    let mut t = format!(" {title} ");
    if app.loading.devices {
        t = format!(" {title} (載入中…) ");
    }
    Block::default().title(t).borders(Borders::ALL)
}
