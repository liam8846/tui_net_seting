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
            ("Enter", "連接"),
            ("d", "中斷"),
            ("s", "詳細"),
            ("r", "重新整理"),
        ],
        Tab::Connections => &[
            ("Enter", "啟用"),
            ("d", "停用"),
            ("a", "新增"),
            ("e", "編輯"),
            ("x", "刪除"),
        ],
        Tab::Wifi => &[
            ("Enter", "連接"),
            ("d", "中斷"),
            ("f", "忘記"),
            ("r", "重新掃描"),
        ],
        Tab::Hotspot => &[("a", "建立"), ("x", "停止")],
        Tab::Radio => &[("Enter", "切換")],
    }
}

/// Full binding list for the help overlay.
pub fn help_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        ("1-5 / Tab", "切換分頁"),
        ("j / k / ↑ / ↓", "移動選取"),
        ("r", "重新整理（Wi-Fi 會重新掃描）"),
        ("?", "開關此說明"),
        ("q / Ctrl-C", "離開"),
        ("Esc", "關閉面板 / 取消視窗"),
        ("", ""),
        ("裝置 - Enter/c", "連接裝置"),
        ("裝置 - d", "中斷裝置"),
        ("裝置 - s", "開關詳細面板"),
        ("連線 - Enter", "啟用設定檔"),
        ("連線 - d", "停用設定檔"),
        ("連線 - a / e", "新增 / 編輯設定檔"),
        ("連線 - x / Del", "刪除設定檔"),
        ("Wi-Fi - Enter", "連接（需要時詢問密碼）"),
        ("Wi-Fi - d / f", "中斷 / 忘記"),
        ("熱點 - a / x", "建立 / 停止熱點"),
        ("無線 - Enter", "切換網路 / Wi-Fi"),
        ("", ""),
        ("表單 - Tab/↑↓", "在欄位間移動"),
        ("表單 - ←/→/Space", "切換選項欄位"),
        ("表單 - Enter", "送出"),
    ]
}
