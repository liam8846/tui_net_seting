//! Pure state-machine tests for `App`. No terminal, no nmcli — just feed
//! Actions/Msgs and assert on the resulting state and effects.

use tui_net_seting::app::{App, Modal, ToastLevel};
use tui_net_seting::effect::Effect;
use tui_net_seting::event::{Action, AppError, Msg, OpKind, Tab};
use tui_net_seting::model::Device;
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
