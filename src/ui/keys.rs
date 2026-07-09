//! The single source of truth mapping key events to [`Action`]s. Dispatch is
//! modal-first (an open modal owns all input), then per-tab. The context hint
//! bar and help overlay are generated from the same [`hints`] tables so they
//! never drift from the actual bindings.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Modal};
use crate::event::{Action, Tab};

/// Translate a crossterm event into an [`Action`], or `None` to ignore it.
pub fn translate(event: &crossterm::event::Event, app: &App) -> Option<Action> {
    let crossterm::event::Event::Key(key) = event else {
        return None;
    };
    // crossterm on Windows emits both Press and Release; act on Press only.
    if key.kind == KeyEventKind::Release {
        return None;
    }
    match &app.modal {
        Some(Modal::Help) => help_keys(key),
        Some(Modal::Error(_)) => error_keys(key),
        Some(Modal::Confirm { .. }) => confirm_keys(key),
        Some(Modal::Password(_)) => text_modal_keys(key),
        Some(Modal::ConnForm(_)) => text_modal_keys(key),
        Some(Modal::HotspotForm(_)) => text_modal_keys(key),
        None => root_keys(key, app),
    }
}

fn help_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => Some(Action::ModalCancel),
        _ => None,
    }
}

fn error_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('r') => Some(Action::ErrorRetry),
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        _ => None,
    }
}

fn confirm_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Some(Action::ConfirmYes),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::ConfirmNo),
        _ => None,
    }
}

/// Shared editing keys for the password / connection / hotspot forms.
fn text_modal_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ModalCancel),
        KeyCode::Enter => Some(Action::FormSubmit),
        KeyCode::Tab | KeyCode::Down => Some(Action::FormNext),
        KeyCode::BackTab | KeyCode::Up => Some(Action::FormPrev),
        KeyCode::Left | KeyCode::Right => Some(Action::FormCycle),
        KeyCode::Backspace => Some(Action::FormBackspace),
        KeyCode::Char(c) => Some(Action::FormInput(c)),
        _ => None,
    }
}

fn root_keys(key: &KeyEvent, app: &App) -> Option<Action> {
    // Global bindings first.
    if let Some(a) = global_keys(key) {
        return Some(a);
    }
    match app.tab {
        Tab::Devices => devices_keys(key),
        Tab::Connections => connections_keys(key),
        Tab::Wifi => wifi_keys(key),
        Tab::Hotspot => hotspot_keys(key),
        Tab::Radio => radio_keys(key),
    }
}

fn global_keys(key: &KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('c') if ctrl => Some(Action::Quit),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Tab => Some(Action::NextTab),
        KeyCode::BackTab => Some(Action::PrevTab),
        KeyCode::Char('1') => Some(Action::GotoTab(Tab::Devices)),
        KeyCode::Char('2') => Some(Action::GotoTab(Tab::Connections)),
        KeyCode::Char('3') => Some(Action::GotoTab(Tab::Wifi)),
        KeyCode::Char('4') => Some(Action::GotoTab(Tab::Hotspot)),
        KeyCode::Char('5') => Some(Action::GotoTab(Tab::Radio)),
        KeyCode::Char('r') => Some(Action::Refresh),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Down),
        KeyCode::Esc => Some(Action::Back),
        _ => None,
    }
}

fn devices_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter | KeyCode::Char('c') => Some(Action::DeviceConnect),
        KeyCode::Char('d') => Some(Action::DeviceDisconnect),
        KeyCode::Char('s') => Some(Action::ShowDetails),
        _ => None,
    }
}

fn connections_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::ConnActivate),
        KeyCode::Char('d') => Some(Action::ConnDeactivate),
        KeyCode::Char('a') => Some(Action::ConnAdd),
        KeyCode::Char('e') => Some(Action::ConnEdit),
        KeyCode::Char('x') | KeyCode::Delete => Some(Action::ConnDelete),
        _ => None,
    }
}

fn wifi_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter => Some(Action::WifiConnect),
        KeyCode::Char('d') => Some(Action::WifiDisconnect),
        KeyCode::Char('f') => Some(Action::WifiForget),
        _ => None,
    }
}

fn hotspot_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('a') | KeyCode::Enter => Some(Action::HotspotCreate),
        KeyCode::Char('x') => Some(Action::HotspotStop),
        _ => None,
    }
}

fn radio_keys(key: &KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Enter | KeyCode::Char(' ') => Some(Action::RadioToggle),
        _ => None,
    }
}

/// Context key hints for the current tab, rendered in the status bar.
pub fn hints(tab: Tab) -> &'static [(&'static str, &'static str)] {
    match tab {
        Tab::Devices => &[
            ("Enter", "connect"),
            ("d", "disconnect"),
            ("s", "details"),
            ("r", "refresh"),
        ],
        Tab::Connections => &[
            ("Enter", "activate"),
            ("d", "deactivate"),
            ("a", "add"),
            ("e", "edit"),
            ("x", "delete"),
        ],
        Tab::Wifi => &[
            ("Enter", "connect"),
            ("d", "disconnect"),
            ("f", "forget"),
            ("r", "rescan"),
        ],
        Tab::Hotspot => &[("a", "create"), ("x", "stop")],
        Tab::Radio => &[("Enter", "toggle")],
    }
}

/// Full binding list for the help overlay.
pub fn help_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        ("1-5 / Tab", "switch tabs"),
        ("j / k / ↑ / ↓", "move selection"),
        ("r", "refresh (rescan on Wi-Fi)"),
        ("?", "toggle this help"),
        ("q / Ctrl-C", "quit"),
        ("Esc", "close pane / cancel modal"),
        ("", ""),
        ("Devices — Enter/c", "connect device"),
        ("Devices — d", "disconnect device"),
        ("Devices — s", "toggle details pane"),
        ("Connections — Enter", "activate profile"),
        ("Connections — d", "deactivate profile"),
        ("Connections — a / e", "add / edit profile"),
        ("Connections — x / Del", "delete profile"),
        ("Wi-Fi — Enter", "connect (prompts for password)"),
        ("Wi-Fi — d / f", "disconnect / forget"),
        ("Hotspot — a / x", "create / stop hotspot"),
        ("Radio — Enter", "toggle networking / Wi-Fi"),
        ("", ""),
        ("In forms — Tab/↑↓", "move between fields"),
        ("In forms — ←/→/Space", "cycle a choice field"),
        ("In forms — Enter", "submit"),
    ]
}
