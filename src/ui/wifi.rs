//! Wi-Fi tab: access-point table with signal bars, security badges and the
//! in-use marker. A header line shows the Wi-Fi radio state.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::app::App;
use crate::model::WifiAp;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let [header_area, table_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    render_header(frame, app, header_area);

    let header = Row::new(["", "SSID", "儲存", "訊號", "安全性", "頻道", "速率"]).style(
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.wifi.iter().map(|ap| {
        let ssid = if ap.is_hidden() {
            Span::styled("<隱藏>", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(ap.ssid.clone())
        };
        Row::new(vec![
            Cell::from(in_use_marker(ap)),
            Cell::from(ssid),
            Cell::from(saved_marker(app, ap)),
            Cell::from(signal_cell(ap.signal)),
            Cell::from(security_label(ap)),
            Cell::from(ap.chan.clone()),
            Cell::from(ap.rate.clone()),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Min(16),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(4),
            Constraint::Length(11),
        ],
    )
    .header(header)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol("› ")
    .block(block(app));

    let mut state = TableState::default();
    if !app.wifi.is_empty() {
        state.select(Some(app.wifi.selected));
    }
    frame.render_stateful_widget(table, table_area, &mut state);
    if app.wifi.is_empty() {
        let text = if app.loading.wifi {
            "載入中…".to_string()
        } else if !app.wifi_filter.query.is_empty() {
            format!(
                "沒有符合「{}」的 Wi-Fi 網路 — 按 Esc 清除篩選",
                app.wifi_filter.query
            )
        } else if app.radio.is_some_and(|radio| !radio.wifi) {
            "Wi-Fi 無線電已關閉 — 到 5:無線開關 分頁開啟".to_string()
        } else {
            "找不到 Wi-Fi 網路 — 按 r 重新掃描".to_string()
        };
        super::widgets::empty_state(frame, table_area, text);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let radio = match app.radio {
        Some(r) if r.wifi => Span::styled("Wi-Fi 無線電：開", Style::default().fg(Color::Green)),
        Some(_) => Span::styled(
            "Wi-Fi 無線電：關（按 5 → 切換以啟用）",
            Style::default().fg(Color::Red),
        ),
        None => Span::styled("Wi-Fi 無線電：…", Style::default().fg(Color::DarkGray)),
    };
    frame.render_widget(Paragraph::new(Line::from(radio)), area);
}

fn in_use_marker(ap: &WifiAp) -> Span<'static> {
    if ap.in_use {
        Span::styled("●", Style::default().fg(Color::Green))
    } else {
        Span::raw(" ")
    }
}

fn saved_marker(app: &App, ap: &WifiAp) -> Span<'static> {
    if app.is_wifi_saved(ap) {
        Span::styled("★", Style::default().fg(Color::DarkGray))
    } else {
        Span::raw(" ")
    }
}

/// A five-segment bar plus the numeric signal, coloured by strength.
fn signal_cell(signal: u8) -> Span<'static> {
    let filled = (signal as usize * 5 / 100).min(5);
    let bar: String = (0..5).map(|i| if i < filled { '▮' } else { '▯' }).collect();
    let color = match signal {
        70..=100 => Color::Green,
        40..=69 => Color::Yellow,
        _ => Color::Red,
    };
    Span::styled(format!("{bar} {signal:>3}"), Style::default().fg(color))
}

fn security_label(ap: &WifiAp) -> Span<'static> {
    if ap.is_secured() {
        Span::styled(ap.security.clone(), Style::default().fg(Color::Yellow))
    } else {
        Span::styled("開放", Style::default().fg(Color::DarkGray))
    }
}

fn block(app: &App) -> Block<'static> {
    let title = if app.loading.wifi {
        " 存取點 (掃描中…) "
    } else {
        " 存取點 "
    };
    Block::default().title(title).borders(Borders::ALL)
}
