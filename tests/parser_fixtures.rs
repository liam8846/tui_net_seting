//! Integration tests that parse captured nmcli terse output. These run on any
//! OS (no nmcli needed) and double as regression guards for the zero-copy
//! contract: clean fields must stay `Cow::Borrowed`.

use std::borrow::Cow;

use tui_net_seting::model::{
    Connection, ConnectionType, Device, DeviceState, DeviceType, IpMethod, WifiAp,
};
use tui_net_seting::parse::connection::{parse_connections, parse_profile};
use tui_net_seting::parse::device::{parse_devices, parse_kv_lines};
use tui_net_seting::parse::wifi::parse_wifi_list;

const DEVICE_STATUS: &str = include_str!("fixtures/device_status.terse");
const DEVICE_SHOW: &str = include_str!("fixtures/device_show_eth0.terse");
const CONNECTIONS: &str = include_str!("fixtures/connection_show.terse");
const PROFILE: &str = include_str!("fixtures/connection_profile.terse");
const WIFI_LIST: &str = include_str!("fixtures/wifi_list.terse");

#[test]
fn device_status_fixture() {
    let rows = parse_devices(DEVICE_STATUS).unwrap();
    assert_eq!(rows.len(), 5);
    let devices: Vec<Device> = rows.into_iter().map(Device::from).collect();
    assert_eq!(devices[0].name, "eth0");
    assert_eq!(devices[0].dtype, DeviceType::Ethernet);
    assert_eq!(devices[0].state, DeviceState::Connected);
    assert_eq!(devices[0].connection.as_deref(), Some("Wired connection 1"));
    assert_eq!(devices[3].state, DeviceState::Unmanaged);
    assert!(devices[4].connection.is_none());
}

#[test]
fn device_status_rows_are_zero_copy() {
    let rows = parse_devices(DEVICE_STATUS).unwrap();
    // No field in this fixture contains an escape → everything borrows.
    assert!(rows.iter().all(|r| matches!(r.name, Cow::Borrowed(_))));
    assert!(rows.iter().all(|r| matches!(r.dtype, Cow::Borrowed(_))));
}

#[test]
fn device_show_detail_fixture() {
    let kvs: Vec<_> = parse_kv_lines(DEVICE_SHOW).collect();
    let hwaddr = kvs.iter().find(|k| k.key == "GENERAL.HWADDR").unwrap();
    assert_eq!(hwaddr.value, "AA:BB:CC:DD:EE:FF"); // unescaped
    let mtu = kvs.iter().find(|k| k.key == "GENERAL.MTU").unwrap();
    assert!(matches!(mtu.value, Cow::Borrowed(_))); // no escape → borrowed
    let addr = kvs.iter().find(|k| k.key == "IP4.ADDRESS[1]").unwrap();
    assert_eq!(addr.value, "192.168.1.42/24");
}

#[test]
fn connections_fixture_including_escaped_name() {
    let rows = parse_connections(CONNECTIONS).unwrap();
    let conns: Vec<Connection> = rows.into_iter().map(Connection::from).collect();
    assert_eq!(conns.len(), 4);
    assert!(conns[0].active);
    assert_eq!(conns[0].ctype, ConnectionType::Ethernet);
    assert!(!conns[1].autoconnect);
    assert_eq!(conns[2].name, "Home:Net"); // "Home\:Net" unescaped
    assert!(conns[2].device.is_none());
    assert_eq!(conns[3].name, "Hotspot");
    assert_eq!(conns[3].ctype, ConnectionType::Wifi);
}

#[test]
fn profile_fixture_maps_to_draft_fields() {
    let profile = parse_profile(PROFILE).unwrap();
    assert_eq!(profile.name, "Wired connection 1");
    assert!(profile.autoconnect);
    assert_eq!(IpMethod::parse(&profile.ipv4_method), IpMethod::Manual);
    assert_eq!(profile.ipv4_addresses, "192.168.1.42/24");
    assert_eq!(profile.ipv4_dns, "192.168.1.1,9.9.9.9");
    assert_eq!(IpMethod::parse(&profile.ipv6_method), IpMethod::Auto);
    assert_eq!(profile.ipv6_dns, "");
}

#[test]
fn wifi_list_fixture_with_escaped_and_hidden() {
    let rows = parse_wifi_list(WIFI_LIST).unwrap();
    let aps: Vec<WifiAp> = rows.into_iter().map(WifiAp::from).collect();
    assert_eq!(aps.len(), 4);

    assert!(aps[0].in_use);
    assert_eq!(aps[0].ssid, "CafeWifi");
    assert_eq!(aps[0].bssid, "AA:BB:CC:DD:EE:FF");
    assert!(aps[0].is_secured());

    // Escaped SSID with both `\:` and `\\`.
    assert_eq!(aps[1].ssid, "my:cafe\\net");
    assert_eq!(aps[1].signal, 64);

    // Open network: empty SECURITY.
    assert!(!aps[2].is_secured());
    assert_eq!(aps[2].ssid, "OpenGuest");

    // Hidden network: empty SSID.
    assert!(aps[3].is_hidden());
    assert!(aps[3].is_secured());
}
