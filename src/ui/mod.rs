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

#[derive(Debug, Clone, Copy)]
pub(crate) struct RootAreas {
    pub tabs: Rect,
    pub body: Rect,
    pub status: Rect,
}

pub(crate) fn root_areas(area: Rect) -> RootAreas {
    let [tabs, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(area);
    RootAreas { tabs, body, status }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let areas = root_areas(frame.area());

    render_tabs(frame, app, areas.tabs);
    render_body(frame, app, areas.body);
    render_status(frame, app, areas.status);

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
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .padding("", "")
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
    let line = if app.tab == Tab::Wifi && app.wifi_filter.editing {
        Line::from(Span::styled(
            format!("/{}", app.wifi_filter.query),
            Style::default().fg(Color::Cyan),
        ))
    } else if let Some(inflight) = &app.in_flight {
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
    } else if app.tab == Tab::Wifi && !app.wifi_filter.query.is_empty() {
        Line::from(Span::styled(
            format!("篩選：/{}（按 / 編輯，Esc 清除）", app.wifi_filter.query),
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

    frame.render_widget(Paragraph::new(hint_line(app, hints.width)), hints);
}

fn hint_line(app: &App, width: u16) -> Line<'static> {
    let mut bindings = vec![("?", "說明"), ("q", "離開")];
    bindings.extend_from_slice(keys::hints(app));

    let mut spans = Vec::new();
    let mut used = 0usize;
    for (key, label) in bindings {
        let group = vec![
            Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" {label}   ")),
        ];
        let group_width = Line::from(group.clone()).width();
        if used.saturating_add(group_width) > usize::from(width) {
            break;
        }
        used = used.saturating_add(group_width);
        spans.extend(group);
    }
    Line::from(spans)
}

fn render_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    match modal {
        Modal::Help => render_help(frame, app),
        Modal::Error(msg) => render_error(frame, msg),
        Modal::Confirm { prompt, .. } => widgets::confirm::render(frame, prompt),
        Modal::Password(p) => widgets::password::render(frame, p),
        Modal::ConnForm(f) => widgets::form::render_connection(frame, f),
        Modal::HotspotForm(f) => widgets::form::render_hotspot(frame, f),
        Modal::ConnTest(state) => widgets::conn_test::render(frame, state, app.tick),
    }
}

fn render_help(frame: &mut Frame, app: &App) {
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
    let area = help_area(frame.area(), lines.len());
    frame.render_widget(Clear, area);
    let viewport = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(viewport) as u16;
    let scroll = app.help_scroll.min(max_scroll);
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
            Span::raw(" 說明（按 ? 或 Esc 關閉） "),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

pub(crate) fn help_area(screen: Rect, line_count: usize) -> Rect {
    let content_height = u16::try_from(line_count).unwrap_or(u16::MAX);
    let height = content_height
        .saturating_add(2)
        .min(screen.height.saturating_sub(2));
    widgets::centered(screen, 60, height)
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

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::{draw, hint_line};
    use crate::app::{App, ConnTestStageState, ConnTestState, DetailPane, Modal, PasswordPrompt};
    use crate::event::{Action, Msg, Tab};
    use crate::model::{Device, DeviceState, DeviceType};

    #[test]
    fn hint_line_keeps_complete_priority_groups_when_narrow() {
        let app = App::new();
        let line = hint_line(&app, 60);
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(line.width() <= 60);
        assert_eq!(line.spans.len() % 2, 0);
        assert!(rendered.contains("?  說明"));
        assert!(rendered.contains("q  離開"));
    }

    #[test]
    fn all_tabs_render_at_supported_small_sizes() {
        for (width, height) in [(80, 24), (60, 16)] {
            for tab in Tab::ALL {
                let backend = TestBackend::new(width, height);
                let mut terminal = Terminal::new(backend).expect("test terminal");
                let mut app = App::new();
                app.tab = tab;
                terminal.draw(|frame| draw(frame, &app)).expect("draw tab");
            }
        }
    }

    #[test]
    fn narrow_detail_overlay_and_help_render_without_panicking() {
        let backend = TestBackend::new(60, 16);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::new();
        app.detail = Some(DetailPane {
            device: "eth0".into(),
            raw: "GENERAL.DEVICE:eth0\nIP4.ADDRESS[1]:192.0.2.2/24\n".into(),
        });
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw detail overlay");

        app.modal = Some(Modal::Help);
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw help overlay");

        let size = terminal.size().expect("terminal size");
        assert_eq!((size.width, size.height), (60, 16));
    }

    #[test]
    fn password_modal_renders_masked_and_visible_input() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::new();
        app.modal = Some(Modal::Password(PasswordPrompt {
            ssid: "測試網路".into(),
            input: String::new(),
            show_password: false,
            error: None,
        }));
        for character in "secret42".chars() {
            app.apply(Action::FormInput(character));
        }

        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw masked password modal");
        let masked = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(masked.matches('*').count(), 8);
        assert!(masked.contains("********▌"));

        app.apply(Action::TogglePasswordVisibility);
        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw visible password modal");
        let visible = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(visible.contains("secret42▌"));
    }

    #[test]
    fn focused_connection_interface_renders_as_device_choice() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::new();
        app.on_msg(Msg::Devices(Ok(vec![Device {
            name: "eth0".into(),
            dtype: DeviceType::Ethernet,
            state: DeviceState::Disconnected,
            connection: None,
        }])));
        app.apply(Action::ConnAdd);
        if let Some(Modal::ConnForm(form)) = &mut app.modal {
            form.focus = 2;
        }
        app.apply(Action::FormCycle);

        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw connection form");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let compact = rendered.replace(' ', "");

        assert!(rendered.contains('‹'));
        assert!(rendered.contains('›'));
        assert!(compact.contains("eth0（乙太網）"));
    }

    #[test]
    fn connection_test_modal_renders_all_stages_and_result_symbols() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App::new();
        app.modal = Some(Modal::ConnTest(ConnTestState {
            run_id: 1,
            gateway: ConnTestStageState::Passed("閘道回應正常".into()),
            internet: ConnTestStageState::Failed("無法連上網際網路".into()),
            dns: ConnTestStageState::Pending,
            wifi: ConnTestStageState::Skipped("略過：沒有 Wi-Fi 裝置".into()),
        }));

        terminal
            .draw(|frame| draw(frame, &app))
            .expect("draw connection test modal");
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let compact = rendered.replace(' ', "");

        assert!(compact.contains("連線測試"));
        assert!(compact.contains("LAN（閘道）"));
        assert!(compact.contains("網際網路"));
        assert!(compact.contains("DNS解析"));
        assert!(compact.contains("Wi-Fi無線狀態"));
        assert!(rendered.contains('✓'));
        assert!(rendered.contains('✗'));
    }
}
