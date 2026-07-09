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
    match &app.detail {
        Some(_) => {
            let [left, right] =
                Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                    .areas(area);
            render_table(frame, app, left);
            render_detail(frame, app, right);
        }
        None => render_table(frame, app, area),
    }
}

fn render_table(frame: &mut Frame, app: &App, area: Rect) {
    let header = Row::new(["裝置", "類型", "狀態", "連線"]).style(
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.devices.items.iter().map(|d| {
        Row::new(vec![
            Cell::from(d.name.clone()),
            Cell::from(type_label(&d.dtype)),
            Cell::from(Span::styled(
                d.state.label().to_string(),
                state_style(&d.state),
            )),
            Cell::from(d.connection.clone().unwrap_or_else(|| "--".into())),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol("› ")
    .block(block(app, "裝置"));

    let mut state = TableState::default();
    if !app.devices.items.is_empty() {
        state.select(Some(app.devices.selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
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
                    Style::default().fg(Color::Gray),
                ),
                Span::raw(kv.value.into_owned()),
            ])
        })
        .collect();

    let block = Block::default()
        .title(format!(" {} 詳細資訊 ", pane.device))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
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
        DeviceState::Disconnected => Color::Gray,
        DeviceState::Unavailable | DeviceState::Unmanaged => Color::DarkGray,
        DeviceState::Other(_) => Color::White,
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
