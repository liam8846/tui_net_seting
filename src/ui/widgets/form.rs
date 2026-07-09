//! Rendering for the two modal forms: the connection editor and the hotspot
//! creator. Both share a vertical "label: value" field layout where the
//! focused row is highlighted and choice fields show `‹ value ›`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{ConnField, ConnForm, HotspotForm};
use crate::ui::widgets::centered;

const LABEL_WIDTH: usize = 16;

/// A single field row. `focused` highlights it; `enabled=false` dims it
/// (manual-only fields when the method isn't manual); `choice` wraps the value
/// in cycle arrows. All content is owned, so the line borrows nothing.
fn field_line(
    label: &str,
    value: &str,
    focused: bool,
    enabled: bool,
    choice: bool,
) -> Line<'static> {
    let marker = if focused { "› " } else { "  " };
    let shown = if choice {
        format!("‹ {value} ›")
    } else {
        value.to_string()
    };
    let value_style = if !enabled {
        Style::default().fg(Color::DarkGray)
    } else if focused {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(vec![
        Span::styled(marker, Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{label:<LABEL_WIDTH$}"),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(shown, value_style),
    ])
}

pub fn render_connection(frame: &mut Frame, form: &ConnForm) {
    let rows = form.rows();
    let title = if form.draft.is_new() {
        " Add connection ".to_string()
    } else {
        format!(" Edit \"{}\" ", form.draft.name)
    };
    let focused_field = form.focused();

    let mut lines: Vec<Line> = rows
        .iter()
        .map(|(field, enabled)| {
            let value = form.value(*field);
            let choice = matches!(
                field,
                ConnField::Type
                    | ConnField::Autoconnect
                    | ConnField::V4Method
                    | ConnField::V6Method
            );
            field_line(
                field.label(),
                &value,
                *field == focused_field,
                *enabled,
                choice,
            )
        })
        .collect();

    lines.push(Line::from(""));
    if let Some(err) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("✗ {err}"),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(hint_line());

    render_modal(frame, &title, lines);
}

pub fn render_hotspot(frame: &mut Frame, form: &HotspotForm) {
    let masked = "•".repeat(form.password.chars().count());
    let fields = [
        ("Interface", form.ifname().to_string(), form.focus == 0, true),
        ("SSID", form.ssid.clone(), form.focus == 1, false),
        ("Password", masked, form.focus == 2, false),
    ];
    let mut lines: Vec<Line> = fields
        .iter()
        .map(|(label, value, focused, choice)| {
            field_line(label, value, *focused, true, *choice)
        })
        .collect();
    lines.push(Line::from(""));
    if let Some(err) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("✗ {err}"),
            Style::default().fg(Color::Red),
        )));
    }
    lines.push(hint_line());
    render_modal(frame, " Create hotspot ", lines);
}

fn hint_line() -> Line<'static> {
    Line::from(Span::styled(
        "Tab/↑↓ field   ←/→/Space cycle   Enter submit   Esc cancel",
        Style::default().fg(Color::DarkGray),
    ))
}

fn render_modal(frame: &mut Frame, title: &str, lines: Vec<Line>) {
    let height = (lines.len() as u16 + 2).min(frame.area().height.saturating_sub(2));
    let area = centered(frame.area(), 62, height);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [body] = Layout::vertical([Constraint::Min(1)]).areas(inner);
    frame.render_widget(Paragraph::new(lines), body);
}
