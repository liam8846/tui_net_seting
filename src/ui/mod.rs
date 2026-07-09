//! Rendering root. `draw` is the single entry point; it never mutates `App`
//! and performs no I/O. Layout is tab bar / active screen / status bar, with
//! modal overlays drawn last.

pub mod connections;
pub mod devices;
pub mod hotspot;
pub mod keys;
pub mod radio;
pub mod widgets;
pub mod wifi;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};

use crate::app::{App, Modal};
use crate::event::Tab;

pub fn draw(frame: &mut Frame, app: &App) {
    let [tabs_area, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    render_tabs(frame, app, tabs_area);
    render_body(frame, app, body);
    render_status(frame, app, status);

    if let Some(modal) = &app.modal {
        render_modal(frame, app, modal);
    }
}

fn render_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| Line::from(format!(" {}:{} ", i + 1, t.title())));
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("");
    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame, app: &App, area: Rect) {
    match app.tab {
        Tab::Devices => devices::render(frame, app, area),
        Tab::Connections => connections::render(frame, app, area),
        Tab::Wifi => wifi::render(frame, app, area),
        Tab::Hotspot => hotspot::render(frame, app, area),
        Tab::Radio => radio::render(frame, app, area),
    }
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let [top, hints] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    // Top line: spinner + in-flight label, else the most recent toast.
    let line = if let Some(inflight) = &app.in_flight {
        Line::from(vec![
            Span::styled(
                format!("{} ", widgets::spinner::frame(app.tick)),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(inflight.label.clone()),
        ])
    } else if let Some(toast) = app.current_toast() {
        Line::from(widgets::toast::span(toast))
    } else if app.loading.any() {
        Line::from(Span::styled(
            format!("{} 載入中…", widgets::spinner::frame(app.tick)),
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            if app.monitor_ok {
                "就緒"
            } else {
                "就緒（變更監視已停止，正在輪詢）"
            },
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(line), top);

    // Hint line: per-tab keys + globals.
    let mut spans = Vec::new();
    for (k, label) in keys::hints(app.tab) {
        spans.push(Span::styled(
            format!(" {k} "),
            Style::default().fg(Color::Black).bg(Color::Gray),
        ));
        spans.push(Span::raw(format!(" {label}   ")));
    }
    spans.push(Span::styled(
        " ? ",
        Style::default().fg(Color::Black).bg(Color::Gray),
    ));
    spans.push(Span::raw(" 說明   "));
    spans.push(Span::styled(
        " q ",
        Style::default().fg(Color::Black).bg(Color::Gray),
    ));
    spans.push(Span::raw(" 離開"));
    frame.render_widget(Paragraph::new(Line::from(spans)), hints);
}

fn render_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    match modal {
        Modal::Help => render_help(frame),
        Modal::Error(msg) => render_error(frame, msg),
        Modal::Confirm { prompt, .. } => widgets::confirm::render(frame, prompt),
        Modal::Password(p) => widgets::password::render(frame, p),
        Modal::ConnForm(f) => widgets::form::render_connection(frame, f),
        Modal::HotspotForm(f) => widgets::form::render_hotspot(frame, f),
    }
    let _ = app;
}

fn render_help(frame: &mut Frame) {
    let lines: Vec<Line> = keys::help_lines()
        .into_iter()
        .map(|(k, v)| {
            if k.is_empty() && v.is_empty() {
                Line::from("")
            } else {
                Line::from(vec![
                    Span::styled(format!("  {k:<22}"), Style::default().fg(Color::Cyan)),
                    Span::raw(v),
                ])
            }
        })
        .collect();
    let height = (lines.len() as u16 + 2).min(frame.area().height.saturating_sub(2));
    let area = widgets::centered(frame.area(), 60, height);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" 說明（按 ? 或 Esc 關閉） ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_error(frame: &mut Frame, msg: &str) {
    let area = widgets::centered(frame.area(), 60, 9);
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled(
            "NetworkManager 無法使用",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(msg.to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "r 重試    q 離開",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let block = Block::default()
        .title(" 錯誤 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
