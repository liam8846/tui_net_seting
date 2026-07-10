//! All UI state plus the pure state machine that drives it.
//!
//! `App::apply` (user actions), `App::on_msg` (worker results) and
//! `App::on_tick` mutate state and return the side effects to run — they
//! perform no I/O themselves, which keeps the whole state machine unit
//! testable on any OS.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::effect::Effect;
use crate::event::{Action, AppError, Msg, OpKind, Tab};
use crate::model::{
    Connection, ConnectionDraft, ConnectionType, Device, DeviceState, DeviceType, IpMethod,
    RadioState, WifiAp, validate_address_field, validate_dns_field, validate_gateway_field,
    validate_wifi_password,
};
use crate::nm::command as cmd;

/// Profile name that `nmcli device wifi hotspot` creates by default.
pub const HOTSPOT_NAME: &str = "Hotspot";

const TOAST_INFO_TTL: Duration = Duration::from_secs(3);
const TOAST_ERROR_TTL: Duration = Duration::from_secs(6);
/// Fallback polling cadence (in 250 ms ticks) while the monitor is down.
const POLL_TICKS: u64 = 40;

// ── small state containers ──────────────────────────────────────────────────

/// A list plus its table selection.
pub struct Rows<T> {
    pub items: Vec<T>,
    pub selected: usize,
    visible: Vec<usize>,
}

pub trait RowIdentity {
    fn same_identity(&self, other: &Self) -> bool;
}

impl RowIdentity for Device {
    fn same_identity(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl RowIdentity for Connection {
    fn same_identity(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl RowIdentity for WifiAp {
    fn same_identity(&self, other: &Self) -> bool {
        let key = if self.bssid.is_empty() {
            &self.ssid
        } else {
            &self.bssid
        };
        let other_key = if other.bssid.is_empty() {
            &other.ssid
        } else {
            &other.bssid
        };
        key == other_key
    }
}

impl<T: RowIdentity> Rows<T> {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            visible: Vec::new(),
        }
    }

    pub fn selected(&self) -> Option<&T> {
        self.visible
            .get(self.selected)
            .and_then(|index| self.items.get(*index))
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.visible
            .iter()
            .filter_map(|index| self.items.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.selected + 1 < self.visible.len() {
            self.selected += 1;
        }
    }

    pub fn first(&mut self) {
        self.selected = 0;
    }

    pub fn last(&mut self) {
        self.selected = self.visible.len().saturating_sub(1);
    }

    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.selected = self
            .selected
            .saturating_add(10)
            .min(self.visible.len().saturating_sub(1));
    }

    pub fn set(&mut self, items: Vec<T>) {
        let old_selected = self.selected;
        let aligned = self
            .selected()
            .and_then(|old| items.iter().position(|item| old.same_identity(item)));
        self.items = items;
        self.visible = (0..self.items.len()).collect();
        self.selected = aligned
            .unwrap_or(old_selected)
            .min(self.visible.len().saturating_sub(1));
    }

    fn filter(&mut self, include: impl Fn(&T) -> bool) {
        let old_selected = self.selected;
        let old_index = self.visible.get(self.selected).copied();
        self.visible = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| include(item).then_some(index))
            .collect();
        self.selected = old_index
            .and_then(|index| self.visible.iter().position(|visible| *visible == index))
            .unwrap_or(old_selected)
            .min(self.visible.len().saturating_sub(1));
    }
}

fn navigate_rows<T: RowIdentity>(rows: &mut Rows<T>, action: &Action) {
    match action {
        Action::Up => rows.up(),
        Action::Down => rows.down(),
        Action::First => rows.first(),
        Action::Last => rows.last(),
        Action::PageUp => rows.page_up(),
        Action::PageDown => rows.page_down(),
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub level: ToastLevel,
    pub text: String,
    pub expires_at: Instant,
}

pub struct InFlight {
    pub label: String,
}

#[derive(Default)]
pub struct Loading {
    pub devices: bool,
    pub connections: bool,
    pub wifi: bool,
    pub radio: bool,
}

impl Loading {
    pub fn any(&self) -> bool {
        self.devices || self.connections || self.wifi || self.radio
    }
}

/// Raw `nmcli -t device show` buffer, reparsed (zero-copy) at draw time.
pub struct DetailPane {
    pub device: String,
    pub raw: String,
}

#[derive(Default)]
pub struct WifiFilter {
    pub query: String,
    pub editing: bool,
}

// ── modals ──────────────────────────────────────────────────────────────────

pub enum Modal {
    Help,
    /// Blocking error (NetworkManager unreachable). `r` retries, `q` quits.
    Error(String),
    Confirm {
        prompt: String,
        on_yes: Confirmed,
    },
    Password(PasswordPrompt),
    ConnForm(ConnForm),
    HotspotForm(HotspotForm),
}

/// What a confirmed yes/no dialog should do.
pub enum Confirmed {
    DeleteConnection { uuid: String, name: String },
    ForgetWifi { uuid: String, name: String },
    DisconnectDevice(String),
    NetworkingOff,
    StopHotspot { uuid: String },
    Quit,
}

pub struct PasswordPrompt {
    pub ssid: String,
    pub input: String,
    pub show_password: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnField {
    Name,
    Type,
    Ssid,
    Ifname,
    Autoconnect,
    V4Method,
    V4Addresses,
    V4Gateway,
    V4Dns,
    V6Method,
    V6Addresses,
    V6Gateway,
    V6Dns,
}

impl ConnField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "名稱",
            Self::Type => "類型",
            Self::Ssid => "SSID",
            Self::Ifname => "介面",
            Self::Autoconnect => "自動連接",
            Self::V4Method => "IPv4 方式",
            Self::V4Addresses => "IPv4 位址",
            Self::V4Gateway => "IPv4 閘道",
            Self::V4Dns => "IPv4 DNS",
            Self::V6Method => "IPv6 方式",
            Self::V6Addresses => "IPv6 位址",
            Self::V6Gateway => "IPv6 閘道",
            Self::V6Dns => "IPv6 DNS",
        }
    }
}

fn ip_method_label(method: IpMethod) -> &'static str {
    match method {
        IpMethod::Auto => "自動",
        IpMethod::Manual => "手動",
        IpMethod::Disabled => "停用",
    }
}

/// The connection add/edit form.
pub struct ConnForm {
    pub draft: ConnectionDraft,
    /// Index into [`ConnForm::editable`].
    pub focus: usize,
    field_errors: Vec<(ConnField, String)>,
    pub error: Option<String>,
}

impl ConnForm {
    pub fn add() -> Self {
        Self {
            draft: ConnectionDraft::new_ethernet(),
            focus: 0,
            field_errors: Vec::new(),
            error: None,
        }
    }

    pub fn edit(draft: ConnectionDraft) -> Self {
        Self {
            draft,
            focus: 0,
            field_errors: Vec::new(),
            error: None,
        }
    }

    /// Display order with an "editable right now" flag (manual-only fields
    /// render dimmed and are skipped by focus while the method isn't manual).
    pub fn rows(&self) -> Vec<(ConnField, bool)> {
        let mut v = vec![(ConnField::Name, true)];
        if self.draft.is_new() {
            v.push((ConnField::Type, true));
            if self.draft.ctype.is_wifi() {
                v.push((ConnField::Ssid, true));
            }
            v.push((ConnField::Ifname, true));
        }
        v.push((ConnField::Autoconnect, true));
        v.push((ConnField::V4Method, true));
        let m4 = self.draft.ipv4.method == IpMethod::Manual;
        v.push((ConnField::V4Addresses, m4));
        v.push((ConnField::V4Gateway, m4));
        v.push((ConnField::V4Dns, m4));
        v.push((ConnField::V6Method, true));
        let m6 = self.draft.ipv6.method == IpMethod::Manual;
        v.push((ConnField::V6Addresses, m6));
        v.push((ConnField::V6Gateway, m6));
        v.push((ConnField::V6Dns, m6));
        v
    }

    fn editable(&self) -> Vec<ConnField> {
        self.rows()
            .into_iter()
            .filter(|(_, e)| *e)
            .map(|(f, _)| f)
            .collect()
    }

    pub fn focused(&self) -> ConnField {
        let fields = self.editable();
        fields[self.focus.min(fields.len() - 1)]
    }

    pub fn next(&mut self) {
        self.validate_current_field();
        self.focus = (self.focus + 1) % self.editable().len();
    }

    pub fn prev(&mut self) {
        self.validate_current_field();
        let len = self.editable().len();
        self.focus = (self.focus + len - 1) % len;
    }

    fn clamp(&mut self) {
        self.focus = self.focus.min(self.editable().len() - 1);
    }

    pub fn value(&self, field: ConnField) -> String {
        let d = &self.draft;
        match field {
            ConnField::Name => d.name.clone(),
            ConnField::Type => d.ctype.label().to_string(),
            ConnField::Ssid => d.ssid.clone(),
            ConnField::Ifname => d.ifname.clone(),
            ConnField::Autoconnect => if d.autoconnect { "是" } else { "否" }.into(),
            ConnField::V4Method => ip_method_label(d.ipv4.method).into(),
            ConnField::V4Addresses => d.ipv4.addresses.clone(),
            ConnField::V4Gateway => d.ipv4.gateway.clone(),
            ConnField::V4Dns => d.ipv4.dns.clone(),
            ConnField::V6Method => ip_method_label(d.ipv6.method).into(),
            ConnField::V6Addresses => d.ipv6.addresses.clone(),
            ConnField::V6Gateway => d.ipv6.gateway.clone(),
            ConnField::V6Dns => d.ipv6.dns.clone(),
        }
    }

    fn text_mut(&mut self, field: ConnField) -> Option<&mut String> {
        let d = &mut self.draft;
        match field {
            ConnField::Name => Some(&mut d.name),
            ConnField::Ssid => Some(&mut d.ssid),
            ConnField::Ifname => Some(&mut d.ifname),
            ConnField::V4Addresses => Some(&mut d.ipv4.addresses),
            ConnField::V4Gateway => Some(&mut d.ipv4.gateway),
            ConnField::V4Dns => Some(&mut d.ipv4.dns),
            ConnField::V6Addresses => Some(&mut d.ipv6.addresses),
            ConnField::V6Gateway => Some(&mut d.ipv6.gateway),
            ConnField::V6Dns => Some(&mut d.ipv6.dns),
            _ => None,
        }
    }

    pub fn insert(&mut self, c: char) {
        let field = self.focused();
        if let Some(text) = self.text_mut(field) {
            text.push(c);
            self.error = None;
            self.validate_field(field);
        } else if c == ' ' {
            self.cycle();
        }
    }

    pub fn backspace(&mut self) {
        let field = self.focused();
        if let Some(text) = self.text_mut(field) {
            text.pop();
            self.error = None;
            self.validate_field(field);
        }
    }

    pub fn cycle(&mut self) {
        self.error = None;
        match self.focused() {
            ConnField::Type => {
                self.draft.ctype = if self.draft.ctype.is_wifi() {
                    ConnectionType::Ethernet
                } else {
                    ConnectionType::Wifi
                };
            }
            ConnField::Autoconnect => self.draft.autoconnect = !self.draft.autoconnect,
            ConnField::V4Method => self.draft.ipv4.method = self.draft.ipv4.method.cycle(),
            ConnField::V6Method => self.draft.ipv6.method = self.draft.ipv6.method.cycle(),
            _ => {}
        }
        self.clear_disabled_field_errors();
        self.clamp();
    }

    pub fn field_error(&self, field: ConnField) -> Option<&str> {
        self.field_errors
            .iter()
            .find(|(candidate, _)| *candidate == field)
            .map(|(_, error)| error.as_str())
    }

    fn set_field_error(&mut self, field: ConnField, error: Option<String>) {
        self.field_errors
            .retain(|(candidate, _)| *candidate != field);
        if let Some(error) = error {
            self.field_errors.push((field, error));
        }
    }

    fn validate_current_field(&mut self) {
        self.validate_field(self.focused());
    }

    fn validate_field(&mut self, field: ConnField) {
        let result = match field {
            ConnField::V4Addresses if self.draft.ipv4.method == IpMethod::Manual => {
                validate_address_field(&self.draft.ipv4.addresses, false)
            }
            ConnField::V4Gateway if self.draft.ipv4.method == IpMethod::Manual => {
                validate_gateway_field(&self.draft.ipv4.gateway, false)
            }
            ConnField::V4Dns if self.draft.ipv4.method == IpMethod::Manual => {
                validate_dns_field(&self.draft.ipv4.dns, false)
            }
            ConnField::V6Addresses if self.draft.ipv6.method == IpMethod::Manual => {
                validate_address_field(&self.draft.ipv6.addresses, true)
            }
            ConnField::V6Gateway if self.draft.ipv6.method == IpMethod::Manual => {
                validate_gateway_field(&self.draft.ipv6.gateway, true)
            }
            ConnField::V6Dns if self.draft.ipv6.method == IpMethod::Manual => {
                validate_dns_field(&self.draft.ipv6.dns, true)
            }
            _ => Ok(()),
        };
        self.set_field_error(field, result.err());
    }

    fn clear_disabled_field_errors(&mut self) {
        if self.draft.ipv4.method != IpMethod::Manual {
            for field in [
                ConnField::V4Addresses,
                ConnField::V4Gateway,
                ConnField::V4Dns,
            ] {
                self.set_field_error(field, None);
            }
        }
        if self.draft.ipv6.method != IpMethod::Manual {
            for field in [
                ConnField::V6Addresses,
                ConnField::V6Gateway,
                ConnField::V6Dns,
            ] {
                self.set_field_error(field, None);
            }
        }
    }

    fn validate_all_fields(&mut self) {
        for field in [
            ConnField::V4Addresses,
            ConnField::V4Gateway,
            ConnField::V4Dns,
            ConnField::V6Addresses,
            ConnField::V6Gateway,
            ConnField::V6Dns,
        ] {
            self.validate_field(field);
        }
    }
}

/// The hotspot creation form.
pub struct HotspotForm {
    pub ifnames: Vec<String>,
    pub ifname_idx: usize,
    pub ssid: String,
    pub password: String,
    pub show_password: bool,
    /// 0 = interface, 1 = SSID, 2 = password.
    pub focus: usize,
    pub password_error: Option<String>,
    pub error: Option<String>,
}

impl HotspotForm {
    pub fn new(ifnames: Vec<String>) -> Self {
        Self {
            ifnames,
            ifname_idx: 0,
            ssid: HOTSPOT_NAME.to_string(),
            password: String::new(),
            show_password: false,
            focus: 0,
            password_error: None,
            error: None,
        }
    }

    pub fn ifname(&self) -> &str {
        self.ifnames
            .get(self.ifname_idx)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn insert(&mut self, c: char) {
        match self.focus {
            0 if c == ' ' => self.cycle(),
            1 => {
                self.ssid.push(c);
                self.error = None;
            }
            2 => {
                self.password.push(c);
                self.error = None;
                self.validate_password_field();
            }
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            1 => {
                self.ssid.pop();
                self.error = None;
            }
            2 => {
                self.password.pop();
                self.error = None;
                self.validate_password_field();
            }
            _ => {}
        }
    }

    pub fn cycle(&mut self) {
        if self.focus == 0 && !self.ifnames.is_empty() {
            self.ifname_idx = (self.ifname_idx + 1) % self.ifnames.len();
        }
    }

    pub fn next(&mut self) {
        self.validate_current_field();
        self.focus = (self.focus + 1) % 3;
    }

    pub fn prev(&mut self) {
        self.validate_current_field();
        self.focus = (self.focus + 2) % 3;
    }

    fn validate_current_field(&mut self) {
        if self.focus == 2 {
            self.validate_password_field();
        }
    }

    fn validate_password_field(&mut self) {
        self.password_error = validate_wifi_password(&self.password).err();
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.ifname().is_empty() {
            return Err("尚未選擇 wifi 介面".into());
        }
        if self.ssid.trim().is_empty() {
            return Err("SSID 不能為空".into());
        }
        validate_wifi_password(&self.password)
    }
}

// ── the app itself ──────────────────────────────────────────────────────────

pub struct App {
    pub tab: Tab,
    pub devices: Rows<Device>,
    pub connections: Rows<Connection>,
    pub wifi: Rows<WifiAp>,
    pub radio: Option<RadioState>,
    /// Selected row on the Radio tab: 0 = networking, 1 = wifi radio.
    pub radio_sel: usize,
    pub detail: Option<DetailPane>,
    pub detail_scroll: u16,
    pub help_scroll: u16,
    pub wifi_filter: WifiFilter,
    pub modal: Option<Modal>,
    pub in_flight: Option<InFlight>,
    pub loading: Loading,
    pub toasts: VecDeque<Toast>,
    pub monitor_ok: bool,
    pub tick: u64,
    pub dirty: bool,
    pub should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Devices,
            devices: Rows::new(),
            connections: Rows::new(),
            wifi: Rows::new(),
            radio: None,
            radio_sel: 0,
            detail: None,
            detail_scroll: 0,
            help_scroll: 0,
            wifi_filter: WifiFilter::default(),
            modal: None,
            in_flight: None,
            loading: Loading::default(),
            toasts: VecDeque::new(),
            monitor_ok: true,
            tick: 0,
            dirty: true,
            should_quit: false,
        }
    }

    pub fn initial_effects(&mut self) -> Vec<Effect> {
        let mut effects = self.refresh_all();
        effects.push(Effect::SpawnMonitor);
        effects
    }

    // ── toasts ──────────────────────────────────────────────────────────

    fn toast(&mut self, level: ToastLevel, text: impl Into<String>, ttl: Duration) {
        self.toasts.push_back(Toast {
            level,
            text: text.into(),
            expires_at: Instant::now() + ttl,
        });
    }

    pub fn toast_info(&mut self, text: impl Into<String>) {
        self.toast(ToastLevel::Info, text, TOAST_INFO_TTL);
    }

    pub fn toast_success(&mut self, text: impl Into<String>) {
        self.toast(ToastLevel::Success, text, TOAST_INFO_TTL);
    }

    pub fn toast_error(&mut self, text: impl Into<String>) {
        self.toast(ToastLevel::Error, text, TOAST_ERROR_TTL);
    }

    pub fn current_toast(&self) -> Option<&Toast> {
        self.toasts.back()
    }

    // ── refresh plumbing ────────────────────────────────────────────────

    fn refresh_all(&mut self) -> Vec<Effect> {
        self.loading = Loading {
            devices: true,
            connections: true,
            wifi: true,
            radio: true,
        };
        vec![
            Effect::LoadDevices,
            Effect::LoadConnections,
            Effect::LoadWifi { rescan: false },
            Effect::LoadRadio,
        ]
    }

    fn refresh_tab(&mut self, rescan: bool) -> Vec<Effect> {
        match self.tab {
            Tab::Devices => {
                self.loading.devices = true;
                vec![Effect::LoadDevices]
            }
            Tab::Connections => {
                self.loading.connections = true;
                vec![Effect::LoadConnections]
            }
            Tab::Wifi => {
                self.loading.wifi = true;
                self.loading.radio = true;
                vec![Effect::LoadWifi { rescan }, Effect::LoadRadio]
            }
            Tab::Hotspot => {
                self.loading.devices = true;
                self.loading.connections = true;
                vec![Effect::LoadDevices, Effect::LoadConnections]
            }
            Tab::Radio => {
                self.loading.radio = true;
                vec![Effect::LoadRadio]
            }
        }
    }

    /// Funnel for every mutating operation: enforces "at most one in flight".
    fn start_op(&mut self, op: OpKind, args: Vec<String>, effects: &mut Vec<Effect>) {
        if self.in_flight.is_some() {
            self.toast_info("另一個操作仍在執行中");
            return;
        }
        self.in_flight = Some(InFlight { label: op.label() });
        effects.push(Effect::Run { op, args });
    }

    // ── lookups ─────────────────────────────────────────────────────────

    fn saved_wifi_profile(&self, ssid: &str) -> Option<&Connection> {
        self.connections
            .items
            .iter()
            .find(|c| c.ctype == ConnectionType::Wifi && c.name == ssid)
    }

    pub fn is_wifi_saved(&self, ap: &WifiAp) -> bool {
        self.saved_wifi_profile(&ap.ssid).is_some()
    }

    pub fn hotspot_profile(&self) -> Option<&Connection> {
        self.connections
            .items
            .iter()
            .find(|c| c.ctype == ConnectionType::Wifi && c.name == HOTSPOT_NAME)
    }

    pub fn wifi_device_names(&self) -> Vec<String> {
        self.devices
            .items
            .iter()
            .filter(|d| d.dtype == DeviceType::Wifi)
            .map(|d| d.name.clone())
            .collect()
    }

    fn connected_wifi_device(&self) -> Option<&Device> {
        self.devices
            .items
            .iter()
            .find(|d| d.dtype == DeviceType::Wifi && d.state == DeviceState::Connected)
    }

    // ── user actions ────────────────────────────────────────────────────

    pub fn apply(&mut self, action: Action) -> Vec<Effect> {
        self.dirty = true;
        let mut effects = Vec::new();
        if self.modal.is_some() {
            self.apply_modal(action, &mut effects);
            return effects;
        }
        match action {
            Action::Quit => {
                if self.in_flight.is_some() {
                    self.modal = Some(Modal::Confirm {
                        prompt: "操作仍在執行中。仍要離開？".into(),
                        on_yes: Confirmed::Quit,
                    });
                } else {
                    self.should_quit = true;
                }
            }
            Action::NextTab => return self.goto_tab(self.tab.next()),
            Action::PrevTab => return self.goto_tab(self.tab.prev()),
            Action::GotoTab(tab) => return self.goto_tab(tab),
            Action::Up
            | Action::Down
            | Action::First
            | Action::Last
            | Action::PageUp
            | Action::PageDown => self.navigate(action, &mut effects),
            Action::SelectRow(index) => self.select_row(index, &mut effects),
            Action::Refresh => return self.refresh_tab(self.tab == Tab::Wifi),
            Action::ShowHelp => {
                self.help_scroll = 0;
                self.modal = Some(Modal::Help);
            }
            Action::Back => {
                if self.tab == Tab::Wifi && !self.wifi_filter.query.is_empty() {
                    self.clear_wifi_filter();
                } else {
                    self.close_detail();
                }
            }
            Action::WifiFilterStart => self.wifi_filter.editing = true,
            Action::WifiFilterInput(c) => {
                self.wifi_filter.query.push(c);
                self.apply_wifi_filter();
            }
            Action::WifiFilterBackspace => {
                self.wifi_filter.query.pop();
                self.apply_wifi_filter();
            }
            Action::WifiFilterAccept => self.wifi_filter.editing = false,
            Action::WifiFilterClear => self.clear_wifi_filter(),
            Action::ScrollUp => self.scroll_up(),
            Action::ScrollDown => self.scroll_down(),

            Action::DeviceConnect => {
                if let Some(d) = self.devices.selected() {
                    let name = d.name.clone();
                    self.start_op(
                        OpKind::DeviceConnect(name.clone()),
                        cmd::args_device_connect(&name),
                        &mut effects,
                    );
                }
            }
            Action::DeviceDisconnect => {
                if let Some(d) = self.devices.selected() {
                    let name = d.name.clone();
                    if d.dtype == DeviceType::Ethernet {
                        // Cutting the wire may drop the session you're in.
                        self.modal = Some(Modal::Confirm {
                            prompt: format!("要中斷有線裝置 {name} 嗎？"),
                            on_yes: Confirmed::DisconnectDevice(name),
                        });
                    } else {
                        self.start_op(
                            OpKind::DeviceDisconnect(name.clone()),
                            cmd::args_device_disconnect(&name),
                            &mut effects,
                        );
                    }
                }
            }
            Action::ShowDetails => {
                if let Some(d) = self.devices.selected() {
                    let same = self.detail.as_ref().is_some_and(|p| p.device == d.name);
                    if same {
                        self.close_detail();
                    } else {
                        self.detail_scroll = 0;
                        effects.push(Effect::LoadDeviceDetail(d.name.clone()));
                    }
                }
            }

            Action::ConnActivate => {
                if let Some(c) = self.connections.selected() {
                    let (name, uuid) = (c.name.clone(), c.uuid.clone());
                    self.start_op(
                        OpKind::ConnUp(name),
                        cmd::args_connection_up(&uuid),
                        &mut effects,
                    );
                }
            }
            Action::ConnDeactivate => {
                if let Some(c) = self.connections.selected() {
                    if !c.active {
                        self.toast_info("連線未啟用");
                    } else {
                        let (name, uuid) = (c.name.clone(), c.uuid.clone());
                        self.start_op(
                            OpKind::ConnDown(name),
                            cmd::args_connection_down(&uuid),
                            &mut effects,
                        );
                    }
                }
            }
            Action::ConnAdd => self.modal = Some(Modal::ConnForm(ConnForm::add())),
            Action::ConnEdit => {
                if let Some(c) = self.connections.selected() {
                    effects.push(Effect::LoadConnectionProfile(c.uuid.clone()));
                }
            }
            Action::ConnDelete => {
                if let Some(c) = self.connections.selected() {
                    self.modal = Some(Modal::Confirm {
                        prompt: format!("要刪除連線「{}」嗎？", c.name),
                        on_yes: Confirmed::DeleteConnection {
                            uuid: c.uuid.clone(),
                            name: c.name.clone(),
                        },
                    });
                }
            }

            Action::WifiConnect => self.wifi_connect(&mut effects),
            Action::WifiDisconnect => {
                if let Some(d) = self.connected_wifi_device() {
                    let name = d.name.clone();
                    self.start_op(
                        OpKind::DeviceDisconnect(name.clone()),
                        cmd::args_device_disconnect(&name),
                        &mut effects,
                    );
                } else {
                    self.toast_info("沒有已連接的 wifi 裝置");
                }
            }
            Action::WifiForget => {
                if let Some(ap) = self.wifi.selected() {
                    match self.saved_wifi_profile(&ap.ssid) {
                        Some(c) => {
                            self.modal = Some(Modal::Confirm {
                                prompt: format!("要忘記已儲存的網路「{}」嗎？", c.name),
                                on_yes: Confirmed::ForgetWifi {
                                    uuid: c.uuid.clone(),
                                    name: c.name.clone(),
                                },
                            });
                        }
                        None => self.toast_info("此網路沒有已儲存的設定檔"),
                    }
                }
            }

            Action::HotspotCreate => {
                let ifnames = self.wifi_device_names();
                if ifnames.is_empty() {
                    self.toast_error("沒有可用於熱點的 wifi 裝置");
                } else {
                    self.modal = Some(Modal::HotspotForm(HotspotForm::new(ifnames)));
                }
            }
            Action::HotspotStop => match self.hotspot_profile() {
                Some(c) => {
                    self.modal = Some(Modal::Confirm {
                        prompt: "要停止並刪除熱點嗎？".into(),
                        on_yes: Confirmed::StopHotspot {
                            uuid: c.uuid.clone(),
                        },
                    });
                }
                None => self.toast_info("找不到熱點設定檔"),
            },
            Action::RadioToggle => self.radio_toggle(&mut effects),

            // Modal-scoped actions outside a modal are no-ops.
            _ => {}
        }
        effects
    }

    fn goto_tab(&mut self, tab: Tab) -> Vec<Effect> {
        self.tab = tab;
        self.close_detail();
        self.refresh_tab(false)
    }

    fn navigate(&mut self, action: Action, effects: &mut Vec<Effect>) {
        match self.tab {
            Tab::Devices => {
                let before = self.devices.selected;
                navigate_rows(&mut self.devices, &action);
                // Keep an open detail pane in sync with the selection.
                if self.devices.selected != before
                    && self.detail.is_some()
                    && let Some(d) = self.devices.selected()
                {
                    self.detail_scroll = 0;
                    effects.push(Effect::LoadDeviceDetail(d.name.clone()));
                }
            }
            Tab::Connections => navigate_rows(&mut self.connections, &action),
            Tab::Wifi => navigate_rows(&mut self.wifi, &action),
            Tab::Radio => {
                self.radio_sel = if matches!(action, Action::Up | Action::First | Action::PageUp) {
                    0
                } else {
                    1
                };
            }
            Tab::Hotspot => {}
        }
    }

    fn select_row(&mut self, index: usize, effects: &mut Vec<Effect>) {
        match self.tab {
            Tab::Devices => {
                let before = self.devices.selected;
                if index < self.devices.len() {
                    self.devices.selected = index;
                }
                if self.devices.selected != before
                    && self.detail.is_some()
                    && let Some(device) = self.devices.selected()
                {
                    self.detail_scroll = 0;
                    effects.push(Effect::LoadDeviceDetail(device.name.clone()));
                }
            }
            Tab::Connections => {
                if index < self.connections.len() {
                    self.connections.selected = index;
                }
            }
            Tab::Wifi => {
                if index < self.wifi.len() {
                    self.wifi.selected = index;
                }
            }
            Tab::Radio => {
                if index < 2 {
                    self.radio_sel = index;
                }
            }
            Tab::Hotspot => {}
        }
    }

    fn apply_wifi_filter(&mut self) {
        let query = self.wifi_filter.query.to_lowercase();
        self.wifi
            .filter(|ap| ap.ssid.to_lowercase().contains(&query));
    }

    fn clear_wifi_filter(&mut self) {
        self.wifi_filter.query.clear();
        self.wifi_filter.editing = false;
        self.apply_wifi_filter();
    }

    fn close_detail(&mut self) {
        self.detail = None;
        self.detail_scroll = 0;
    }

    fn scroll_up(&mut self) {
        if matches!(self.modal.as_ref(), Some(Modal::Help)) {
            self.help_scroll = self.help_scroll.saturating_sub(1);
        } else if self.detail.is_some() {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
        }
    }

    fn scroll_down(&mut self) {
        if matches!(self.modal.as_ref(), Some(Modal::Help)) {
            self.help_scroll = self.help_scroll.saturating_add(1);
        } else if self.detail.is_some() {
            self.detail_scroll = self.detail_scroll.saturating_add(1);
        }
    }

    fn wifi_connect(&mut self, effects: &mut Vec<Effect>) {
        let Some(ap) = self.wifi.selected() else {
            return;
        };
        if ap.is_hidden() {
            self.toast_info("尚未支援隱藏網路");
            return;
        }
        if ap.in_use {
            self.toast_info("已連接到此網路");
            return;
        }
        let ssid = ap.ssid.clone();
        let secured = ap.is_secured();
        if let Some(saved) = self.saved_wifi_profile(&ssid) {
            let (name, uuid) = (saved.name.clone(), saved.uuid.clone());
            self.start_op(
                OpKind::ConnUp(name),
                cmd::args_connection_up(&uuid),
                effects,
            );
        } else if secured {
            self.modal = Some(Modal::Password(PasswordPrompt {
                ssid,
                input: String::new(),
                show_password: false,
                error: None,
            }));
        } else {
            self.start_op(
                OpKind::WifiConnect { ssid: ssid.clone() },
                cmd::args_wifi_connect(&ssid, None),
                effects,
            );
        }
    }

    fn radio_toggle(&mut self, effects: &mut Vec<Effect>) {
        let Some(radio) = self.radio else {
            self.toast_info("無線狀態尚未載入");
            return;
        };
        if self.radio_sel == 0 {
            if radio.networking {
                self.modal = Some(Modal::Confirm {
                    prompt: "要停用網路嗎？這會中斷所有連線。".into(),
                    on_yes: Confirmed::NetworkingOff,
                });
            } else {
                self.start_op(
                    OpKind::SetNetworking(true),
                    cmd::args_networking(true),
                    effects,
                );
            }
        } else {
            let target = !radio.wifi;
            self.start_op(
                OpKind::SetWifiRadio(target),
                cmd::args_radio_wifi(target),
                effects,
            );
        }
    }

    // ── modal actions ───────────────────────────────────────────────────

    fn apply_modal(&mut self, action: Action, effects: &mut Vec<Effect>) {
        match action {
            Action::ModalCancel | Action::ConfirmNo => {
                if matches!(self.modal.as_ref(), Some(Modal::Help)) {
                    self.help_scroll = 0;
                }
                self.modal = None;
            }
            Action::ScrollUp => self.scroll_up(),
            Action::ScrollDown => self.scroll_down(),
            Action::ErrorRetry => {
                self.modal = None;
                effects.extend(self.refresh_all());
            }
            Action::Quit => {
                // Only reachable from the blocking error modal.
                self.should_quit = true;
            }
            Action::ConfirmYes => self.confirm_yes(effects),
            Action::FormNext => match &mut self.modal {
                Some(Modal::ConnForm(f)) => f.next(),
                Some(Modal::HotspotForm(f)) => f.next(),
                _ => {}
            },
            Action::FormPrev => match &mut self.modal {
                Some(Modal::ConnForm(f)) => f.prev(),
                Some(Modal::HotspotForm(f)) => f.prev(),
                _ => {}
            },
            Action::FormCycle => match &mut self.modal {
                Some(Modal::ConnForm(f)) => f.cycle(),
                Some(Modal::HotspotForm(f)) => f.cycle(),
                _ => {}
            },
            Action::TogglePasswordVisibility => match &mut self.modal {
                Some(Modal::Password(p)) => p.show_password = !p.show_password,
                Some(Modal::HotspotForm(f)) => f.show_password = !f.show_password,
                _ => {}
            },
            Action::FormBackspace => match &mut self.modal {
                Some(Modal::Password(p)) => {
                    p.input.pop();
                    p.error = None;
                }
                Some(Modal::ConnForm(f)) => f.backspace(),
                Some(Modal::HotspotForm(f)) => f.backspace(),
                _ => {}
            },
            Action::FormInput(c) => match &mut self.modal {
                Some(Modal::Password(p)) => {
                    p.input.push(c);
                    p.error = None;
                }
                Some(Modal::ConnForm(f)) => f.insert(c),
                Some(Modal::HotspotForm(f)) => f.insert(c),
                _ => {}
            },
            Action::FormSubmit => self.form_submit(effects),
            _ => {}
        }
    }

    fn confirm_yes(&mut self, effects: &mut Vec<Effect>) {
        let Some(Modal::Confirm { on_yes, .. }) = self.modal.take() else {
            return;
        };
        match on_yes {
            Confirmed::DeleteConnection { uuid, name } => {
                self.start_op(
                    OpKind::ConnDelete(name),
                    cmd::args_connection_delete(&uuid),
                    effects,
                );
            }
            Confirmed::ForgetWifi { uuid, name } => {
                self.start_op(
                    OpKind::WifiForget(name),
                    cmd::args_connection_delete(&uuid),
                    effects,
                );
            }
            Confirmed::DisconnectDevice(name) => {
                self.start_op(
                    OpKind::DeviceDisconnect(name.clone()),
                    cmd::args_device_disconnect(&name),
                    effects,
                );
            }
            Confirmed::NetworkingOff => {
                self.start_op(
                    OpKind::SetNetworking(false),
                    cmd::args_networking(false),
                    effects,
                );
            }
            Confirmed::StopHotspot { uuid } => {
                // Deleting the profile deactivates it first.
                self.start_op(
                    OpKind::HotspotDown,
                    cmd::args_connection_delete(&uuid),
                    effects,
                );
            }
            Confirmed::Quit => self.should_quit = true,
        }
    }

    fn form_submit(&mut self, effects: &mut Vec<Effect>) {
        match &mut self.modal {
            Some(Modal::Password(p)) => {
                if p.input.is_empty() {
                    p.error = Some("密碼不能為空".into());
                    return;
                }
                p.error = None;
                let (ssid, pw) = (p.ssid.clone(), p.input.clone());
                self.start_op(
                    OpKind::WifiConnect { ssid: ssid.clone() },
                    cmd::args_wifi_connect(&ssid, Some(&pw)),
                    effects,
                );
            }
            Some(Modal::ConnForm(f)) => {
                f.validate_all_fields();
                if let Err(e) = f.draft.validate() {
                    f.error = Some(e);
                    return;
                }
                f.error = None;
                let draft = f.draft.clone();
                let args = match &draft.uuid {
                    Some(uuid) => cmd::args_connection_modify(uuid, &draft),
                    None => cmd::args_connection_add(&draft),
                };
                self.start_op(OpKind::ConnSave(draft.name.clone()), args, effects);
            }
            Some(Modal::HotspotForm(f)) => {
                f.validate_password_field();
                if let Err(e) = f.validate() {
                    f.error = Some(e);
                    return;
                }
                f.error = None;
                let args = cmd::args_hotspot(f.ifname(), f.ssid.trim(), &f.password);
                self.start_op(OpKind::HotspotUp, args, effects);
            }
            _ => {}
        }
    }

    // ── worker messages ─────────────────────────────────────────────────

    pub fn on_msg(&mut self, msg: Msg) -> Vec<Effect> {
        self.dirty = true;
        let mut effects = Vec::new();
        match msg {
            Msg::Devices(result) => {
                self.loading.devices = false;
                match result {
                    Ok(items) => self.devices.set(items),
                    Err(e) => self.fail(e),
                }
            }
            Msg::Connections(result) => {
                self.loading.connections = false;
                match result {
                    Ok(items) => self.connections.set(items),
                    Err(e) => self.fail(e),
                }
            }
            Msg::WifiAps(result) => {
                self.loading.wifi = false;
                match result {
                    Ok(mut items) => {
                        items.sort_by(|a, b| {
                            b.in_use
                                .cmp(&a.in_use)
                                .then_with(|| b.signal.cmp(&a.signal))
                        });
                        self.wifi.set(items);
                        self.apply_wifi_filter();
                    }
                    Err(e) => self.fail(e),
                }
            }
            Msg::Radio(result) => {
                self.loading.radio = false;
                match result {
                    Ok(state) => self.radio = Some(state),
                    Err(e) => self.fail(e),
                }
            }
            Msg::DeviceDetail { device, result } => match result {
                Ok(raw) => {
                    self.detail_scroll = 0;
                    self.detail = Some(DetailPane { device, raw });
                }
                Err(e) => self.fail(e),
            },
            Msg::ConnectionProfile { uuid: _, result } => match result {
                Ok(draft) => self.modal = Some(Modal::ConnForm(ConnForm::edit(*draft))),
                Err(e) => self.fail(e),
            },
            Msg::CommandDone { op, result } => {
                self.in_flight = None;
                match result {
                    Ok(()) => {
                        self.close_modal_for(&op);
                        self.toast_success(op.done_label());
                        effects.extend(self.refresh_after(&op));
                    }
                    Err(e) => self.command_failed(op, e),
                }
            }
            Msg::NetworkChanged => effects.extend(self.refresh_tab(false)),
            Msg::MonitorDied => {
                if self.monitor_ok {
                    self.monitor_ok = false;
                    self.toast_error("變更監視中斷，每 10 秒輪詢一次");
                }
            }
            Msg::MonitorUp => {
                if !self.monitor_ok {
                    self.monitor_ok = true;
                    self.toast_info("變更監視已恢復");
                }
            }
        }
        effects
    }

    /// Read/refresh failure routing: fatal → blocking modal, else toast.
    fn fail(&mut self, e: AppError) {
        if e.is_fatal() {
            self.modal = Some(Modal::Error(e.to_string()));
        } else {
            self.toast_error(e.to_string());
        }
    }

    /// Close the modal a successful operation belongs to.
    fn close_modal_for(&mut self, op: &OpKind) {
        let close = matches!(
            (&self.modal, op),
            (Some(Modal::Password(_)), OpKind::WifiConnect { .. })
                | (Some(Modal::ConnForm(_)), OpKind::ConnSave(_))
                | (Some(Modal::HotspotForm(_)), OpKind::HotspotUp)
        );
        if close {
            self.modal = None;
        }
    }

    fn command_failed(&mut self, op: OpKind, e: AppError) {
        if e.is_fatal() {
            self.modal = Some(Modal::Error(e.to_string()));
            return;
        }
        let text = e.to_string();
        match (&mut self.modal, &op) {
            // Wrong Wi-Fi password → keep/reopen the prompt with the reason.
            (Some(Modal::Password(p)), OpKind::WifiConnect { .. }) => {
                p.error = Some(text);
                p.input.clear();
            }
            (_, OpKind::WifiConnect { ssid }) if e.is_bad_secrets() => {
                self.modal = Some(Modal::Password(PasswordPrompt {
                    ssid: ssid.clone(),
                    input: String::new(),
                    show_password: false,
                    error: Some(text),
                }));
            }
            (_, OpKind::ConnUp(name)) if e.is_bad_secrets() => {
                self.modal = Some(Modal::Password(PasswordPrompt {
                    ssid: name.clone(),
                    input: String::new(),
                    show_password: false,
                    error: Some(text),
                }));
            }
            // Keep edit forms open so the user's input isn't lost.
            (Some(Modal::ConnForm(f)), OpKind::ConnSave(_)) => f.error = Some(text),
            (Some(Modal::HotspotForm(f)), OpKind::HotspotUp) => f.error = Some(text),
            _ => self.toast_error(text),
        }
    }

    /// Which lists to reload after a successful mutation.
    fn refresh_after(&mut self, op: &OpKind) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut devices = || {
            self.loading.devices = true;
            Effect::LoadDevices
        };
        match op {
            OpKind::DeviceConnect(_) | OpKind::DeviceDisconnect(_) => {
                effects.push(devices());
                self.loading.connections = true;
                effects.push(Effect::LoadConnections);
            }
            OpKind::ConnUp(_)
            | OpKind::ConnDown(_)
            | OpKind::ConnDelete(_)
            | OpKind::ConnSave(_) => {
                effects.push(devices());
                self.loading.connections = true;
                effects.push(Effect::LoadConnections);
            }
            OpKind::WifiConnect { .. }
            | OpKind::WifiForget(_)
            | OpKind::HotspotUp
            | OpKind::HotspotDown => {
                effects.push(devices());
                self.loading.connections = true;
                effects.push(Effect::LoadConnections);
                self.loading.wifi = true;
                effects.push(Effect::LoadWifi { rescan: false });
            }
            OpKind::SetWifiRadio(_) | OpKind::SetNetworking(_) => {
                effects.push(devices());
                self.loading.radio = true;
                effects.push(Effect::LoadRadio);
                self.loading.wifi = true;
                effects.push(Effect::LoadWifi { rescan: false });
            }
        }
        effects
    }

    // ── ticks ───────────────────────────────────────────────────────────

    pub fn on_tick(&mut self) -> Vec<Effect> {
        self.tick += 1;
        let now = Instant::now();
        let before = self.toasts.len();
        self.toasts.retain(|t| t.expires_at > now);
        if self.toasts.len() != before {
            self.dirty = true;
        }
        // Keep the spinner animating.
        if self.in_flight.is_some() || self.loading.any() {
            self.dirty = true;
        }
        // Fallback polling while `nmcli monitor` is down.
        if !self.monitor_ok && self.tick.is_multiple_of(POLL_TICKS) {
            return self.refresh_tab(false);
        }
        Vec::new()
    }
}
