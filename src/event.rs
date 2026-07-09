//! The three core enums of the architecture plus the aggregated app error:
//!
//! * [`Event`]  — everything the main loop can wake up on (input / worker
//!   message / tick), merged by `tokio::select!`.
//! * [`Action`] — user intent, produced by the keymap in `ui::keys`.
//! * [`Msg`]    — results flowing back from worker tasks over the mpsc channel.

use crate::model::{Connection, ConnectionDraft, Device, RadioState, WifiAp};
use crate::nm::NmcliError;
use crate::parse::ParseError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Nmcli(#[from] NmcliError),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

impl AppError {
    /// Nothing will work until NetworkManager itself is back → blocking modal.
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Nmcli(e) if e.is_fatal())
    }

    /// Wi-Fi secrets missing/wrong → re-prompt for the password.
    pub fn is_bad_secrets(&self) -> bool {
        matches!(self, Self::Nmcli(e) if e.is_bad_secrets())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Devices,
    Connections,
    Wifi,
    Hotspot,
    Radio,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Devices,
        Tab::Connections,
        Tab::Wifi,
        Tab::Hotspot,
        Tab::Radio,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Devices => "裝置",
            Tab::Connections => "連線",
            Tab::Wifi => "Wi-Fi",
            Tab::Hotspot => "熱點",
            Tab::Radio => "無線",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// What the main loop wakes up on.
pub enum Event {
    Input(crossterm::event::Event),
    Msg(Msg),
    Tick,
}

/// User intent after keymap translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextTab,
    PrevTab,
    GotoTab(Tab),
    Up,
    Down,
    Refresh,
    ShowHelp,
    /// Esc outside a modal: close the detail pane.
    Back,

    // Devices
    DeviceConnect,
    DeviceDisconnect,
    ShowDetails,

    // Connections
    ConnActivate,
    ConnDeactivate,
    ConnAdd,
    ConnEdit,
    ConnDelete,

    // Wi-Fi
    WifiConnect,
    WifiDisconnect,
    WifiForget,

    // Hotspot / Radio
    HotspotCreate,
    HotspotStop,
    RadioToggle,

    // Modal-scoped
    FormInput(char),
    FormBackspace,
    FormNext,
    FormPrev,
    FormCycle,
    FormSubmit,
    ModalCancel,
    ConfirmYes,
    ConfirmNo,
    /// From the blocking error modal: dismiss and reload everything.
    ErrorRetry,
}

/// A mutating nmcli operation, carried through `Effect::Run` and back in
/// `Msg::CommandDone` so completion/failure can be routed to the right UI
/// reaction (toast, modal update, list refresh).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpKind {
    DeviceConnect(String),
    DeviceDisconnect(String),
    ConnUp(String),
    ConnDown(String),
    ConnDelete(String),
    ConnSave(String),
    WifiConnect { ssid: String },
    WifiForget(String),
    HotspotUp,
    HotspotDown,
    SetWifiRadio(bool),
    SetNetworking(bool),
}

impl OpKind {
    /// Spinner label while the operation is in flight.
    pub fn label(&self) -> String {
        match self {
            Self::DeviceConnect(d) => format!("正在連接裝置 {d}…"),
            Self::DeviceDisconnect(d) => format!("正在中斷裝置 {d}…"),
            Self::ConnUp(n) => format!("正在啟用「{n}」…"),
            Self::ConnDown(n) => format!("正在停用「{n}」…"),
            Self::ConnDelete(n) => format!("正在刪除「{n}」…"),
            Self::ConnSave(n) => format!("正在儲存「{n}」…"),
            Self::WifiConnect { ssid } => format!("正在連接「{ssid}」…"),
            Self::WifiForget(n) => format!("正在忘記「{n}」…"),
            Self::HotspotUp => "正在啟動熱點…".into(),
            Self::HotspotDown => "正在停止熱點…".into(),
            Self::SetWifiRadio(true) => "正在啟用 Wi-Fi 無線電…".into(),
            Self::SetWifiRadio(false) => "正在停用 Wi-Fi 無線電…".into(),
            Self::SetNetworking(true) => "正在啟用網路…".into(),
            Self::SetNetworking(false) => "正在停用網路…".into(),
        }
    }

    /// Success-toast text.
    pub fn done_label(&self) -> String {
        match self {
            Self::DeviceConnect(d) => format!("裝置 {d} 已連接"),
            Self::DeviceDisconnect(d) => format!("裝置 {d} 已中斷"),
            Self::ConnUp(n) => format!("「{n}」已啟用"),
            Self::ConnDown(n) => format!("「{n}」已停用"),
            Self::ConnDelete(n) => format!("「{n}」已刪除"),
            Self::ConnSave(n) => format!("「{n}」已儲存"),
            Self::WifiConnect { ssid } => format!("已連接到「{ssid}」"),
            Self::WifiForget(n) => format!("已忘記「{n}」"),
            Self::HotspotUp => "熱點已啟動".into(),
            Self::HotspotDown => "熱點已停止".into(),
            Self::SetWifiRadio(true) => "Wi-Fi 無線電已啟用".into(),
            Self::SetWifiRadio(false) => "Wi-Fi 無線電已停用".into(),
            Self::SetNetworking(true) => "網路已啟用".into(),
            Self::SetNetworking(false) => "網路已停用".into(),
        }
    }
}

/// Results flowing back from worker tasks.
#[derive(Debug)]
pub enum Msg {
    Devices(Result<Vec<Device>, AppError>),
    DeviceDetail {
        device: String,
        result: Result<String, AppError>,
    },
    Connections(Result<Vec<Connection>, AppError>),
    ConnectionProfile {
        uuid: String,
        // Boxed: ConnectionDraft is by far the largest payload, and boxing it
        // keeps `Msg` small enough to pass by value cheaply through the channel.
        result: Result<Box<ConnectionDraft>, AppError>,
    },
    WifiAps(Result<Vec<WifiAp>, AppError>),
    Radio(Result<RadioState, AppError>),
    CommandDone {
        op: OpKind,
        result: Result<(), AppError>,
    },
    /// `nmcli monitor` reported activity → refresh the active tab.
    NetworkChanged,
    /// Monitor child died → fall back to timer polling.
    MonitorDied,
    /// Monitor (re)started successfully.
    MonitorUp,
}
