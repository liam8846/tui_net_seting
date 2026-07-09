//! Building and running nmcli invocations.
//!
//! Every feature of the app maps to a pure argv-builder function here (unit
//! testable on any OS) plus the single async executor [`run_nmcli`].

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::model::ConnectionDraft;
use crate::nm::error::{NmExit, NmcliError};

/// Hard backstop so the UI can never hang on a wedged nmcli child.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

/// nmcli's own wait (seconds) for activation-style commands.
const NM_WAIT: &str = "20";

/// Pinned field sets — explicit `-f` keeps column order stable across nmcli versions.
pub const DEVICE_FIELDS: &str = "DEVICE,TYPE,STATE,CONNECTION";
pub const WIFI_FIELDS: &str = "IN-USE,SSID,BSSID,SIGNAL,SECURITY,CHAN,RATE";
pub const CONNECTION_FIELDS: &str = "NAME,UUID,TYPE,DEVICE,ACTIVE,AUTOCONNECT";
pub const PROFILE_FIELDS: &str = "connection.id,connection.autoconnect,connection.type,\
     ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,\
     ipv6.method,ipv6.addresses,ipv6.gateway,ipv6.dns";

fn base(args: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = vec!["--color".into(), "no".into()];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

// ── read commands (terse / get-values) ──────────────────────────────────────

pub fn args_device_status() -> Vec<String> {
    base(&["-t", "-f", DEVICE_FIELDS, "device", "status"])
}

pub fn args_device_show(device: &str) -> Vec<String> {
    base(&["-t", "device", "show", device])
}

pub fn args_connections() -> Vec<String> {
    base(&["-t", "-f", CONNECTION_FIELDS, "connection", "show"])
}

pub fn args_connection_profile(uuid: &str) -> Vec<String> {
    base(&["-g", PROFILE_FIELDS, "connection", "show", "uuid", uuid])
}

pub fn args_wifi_list(rescan: bool) -> Vec<String> {
    let rescan = if rescan { "yes" } else { "no" };
    base(&[
        "-t",
        "-f",
        WIFI_FIELDS,
        "device",
        "wifi",
        "list",
        "--rescan",
        rescan,
    ])
}

pub fn args_networking_status() -> Vec<String> {
    base(&["networking"])
}

pub fn args_radio_wifi_status() -> Vec<String> {
    base(&["radio", "wifi"])
}

pub fn args_monitor() -> Vec<String> {
    base(&["monitor"])
}

// ── mutating commands ───────────────────────────────────────────────────────

pub fn args_device_connect(device: &str) -> Vec<String> {
    base(&["-w", NM_WAIT, "device", "connect", device])
}

pub fn args_device_disconnect(device: &str) -> Vec<String> {
    base(&["-w", NM_WAIT, "device", "disconnect", device])
}

pub fn args_connection_up(uuid: &str) -> Vec<String> {
    base(&["-w", NM_WAIT, "connection", "up", "uuid", uuid])
}

pub fn args_connection_down(uuid: &str) -> Vec<String> {
    base(&["-w", NM_WAIT, "connection", "down", "uuid", uuid])
}

pub fn args_connection_delete(uuid: &str) -> Vec<String> {
    base(&["connection", "delete", "uuid", uuid])
}

pub fn args_connection_delete_by_name(name: &str) -> Vec<String> {
    base(&["connection", "delete", "id", name])
}

pub fn args_wifi_connect(ssid: &str, password: Option<&str>) -> Vec<String> {
    let mut v = base(&["-w", NM_WAIT, "device", "wifi", "connect"]);
    v.push(ssid.to_string());
    if let Some(pw) = password {
        v.push("password".into());
        v.push(pw.to_string());
    }
    v
}

pub fn args_networking(on: bool) -> Vec<String> {
    base(&["networking", if on { "on" } else { "off" }])
}

pub fn args_radio_wifi(on: bool) -> Vec<String> {
    base(&["radio", "wifi", if on { "on" } else { "off" }])
}

pub fn args_hotspot(ifname: &str, ssid: &str, password: &str) -> Vec<String> {
    let mut v = base(&["-w", NM_WAIT, "device", "wifi", "hotspot", "ifname"]);
    v.push(ifname.to_string());
    v.push("ssid".into());
    v.push(ssid.to_string());
    v.push("password".into());
    v.push(password.to_string());
    v
}

/// `connection modify <uuid> <setting> <value> …` from an edited draft.
pub fn args_connection_modify(uuid: &str, draft: &ConnectionDraft) -> Vec<String> {
    let mut v = base(&["connection", "modify", "uuid"]);
    v.push(uuid.to_string());
    push_settings(&mut v, draft);
    v
}

/// `connection add type <t> con-name <name> ifname <ifname|*> …` for a new draft.
pub fn args_connection_add(draft: &ConnectionDraft) -> Vec<String> {
    let mut v = base(&["connection", "add", "type"]);
    v.push(draft.ctype.nmcli_add_type().to_string());
    v.push("con-name".into());
    v.push(draft.name.clone());
    v.push("ifname".into());
    v.push(if draft.ifname.is_empty() {
        "*".into()
    } else {
        draft.ifname.clone()
    });
    if draft.ctype.is_wifi() {
        v.push("wifi.ssid".into());
        v.push(if draft.ssid.is_empty() {
            draft.name.clone()
        } else {
            draft.ssid.clone()
        });
    }
    push_settings(&mut v, draft);
    v
}

fn push_settings(v: &mut Vec<String>, draft: &ConnectionDraft) {
    let pairs = [
        ("connection.id", draft.name.clone()),
        ("connection.autoconnect", yes_no(draft.autoconnect).into()),
        ("ipv4.method", draft.ipv4.method.nmcli_value().into()),
        (
            "ipv4.addresses",
            draft.ipv4.manual_or_empty(&draft.ipv4.addresses),
        ),
        (
            "ipv4.gateway",
            draft.ipv4.manual_or_empty(&draft.ipv4.gateway),
        ),
        ("ipv4.dns", draft.ipv4.manual_or_empty(&draft.ipv4.dns)),
        ("ipv6.method", draft.ipv6.method.nmcli_value().into()),
        (
            "ipv6.addresses",
            draft.ipv6.manual_or_empty(&draft.ipv6.addresses),
        ),
        (
            "ipv6.gateway",
            draft.ipv6.manual_or_empty(&draft.ipv6.gateway),
        ),
        ("ipv6.dns", draft.ipv6.manual_or_empty(&draft.ipv6.dns)),
    ];
    for (key, value) in pairs {
        v.push(key.into());
        v.push(value);
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

// ── executor ────────────────────────────────────────────────────────────────

/// Run nmcli with the given argv, capturing stdout. `LC_ALL=C` pins field
/// values and state names to English regardless of the user's locale.
pub async fn run_nmcli(args: &[String]) -> Result<String, NmcliError> {
    let fut = Command::new("nmcli")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match tokio::time::timeout(CLIENT_TIMEOUT, fut).await {
        Err(_) => return Err(NmcliError::ClientTimeout(CLIENT_TIMEOUT)),
        Ok(Err(e)) => return Err(NmcliError::Spawn(e)),
        Ok(Ok(out)) => out,
    };

    if output.status.success() {
        // from_utf8 moves the captured buffer — no copy.
        Ok(String::from_utf8(output.stdout)?)
    } else {
        match output.status.code() {
            Some(code) => Err(NmcliError::Failed {
                exit: NmExit::from_code(code),
                code,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            }),
            None => Err(NmcliError::Killed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConnectionType, IpMethod};
    use pretty_assertions::assert_eq;

    #[test]
    fn device_status_argv_is_pinned() {
        assert_eq!(
            args_device_status(),
            [
                "--color",
                "no",
                "-t",
                "-f",
                DEVICE_FIELDS,
                "device",
                "status"
            ]
        );
    }

    #[test]
    fn wifi_connect_with_and_without_password() {
        assert_eq!(
            args_wifi_connect("CafeWifi", Some("hunter22")),
            [
                "--color", "no", "-w", "20", "device", "wifi", "connect", "CafeWifi", "password",
                "hunter22"
            ]
        );
        assert_eq!(
            args_wifi_connect("Open Net", None),
            [
                "--color", "no", "-w", "20", "device", "wifi", "connect", "Open Net"
            ]
        );
    }

    #[test]
    fn hotspot_argv() {
        assert_eq!(
            args_hotspot("wlan0", "MySpot", "password1"),
            [
                "--color",
                "no",
                "-w",
                "20",
                "device",
                "wifi",
                "hotspot",
                "ifname",
                "wlan0",
                "ssid",
                "MySpot",
                "password",
                "password1"
            ]
        );
    }

    #[test]
    fn modify_manual_ipv4_clears_nothing_and_sets_addresses() {
        let mut draft = ConnectionDraft::new_ethernet();
        draft.name = "office".into();
        draft.autoconnect = true;
        draft.ipv4.method = IpMethod::Manual;
        draft.ipv4.addresses = "192.0.2.10/24".into();
        draft.ipv4.gateway = "192.0.2.1".into();
        draft.ipv4.dns = "9.9.9.9".into();

        let argv = args_connection_modify("abcd-1234", &draft);
        let joined = argv.join(" ");
        assert!(joined.starts_with("--color no connection modify uuid abcd-1234"));
        assert!(joined.contains("connection.id office"));
        assert!(joined.contains("connection.autoconnect yes"));
        assert!(joined.contains("ipv4.method manual"));
        assert!(joined.contains("ipv4.addresses 192.0.2.10/24"));
        assert!(joined.contains("ipv4.gateway 192.0.2.1"));
        assert!(joined.contains("ipv6.method auto"));
    }

    #[test]
    fn modify_auto_method_clears_manual_fields() {
        let mut draft = ConnectionDraft::new_ethernet();
        draft.name = "dhcp".into();
        draft.ipv4.method = IpMethod::Auto;
        draft.ipv4.addresses = "10.0.0.5/24".into(); // stale text must not leak
        let argv = args_connection_modify("u", &draft);
        let pos = argv.iter().position(|a| a == "ipv4.addresses").unwrap();
        assert_eq!(argv[pos + 1], "");
    }

    #[test]
    fn add_wifi_includes_ssid() {
        let mut draft = ConnectionDraft::new_ethernet();
        draft.ctype = ConnectionType::Wifi;
        draft.name = "homewifi".into();
        draft.ssid = "Home WiFi".into();
        let argv = args_connection_add(&draft);
        let joined = argv.join(" ");
        assert!(joined.contains("connection add type wifi con-name homewifi ifname *"));
        assert!(joined.contains("wifi.ssid Home WiFi"));
    }
}
