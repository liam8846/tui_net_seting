//! Pure state-machine tests for `App`. No terminal, no nmcli — just feed
//! Actions/Msgs and assert on the resulting state and effects.

use tui_net_seting::app::{App, ConnField, ConnTestStageState, DetailPane, Modal, ToastLevel};
use tui_net_seting::effect::Effect;
use tui_net_seting::event::{Action, AppError, ConnTestStage, Msg, OpKind, StageResult, Tab};
use tui_net_seting::model::{
    Connection, ConnectionDraft, ConnectionType, Device, DeviceState, DeviceType, IpMethod,
    RadioState, WifiAp,
};
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

fn typed_device(name: &str, dtype: DeviceType, state: DeviceState) -> Device {
    Device {
        name: name.into(),
        dtype,
        state,
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

fn app_with_manual_ipv4_address_focus() -> App {
    let mut app = App::new();
    app.apply(Action::ConnAdd);
    match &mut app.modal {
        Some(Modal::ConnForm(form)) => {
            form.draft.name = "測試連線".into();
            form.draft.ipv4.method = IpMethod::Manual;
            form.focus = 5;
        }
        _ => panic!("expected connection form"),
    }
    app
}

#[test]
fn connection_form_captures_filtered_interface_choices_with_any_first() {
    let mut app = App::new();
    app.on_msg(Msg::Devices(Ok(vec![
        typed_device("eth0", DeviceType::Ethernet, DeviceState::Disconnected),
        typed_device("wlan0", DeviceType::Wifi, DeviceState::Disconnected),
        typed_device("wlan9", DeviceType::Wifi, DeviceState::Unmanaged),
        typed_device("lo", DeviceType::Loopback, DeviceState::Connected),
    ])));

    app.apply(Action::ConnAdd);

    let Some(Modal::ConnForm(form)) = &app.modal else {
        panic!("expected connection form");
    };
    let choices: Vec<(&str, &str)> = form
        .ifnames
        .iter()
        .map(|choice| (choice.ifname.as_str(), choice.label.as_str()))
        .collect();
    assert_eq!(choices, [("", "任意 (*)"), ("eth0", "eth0（乙太網）")]);
    assert_eq!(form.ifname_idx, 0);
    assert!(form.draft.ifname.is_empty());
}

#[test]
fn connection_form_cycles_interface_and_wraps_to_any() {
    let mut app = app_with_devices();
    app.apply(Action::ConnAdd);
    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 2;
    }

    app.apply(Action::FormCycle);
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnForm(form)) if form.draft.ifname == "eth0"
    ));

    app.apply(Action::FormCycle);
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnForm(form))
            if form.draft.ifname.is_empty() && form.ifname_idx == 0
    ));
}

#[test]
fn connection_type_cycle_refilters_interfaces_and_falls_back_to_any() {
    let mut app = app_with_devices();
    app.apply(Action::ConnAdd);
    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 2;
    }
    app.apply(Action::FormCycle);

    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 1;
    }
    app.apply(Action::FormCycle);

    let Some(Modal::ConnForm(form)) = &app.modal else {
        panic!("expected connection form");
    };
    assert_eq!(form.draft.ctype, ConnectionType::Wifi);
    assert!(form.draft.ifname.is_empty());
    let names: Vec<&str> = form
        .ifnames
        .iter()
        .map(|choice| choice.ifname.as_str())
        .collect();
    assert_eq!(names, ["", "wlan0"]);
    assert_eq!(form.ifnames[1].label, "wlan0（Wi-Fi）");
}

#[test]
fn connection_type_cycle_keeps_selected_device_name_when_still_available() {
    let mut app = App::new();
    app.on_msg(Msg::Devices(Ok(vec![
        typed_device("shared0", DeviceType::Ethernet, DeviceState::Disconnected),
        typed_device("shared0", DeviceType::Wifi, DeviceState::Disconnected),
    ])));
    app.apply(Action::ConnAdd);
    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 2;
    }
    app.apply(Action::FormCycle);

    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 1;
    }
    app.apply(Action::FormCycle);

    assert!(matches!(
        &app.modal,
        Some(Modal::ConnForm(form))
            if form.draft.ctype == ConnectionType::Wifi
                && form.draft.ifname == "shared0"
                && form.ifnames[form.ifname_idx].label == "shared0（Wi-Fi）"
    ));
}

#[test]
fn editing_connection_preserves_missing_selected_interface() {
    let mut app = app_with_devices();
    let mut draft = ConnectionDraft::new_ethernet();
    draft.uuid = Some("uuid-1".into());
    draft.name = "外接網卡".into();
    draft.ifname = "enx-missing".into();

    app.on_msg(Msg::ConnectionProfile {
        uuid: "uuid-1".into(),
        result: Ok(Box::new(draft)),
    });

    let Some(Modal::ConnForm(form)) = &app.modal else {
        panic!("expected connection form");
    };
    assert_eq!(form.draft.ifname, "enx-missing");
    assert_eq!(form.ifnames[form.ifname_idx].ifname, "enx-missing");
    assert_eq!(form.ifnames[form.ifname_idx].label, "enx-missing（不存在）");
    assert!(
        form.rows()
            .iter()
            .any(|(field, _)| *field == ConnField::Ifname)
    );
}

#[test]
fn typing_does_not_edit_connection_interface_choice() {
    let mut app = app_with_devices();
    app.apply(Action::ConnAdd);
    if let Some(Modal::ConnForm(form)) = &mut app.modal {
        form.focus = 2;
    }

    app.apply(Action::FormInput('x'));
    app.apply(Action::FormBackspace);

    assert!(matches!(
        &app.modal,
        Some(Modal::ConnForm(form))
            if form.draft.ifname.is_empty() && form.ifname_idx == 0
    ));
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
fn selecting_device_row_keeps_open_detail_in_sync() {
    let mut app = app_with_devices();
    app.detail = Some(DetailPane {
        device: "eth0".into(),
        raw: String::new(),
    });
    app.detail_scroll = 4;

    let effects = app.apply(Action::SelectRow(1));

    assert_eq!(app.devices.selected, 1);
    assert_eq!(app.detail_scroll, 0);
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadDeviceDetail(device)] if device == "wlan0"
    ));
}

#[test]
fn empty_state_distinguishes_loading_from_loaded_empty_lists() {
    let mut app = App::new();
    app.initial_effects();
    assert!(app.loading.devices && app.loading.connections && app.loading.wifi);
    assert!(app.devices.is_empty() && app.connections.is_empty() && app.wifi.is_empty());

    app.on_msg(Msg::Devices(Ok(Vec::new())));
    app.on_msg(Msg::Connections(Ok(Vec::new())));
    app.on_msg(Msg::WifiAps(Ok(Vec::new())));
    app.on_msg(Msg::Radio(Ok(RadioState {
        networking: true,
        wifi: false,
    })));

    assert!(!app.loading.devices && !app.loading.connections && !app.loading.wifi);
    assert!(app.radio.is_some_and(|radio| !radio.wifi));
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

#[test]
fn invalid_ip_field_error_appears_while_typing() {
    let mut app = app_with_manual_ipv4_address_focus();
    for c in "bad".chars() {
        app.apply(Action::FormInput(c));
    }

    match &app.modal {
        Some(Modal::ConnForm(form)) => {
            assert!(form.field_error(ConnField::V4Addresses).is_some());
        }
        _ => panic!("expected connection form"),
    }
}

#[test]
fn correcting_ip_clears_its_field_error() {
    let mut app = app_with_manual_ipv4_address_focus();
    for c in "bad".chars() {
        app.apply(Action::FormInput(c));
    }
    for _ in 0..3 {
        app.apply(Action::FormBackspace);
    }
    for c in "192.0.2.10/24".chars() {
        app.apply(Action::FormInput(c));
    }

    match &app.modal {
        Some(Modal::ConnForm(form)) => {
            assert!(form.field_error(ConnField::V4Addresses).is_none());
        }
        _ => panic!("expected connection form"),
    }
}

#[test]
fn connection_submit_is_blocked_by_invalid_field() {
    let mut app = app_with_manual_ipv4_address_focus();
    for c in "bad".chars() {
        app.apply(Action::FormInput(c));
    }

    let effects = app.apply(Action::FormSubmit);

    assert!(effects.is_empty());
    match &app.modal {
        Some(Modal::ConnForm(form)) => {
            assert!(form.field_error(ConnField::V4Addresses).is_some());
            assert!(form.error.is_some());
        }
        _ => panic!("expected connection form to remain open"),
    }
}

#[test]
fn hotspot_password_error_updates_while_typing() {
    let mut app = app_with_devices();
    app.apply(Action::HotspotCreate);
    if let Some(Modal::HotspotForm(form)) = &mut app.modal {
        form.focus = 2;
    }
    for c in "short".chars() {
        app.apply(Action::FormInput(c));
    }
    assert!(matches!(
        &app.modal,
        Some(Modal::HotspotForm(form)) if form.password_error.is_some()
    ));

    for c in "123".chars() {
        app.apply(Action::FormInput(c));
    }
    assert!(matches!(
        &app.modal,
        Some(Modal::HotspotForm(form)) if form.password_error.is_none()
    ));
}

#[test]
fn password_prompt_visibility_toggles_and_new_prompt_is_masked() {
    let mut app = App::new();
    let mut ap = wifi_ap("安全網路", "AA", 80, false);
    ap.security = "WPA2".into();
    app.on_msg(Msg::WifiAps(Ok(vec![ap])));
    app.apply(Action::GotoTab(Tab::Wifi));
    app.apply(Action::WifiConnect);

    app.apply(Action::TogglePasswordVisibility);
    assert!(matches!(
        &app.modal,
        Some(Modal::Password(prompt)) if prompt.show_password
    ));

    app.apply(Action::ModalCancel);
    app.apply(Action::WifiConnect);
    assert!(matches!(
        &app.modal,
        Some(Modal::Password(prompt)) if !prompt.show_password
    ));
}

#[test]
fn hotspot_visibility_toggles_and_new_form_is_masked() {
    let mut app = app_with_devices();
    app.apply(Action::HotspotCreate);
    app.apply(Action::TogglePasswordVisibility);
    assert!(matches!(
        &app.modal,
        Some(Modal::HotspotForm(form)) if form.show_password
    ));

    app.apply(Action::ModalCancel);
    app.apply(Action::HotspotCreate);
    assert!(matches!(
        &app.modal,
        Some(Modal::HotspotForm(form)) if !form.show_password
    ));
}

#[test]
fn conn_test_prefers_selected_active_connection_device() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    app.on_msg(Msg::Devices(Ok(vec![Device {
        name: "eth0".into(),
        dtype: DeviceType::Ethernet,
        state: DeviceState::Connected,
        connection: Some("有線".into()),
    }])));
    let mut selected = connection("目前連線", "active", ConnectionType::Wifi);
    selected.active = true;
    selected.device = Some("wlan0".into());
    app.on_msg(Msg::Connections(Ok(vec![selected])));

    let effects = app.apply(Action::ConnTest);

    assert!(matches!(
        effects.as_slice(),
        [Effect::RunConnTest {
            device: Some(device),
            ..
        }] if device == "wlan0"
    ));
    assert!(matches!(
        app.modal,
        Some(Modal::ConnTest(ref state))
            if matches!(state.gateway, ConnTestStageState::Running)
    ));
}

#[test]
fn conn_test_falls_back_to_first_connected_non_loopback_device() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    app.on_msg(Msg::Devices(Ok(vec![
        Device {
            name: "lo".into(),
            dtype: DeviceType::Loopback,
            state: DeviceState::Connected,
            connection: Some("lo".into()),
        },
        Device {
            name: "eth0".into(),
            dtype: DeviceType::Ethernet,
            state: DeviceState::Connected,
            connection: Some("有線".into()),
        },
    ])));
    app.on_msg(Msg::Connections(Ok(vec![connection(
        "未啟用",
        "inactive",
        ConnectionType::Ethernet,
    )])));

    let effects = app.apply(Action::ConnTest);

    assert!(matches!(
        effects.as_slice(),
        [Effect::RunConnTest {
            device: Some(device),
            ..
        }] if device == "eth0"
    ));
}

#[test]
fn conn_test_without_connected_device_marks_gateway_failed() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    app.on_msg(Msg::Devices(Ok(vec![Device {
        name: "lo".into(),
        dtype: DeviceType::Loopback,
        state: DeviceState::Connected,
        connection: Some("lo".into()),
    }])));

    let effects = app.apply(Action::ConnTest);

    assert!(matches!(
        effects.as_slice(),
        [Effect::RunConnTest { device: None, .. }]
    ));
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(
                &state.gateway,
                ConnTestStageState::Failed(message) if message == "沒有已連線的裝置"
            ) && matches!(state.internet, ConnTestStageState::Running)
    ));
}

#[test]
fn conn_test_without_wifi_marks_wifi_skipped_and_disables_worker_check() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    app.on_msg(Msg::Devices(Ok(vec![Device {
        name: "eth0".into(),
        dtype: DeviceType::Ethernet,
        state: DeviceState::Connected,
        connection: Some("有線".into()),
    }])));

    let effects = app.apply(Action::ConnTest);

    assert!(matches!(
        effects.as_slice(),
        [Effect::RunConnTest {
            check_wifi: false,
            ..
        }]
    ));
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(
                &state.wifi,
                ConnTestStageState::Skipped(message)
                    if message == "略過：沒有 Wi-Fi 裝置"
            )
    ));
}

#[test]
fn conn_test_stage_messages_advance_in_order() {
    let mut app = app_with_devices();
    app.tab = Tab::Connections;
    app.apply(Action::ConnTest);
    let run_id = match &app.modal {
        Some(Modal::ConnTest(state)) => state.run_id,
        _ => panic!("expected connection test modal"),
    };

    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Gateway,
        result: StageResult::Passed("閘道正常".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(state.gateway, ConnTestStageState::Passed(_))
                && matches!(state.internet, ConnTestStageState::Running)
    ));

    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Internet,
        result: StageResult::Failed("網際網路失敗".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(state.internet, ConnTestStageState::Failed(_))
                && matches!(state.dns, ConnTestStageState::Running)
    ));

    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Dns,
        result: StageResult::Passed("DNS 正常".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(state.dns, ConnTestStageState::Passed(_))
                && matches!(state.wifi, ConnTestStageState::Running)
    ));

    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Wifi,
        result: StageResult::Skipped("未連線".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state)) if state.is_complete()
    ));
}

#[test]
fn conn_test_ignores_stale_messages_and_messages_after_escape() {
    let mut app = app_with_devices();
    app.tab = Tab::Connections;
    app.apply(Action::ConnTest);
    let run_id = match &app.modal {
        Some(Modal::ConnTest(state)) => state.run_id,
        _ => panic!("expected connection test modal"),
    };

    app.on_msg(Msg::ConnTestStage {
        run_id: run_id + 1,
        stage: ConnTestStage::Gateway,
        result: StageResult::Passed("不應套用".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if matches!(state.gateway, ConnTestStageState::Running)
    ));

    app.apply(Action::ModalCancel);
    assert!(app.modal.is_none());
    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Gateway,
        result: StageResult::Passed("殘留訊息".into()),
    });
    assert!(app.modal.is_none());
}

#[test]
fn completed_conn_test_can_restart_but_running_test_cannot() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    let first_effects = app.apply(Action::ConnTest);
    let first_run_id = match first_effects.as_slice() {
        [Effect::RunConnTest { run_id, .. }] => *run_id,
        _ => panic!("expected connection test effect"),
    };

    assert!(app.apply(Action::ConnTest).is_empty());
    app.on_msg(Msg::ConnTestStage {
        run_id: first_run_id,
        stage: ConnTestStage::Internet,
        result: StageResult::Passed("正常".into()),
    });
    app.on_msg(Msg::ConnTestStage {
        run_id: first_run_id,
        stage: ConnTestStage::Dns,
        result: StageResult::Failed("失敗".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state)) if state.is_complete()
    ));

    let retry_effects = app.apply(Action::ConnTest);
    assert!(matches!(
        retry_effects.as_slice(),
        [Effect::RunConnTest { run_id, .. }] if *run_id == first_run_id + 1
    ));
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state))
            if state.run_id == first_run_id + 1
                && matches!(state.gateway, ConnTestStageState::Failed(_))
                && matches!(state.internet, ConnTestStageState::Running)
                && matches!(state.dns, ConnTestStageState::Pending)
                && matches!(state.wifi, ConnTestStageState::Skipped(_))
    ));
}

#[test]
fn running_conn_test_can_be_closed_with_escape() {
    let mut app = app_with_devices();
    app.tab = Tab::Connections;
    app.apply(Action::ConnTest);
    assert!(matches!(app.modal, Some(Modal::ConnTest(_))));

    app.apply(Action::ModalCancel);

    assert!(app.modal.is_none());
}

#[test]
fn conn_test_tick_redraws_only_while_test_is_running() {
    let mut app = App::new();
    app.tab = Tab::Connections;
    let effects = app.apply(Action::ConnTest);
    let run_id = match effects.as_slice() {
        [Effect::RunConnTest { run_id, .. }] => *run_id,
        _ => panic!("expected connection test effect"),
    };

    app.dirty = false;
    app.on_tick();
    assert!(app.dirty);

    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Internet,
        result: StageResult::Passed("正常".into()),
    });
    app.on_msg(Msg::ConnTestStage {
        run_id,
        stage: ConnTestStage::Dns,
        result: StageResult::Passed("正常".into()),
    });
    assert!(matches!(
        &app.modal,
        Some(Modal::ConnTest(state)) if state.is_complete()
    ));

    app.dirty = false;
    app.on_tick();
    assert!(!app.dirty);
}
