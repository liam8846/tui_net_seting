//! Pure state-machine tests for `App`. No terminal, no nmcli — just feed
//! Actions/Msgs and assert on the resulting state and effects.

use tui_net_seting::app::{App, DetailPane, Modal, ToastLevel};
use tui_net_seting::effect::Effect;
use tui_net_seting::event::{Action, AppError, Msg, OpKind, Tab};
use tui_net_seting::model::{Connection, ConnectionType, Device, DeviceState, DeviceType, WifiAp};
use tui_net_seting::nm::{NmExit, NmcliError};
use tui_net_seting::parse::device::parse_devices;

fn app_with_devices() -> App {
    let mut app = App::new();
    let rows =
        parse_devices("eth0:ethernet:connected:Wired\nwlan0:wifi:disconnected:--\n").unwrap();
    let devices: Vec<Device> = rows.into_iter().map(Device::from).collect();
    app.on_msg(Msg::Devices(Ok(devices)));
    app
}

fn activation_error() -> AppError {
    AppError::Nmcli(NmcliError::Failed {
        exit: Some(NmExit::ActivationFailed),
        code: 4,
        stderr: "Error: Connection activation failed: Secrets were required".into(),
    })
}
fn passwd_file_password_error() -> AppError {
    AppError::Nmcli(NmcliError::Failed {
        exit: Some(NmExit::ActivationFailed),
        code: 4,
        stderr: "Warning: password for '802-11-wireless-security.psk' not given in 'passwd-file' and nmcli cannot ask without '--ask' option.".into(),
    })
}

fn device(name: &str) -> Device {
    Device {
        name: name.into(),
        dtype: DeviceType::Ethernet,
        state: DeviceState::Disconnected,
        connection: None,
    }
}

fn wifi_ap(ssid: &str, bssid: &str, signal: u8, in_use: bool) -> WifiAp {
    WifiAp {
        in_use,
        ssid: ssid.into(),
        bssid: bssid.into(),
        signal,
        security: "--".into(),
        chan: "1".into(),
        rate: "54 Mbit/s".into(),
    }
}

fn connection(name: &str, uuid: &str, ctype: ConnectionType) -> Connection {
    Connection {
        name: name.into(),
        uuid: uuid.into(),
        ctype,
        device: None,
        active: false,
        autoconnect: true,
    }
}

#[test]
fn device_connect_starts_one_inflight_op() {
    let mut app = app_with_devices();
    let effects = app.apply(Action::DeviceConnect);
    assert!(app.in_flight.is_some());
    assert!(matches!(
        effects.as_slice(),
        [Effect::Run {
            op: OpKind::DeviceConnect(_),
            ..
        }]
    ));
}

#[test]
fn mutations_are_ignored_while_an_op_is_in_flight() {
    let mut app = app_with_devices();
    app.apply(Action::DeviceConnect);
    let toasts_before = app.toasts.len();

    // A second mutating action must not start another op.
    let effects = app.apply(Action::DeviceConnect);
    assert!(effects.is_empty());
    assert!(app.toasts.len() > toasts_before); // "another operation is still running"
}

#[test]
fn navigation_still_works_while_in_flight() {
    let mut app = app_with_devices();
    app.apply(Action::DeviceConnect);
    let effects = app.apply(Action::Down);
    assert_eq!(app.devices.selected, 1);
    assert!(effects.is_empty());
}

#[test]
fn command_done_ok_clears_inflight_and_refreshes() {
    let mut app = app_with_devices();
    app.apply(Action::DeviceConnect);
    let effects = app.on_msg(Msg::CommandDone {
        op: OpKind::DeviceConnect("eth0".into()),
        result: Ok(()),
    });
    assert!(app.in_flight.is_none());
    assert_eq!(
        app.current_toast().map(|t| t.level),
        Some(ToastLevel::Success)
    );
    assert!(effects.iter().any(|e| matches!(e, Effect::LoadDevices)));
}

#[test]
fn command_failure_shows_error_toast() {
    let mut app = app_with_devices();
    app.apply(Action::DeviceConnect);
    app.on_msg(Msg::CommandDone {
        op: OpKind::DeviceConnect("eth0".into()),
        result: Err(AppError::Nmcli(NmcliError::Failed {
            exit: Some(NmExit::Unknown),
            code: 1,
            stderr: "boom".into(),
        })),
    });
    assert!(app.in_flight.is_none());
    assert_eq!(
        app.current_toast().map(|t| t.level),
        Some(ToastLevel::Error)
    );
}

#[test]
fn wrong_wifi_password_reopens_the_prompt() {
    let mut app = App::new();
    // Simulate an in-flight wifi connect whose password was wrong.
    app.on_msg(Msg::CommandDone {
        op: OpKind::WifiConnect {
            ssid: "CafeWifi".into(),
        },
        result: Err(activation_error()),
    });
    match &app.modal {
        Some(Modal::Password(p)) => {
            assert_eq!(p.ssid, "CafeWifi");
            assert!(p.error.is_some());
            assert!(p.input.is_empty());
        }
        _ => panic!("expected the password modal to reopen"),
    }
}

#[test]
fn saved_wifi_missing_password_reopens_the_prompt() {
    let mut app = App::new();
    app.on_msg(Msg::CommandDone {
        op: OpKind::ConnUp("CafeWifi".into()),
        result: Err(passwd_file_password_error()),
    });
    match &app.modal {
        Some(Modal::Password(p)) => {
            assert_eq!(p.ssid, "CafeWifi");
            assert!(p.error.is_some());
            assert!(p.input.is_empty());
        }
        _ => panic!("expected the password modal to reopen"),
    }
}
#[test]
fn fatal_error_opens_blocking_modal() {
    let mut app = App::new();
    app.on_msg(Msg::Devices(Err(AppError::Nmcli(NmcliError::Failed {
        exit: Some(NmExit::NmNotRunning),
        code: 8,
        stderr: "NetworkManager is not running".into(),
    }))));
    assert!(matches!(app.modal, Some(Modal::Error(_))));
}

#[test]
fn quit_while_in_flight_asks_for_confirmation() {
    let mut app = app_with_devices();
    app.apply(Action::DeviceConnect);
    app.apply(Action::Quit);
    assert!(!app.should_quit);
    assert!(matches!(app.modal, Some(Modal::Confirm { .. })));
    // Confirming quits.
    app.apply(Action::ConfirmYes);
    assert!(app.should_quit);
}

#[test]
fn tab_switch_triggers_a_refresh() {
    let mut app = App::new();
    let effects = app.apply(Action::GotoTab(Tab::Wifi));
    assert_eq!(app.tab, Tab::Wifi);
    assert!(effects.iter().any(|e| matches!(e, Effect::LoadWifi { .. })));
}

#[test]
fn delete_connection_flows_through_confirm() {
    let mut app = App::new();
    let conns = tui_net_seting::parse::connection::parse_connections(
        "HomeWifi:uuid-1:802-11-wireless:--:no:yes\n",
    )
    .unwrap();
    let owned = conns.into_iter().map(Into::into).collect();
    app.on_msg(Msg::Connections(Ok(owned)));
    app.apply(Action::GotoTab(Tab::Connections));

    app.apply(Action::ConnDelete);
    assert!(matches!(app.modal, Some(Modal::Confirm { .. })));

    let effects = app.apply(Action::ConfirmYes);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Run {
            op: OpKind::ConnDelete(_),
            ..
        }]
    ));
}

#[test]
fn device_refresh_keeps_selection_by_name_and_falls_back_when_missing() {
    let mut app = App::new();
    app.on_msg(Msg::Devices(Ok(vec![
        device("eth0"),
        device("eth1"),
        device("eth2"),
    ])));
    app.apply(Action::Down);

    app.on_msg(Msg::Devices(Ok(vec![
        device("eth2"),
        device("eth0"),
        device("eth1"),
    ])));
    assert_eq!(app.devices.selected, 2);
    assert_eq!(
        app.devices.selected().map(|d| d.name.as_str()),
        Some("eth1")
    );

    app.on_msg(Msg::Devices(Ok(vec![device("eth2"), device("eth0")])));
    assert_eq!(app.devices.selected, 1);
    assert_eq!(
        app.devices.selected().map(|d| d.name.as_str()),
        Some("eth0")
    );
}

#[test]
fn connection_and_wifi_refresh_use_uuid_and_bssid_identity() {
    let mut app = App::new();
    app.on_msg(Msg::Connections(Ok(vec![
        connection("第一個名稱", "uuid-a", ConnectionType::Ethernet),
        connection("舊名稱", "uuid-b", ConnectionType::Ethernet),
    ])));
    app.connections.down();
    app.on_msg(Msg::Connections(Ok(vec![
        connection("新名稱", "uuid-b", ConnectionType::Ethernet),
        connection("第一個名稱", "uuid-a", ConnectionType::Ethernet),
    ])));
    assert_eq!(app.connections.selected, 0);
    assert_eq!(
        app.connections.selected().map(|c| c.uuid.as_str()),
        Some("uuid-b")
    );

    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("Alpha", "AA", 50, false),
        wifi_ap("舊 SSID", "BB", 50, false),
    ])));
    app.wifi.down();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("新 SSID", "BB", 50, false),
        wifi_ap("Alpha", "AA", 50, false),
    ])));
    assert_eq!(app.wifi.selected, 0);
    assert_eq!(app.wifi.selected().map(|ap| ap.bssid.as_str()), Some("BB"));
}

#[test]
fn wifi_refresh_falls_back_to_ssid_when_bssid_is_missing() {
    let mut app = App::new();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("Alpha", "", 50, false),
        wifi_ap("Beta", "", 50, false),
    ])));
    app.wifi.down();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("Beta", "", 50, false),
        wifi_ap("Alpha", "", 50, false),
    ])));
    assert_eq!(app.wifi.selected, 0);
    assert_eq!(app.wifi.selected().map(|ap| ap.ssid.as_str()), Some("Beta"));
}

#[test]
fn wifi_scan_sorts_active_then_signal_and_keeps_ties_stable() {
    let mut app = App::new();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("弱", "01", 20, false),
        wifi_ap("同強度甲", "02", 80, false),
        wifi_ap("使用中", "03", 10, true),
        wifi_ap("同強度乙", "04", 80, false),
    ])));
    let ssids: Vec<&str> = app.wifi.items.iter().map(|ap| ap.ssid.as_str()).collect();
    assert_eq!(ssids, ["使用中", "同強度甲", "同強度乙", "弱"]);
}

#[test]
fn saved_wifi_marker_matches_wifi_profile_name_only() {
    let mut app = App::new();
    app.on_msg(Msg::Connections(Ok(vec![
        connection("Cafe", "wired", ConnectionType::Ethernet),
        connection("Home", "wifi", ConnectionType::Wifi),
    ])));
    assert!(!app.is_wifi_saved(&wifi_ap("Cafe", "AA", 50, false)));
    assert!(app.is_wifi_saved(&wifi_ap("Home", "BB", 50, false)));
}

#[test]
fn list_navigation_supports_first_last_and_ten_row_pages() {
    let mut app = App::new();
    app.on_msg(Msg::Devices(Ok((0..25)
        .map(|i| device(&format!("eth{i}")))
        .collect())));

    app.apply(Action::PageDown);
    assert_eq!(app.devices.selected, 10);
    app.apply(Action::PageDown);
    assert_eq!(app.devices.selected, 20);
    app.apply(Action::PageDown);
    assert_eq!(app.devices.selected, 24);
    app.apply(Action::PageUp);
    assert_eq!(app.devices.selected, 14);
    app.apply(Action::First);
    assert_eq!(app.devices.selected, 0);
    app.apply(Action::Last);
    assert_eq!(app.devices.selected, 24);
}

#[test]
fn wifi_filter_is_incremental_case_insensitive_and_clearable() {
    let mut app = App::new();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("Cafe Upstairs", "AA", 60, false),
        wifi_ap("Home", "BB", 50, false),
        wifi_ap("cafe downstairs", "CC", 40, false),
    ])));
    app.apply(Action::GotoTab(Tab::Wifi));
    app.apply(Action::WifiFilterStart);
    for c in "CAFE".chars() {
        app.apply(Action::WifiFilterInput(c));
    }

    assert!(app.wifi_filter.editing);
    let visible: Vec<&str> = app.wifi.iter().map(|ap| ap.ssid.as_str()).collect();
    assert_eq!(visible, ["Cafe Upstairs", "cafe downstairs"]);

    app.apply(Action::WifiFilterAccept);
    assert!(!app.wifi_filter.editing);
    assert_eq!(app.wifi.len(), 2);
    app.apply(Action::Back);
    assert!(app.wifi_filter.query.is_empty());
    assert_eq!(app.wifi.len(), 3);
}

#[test]
fn wifi_action_targets_selected_filtered_row() {
    let mut app = App::new();
    app.on_msg(Msg::WifiAps(Ok(vec![
        wifi_ap("Cafe One", "AA", 80, false),
        wifi_ap("Cafe Two", "BB", 70, false),
    ])));
    app.apply(Action::GotoTab(Tab::Wifi));
    app.apply(Action::WifiFilterStart);
    for c in "two".chars() {
        app.apply(Action::WifiFilterInput(c));
    }

    let effects = app.apply(Action::WifiConnect);
    assert!(matches!(
        effects.as_slice(),
        [Effect::Run {
            op: OpKind::WifiConnect { ssid },
            ..
        }] if ssid == "Cafe Two"
    ));
}

#[test]
fn detail_and_help_scroll_are_stateful_and_reset_when_closed() {
    let mut app = App::new();
    app.detail = Some(DetailPane {
        device: "eth0".into(),
        raw: "GENERAL.DEVICE:eth0\n".into(),
    });
    app.apply(Action::ScrollDown);
    app.apply(Action::ScrollDown);
    assert_eq!(app.detail_scroll, 2);
    app.apply(Action::Back);
    assert!(app.detail.is_none());
    assert_eq!(app.detail_scroll, 0);

    app.apply(Action::ShowHelp);
    app.apply(Action::ScrollDown);
    assert_eq!(app.help_scroll, 1);
    app.apply(Action::ModalCancel);
    assert!(app.modal.is_none());
    assert_eq!(app.help_scroll, 0);
}
