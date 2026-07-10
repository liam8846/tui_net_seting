//! Owned connection-profile types: the list row, the editable draft used by
//! the connection editor form, and the pure validators for its fields.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use crate::parse::connection::{ConnectionRow, ProfileRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Ethernet,
    Wifi,
    Loopback,
    Other,
}

impl ConnectionType {
    pub fn parse(s: &str) -> Self {
        match s {
            "802-3-ethernet" | "ethernet" => Self::Ethernet,
            "802-11-wireless" | "wifi" => Self::Wifi,
            "loopback" => Self::Loopback,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ethernet => "有線",
            Self::Wifi => "wifi",
            Self::Loopback => "回環",
            Self::Other => "其他",
        }
    }

    /// The `type` argument accepted by `nmcli connection add`.
    pub fn nmcli_add_type(self) -> &'static str {
        match self {
            Self::Wifi => "wifi",
            _ => "ethernet",
        }
    }

    pub fn is_wifi(self) -> bool {
        self == Self::Wifi
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub name: String,
    pub uuid: String,
    pub ctype: ConnectionType,
    pub device: Option<String>,
    pub active: bool,
    pub autoconnect: bool,
}

impl From<ConnectionRow<'_>> for Connection {
    fn from(r: ConnectionRow<'_>) -> Self {
        Self {
            ctype: ConnectionType::parse(&r.ctype),
            name: r.name.into_owned(),
            uuid: r.uuid.into_owned(),
            device: r.device.map(|d| d.into_owned()),
            active: r.active,
            autoconnect: r.autoconnect,
        }
    }
}

// ── editable draft (form model) ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpMethod {
    Auto,
    Manual,
    Disabled,
}

impl IpMethod {
    pub fn parse(s: &str) -> Self {
        match s {
            "manual" => Self::Manual,
            "disabled" | "ignore" | "link-local" => Self::Disabled,
            _ => Self::Auto,
        }
    }

    pub fn nmcli_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Disabled => "disabled",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Auto => Self::Manual,
            Self::Manual => Self::Disabled,
            Self::Disabled => Self::Auto,
        }
    }
}

/// One address family's editable settings, kept as raw text while editing.
#[derive(Debug, Clone)]
pub struct IpDraft {
    pub method: IpMethod,
    /// Comma-separated CIDR addresses, e.g. `192.0.2.10/24`.
    pub addresses: String,
    pub gateway: String,
    /// Comma-separated DNS server addresses.
    pub dns: String,
}

impl IpDraft {
    pub fn auto() -> Self {
        Self {
            method: IpMethod::Auto,
            addresses: String::new(),
            gateway: String::new(),
            dns: String::new(),
        }
    }

    /// nmcli argument value: manual fields are cleared (empty string) unless
    /// the method is manual, so stale text never leaks into the profile.
    pub fn manual_or_empty(&self, value: &str) -> String {
        if self.method == IpMethod::Manual {
            value.trim().to_string()
        } else {
            String::new()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionDraft {
    /// `None` = new profile (`connection add`), `Some` = edit (`connection modify`).
    pub uuid: Option<String>,
    pub name: String,
    pub ctype: ConnectionType,
    /// Only used by `connection add`; empty means any interface (`*`).
    pub ifname: String,
    /// Only used when adding a wifi profile.
    pub ssid: String,
    pub autoconnect: bool,
    pub ipv4: IpDraft,
    pub ipv6: IpDraft,
}

impl ConnectionDraft {
    pub fn new_ethernet() -> Self {
        Self {
            uuid: None,
            name: String::new(),
            ctype: ConnectionType::Ethernet,
            ifname: String::new(),
            ssid: String::new(),
            autoconnect: true,
            ipv4: IpDraft::auto(),
            ipv6: IpDraft::auto(),
        }
    }

    pub fn from_profile(uuid: &str, p: ProfileRow<'_>) -> Self {
        Self {
            uuid: Some(uuid.to_string()),
            ctype: ConnectionType::parse(&p.ctype),
            name: p.name.into_owned(),
            ifname: String::new(),
            ssid: String::new(),
            autoconnect: p.autoconnect,
            ipv4: IpDraft {
                method: IpMethod::parse(&p.ipv4_method),
                addresses: p.ipv4_addresses.into_owned(),
                gateway: p.ipv4_gateway.into_owned(),
                dns: p.ipv4_dns.into_owned(),
            },
            ipv6: IpDraft {
                method: IpMethod::parse(&p.ipv6_method),
                addresses: p.ipv6_addresses.into_owned(),
                gateway: p.ipv6_gateway.into_owned(),
                dns: p.ipv6_dns.into_owned(),
            },
        }
    }

    pub fn is_new(&self) -> bool {
        self.uuid.is_none()
    }

    /// Local validation before anything is sent to nmcli.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("名稱不能為空".into());
        }
        if self.is_new() && self.ctype.is_wifi() && self.ssid.trim().is_empty() {
            return Err("wifi 設定檔的 SSID 不能為空".into());
        }
        validate_family(&self.ipv4, false)?;
        validate_family(&self.ipv6, true)?;
        Ok(())
    }
}

fn validate_family(ip: &IpDraft, v6: bool) -> Result<(), String> {
    let fam = if v6 { "IPv6" } else { "IPv4" };
    if ip.method != IpMethod::Manual {
        return Ok(());
    }
    if ip.addresses.trim().is_empty() {
        return Err(format!("{fam}: 手動方式需要至少一個位址"));
    }
    for addr in split_list(&ip.addresses) {
        validate_cidr(addr, v6).map_err(|e| format!("{fam} 位址 {addr:?}: {e}"))?;
    }
    if !ip.gateway.trim().is_empty() {
        validate_ip(ip.gateway.trim(), v6).map_err(|e| format!("{fam} 閘道: {e}"))?;
    }
    for dns in split_list(&ip.dns) {
        validate_ip(dns, v6).map_err(|e| format!("{fam} DNS {dns:?}: {e}"))?;
    }
    Ok(())
}

fn split_list(s: &str) -> impl Iterator<Item = &str> {
    s.split(',').map(str::trim).filter(|p| !p.is_empty())
}

/// Validate the address-list field used by a manual IP configuration.
pub fn validate_address_field(value: &str, v6: bool) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("手動方式需要至少一個位址".into());
    }
    for address in split_list(value) {
        validate_cidr(address, v6).map_err(|_| {
            if v6 {
                "位址需為 CIDR，如 2001:db8::10/64".to_string()
            } else {
                "位址需為 CIDR，如 192.0.2.10/24".to_string()
            }
        })?;
    }
    Ok(())
}

/// Validate an optional gateway field. Empty means no explicit gateway.
pub fn validate_gateway_field(value: &str, v6: bool) -> Result<(), String> {
    if value.trim().is_empty() {
        return Ok(());
    }
    validate_ip(value.trim(), v6)
        .map_err(|_| format!("閘道需為有效的 {} 位址", if v6 { "IPv6" } else { "IPv4" }))
}

/// Validate an optional comma-separated DNS server list.
pub fn validate_dns_field(value: &str, v6: bool) -> Result<(), String> {
    for dns in split_list(value) {
        validate_ip(dns, v6)
            .map_err(|_| format!("DNS 需為有效的 {} 位址", if v6 { "IPv6" } else { "IPv4" }))?;
    }
    Ok(())
}

pub fn validate_ip(s: &str, v6: bool) -> Result<(), String> {
    match IpAddr::from_str(s) {
        Ok(IpAddr::V4(_)) if !v6 => Ok(()),
        Ok(IpAddr::V6(_)) if v6 => Ok(()),
        Ok(_) => Err("位址家族不符".into()),
        Err(_) => Err("不是有效的 IP 位址".into()),
    }
}

/// `a.b.c.d/prefix` (or v6 equivalent); prefix is required by nmcli for
/// unambiguous static addressing.
pub fn validate_cidr(s: &str, v6: bool) -> Result<(), String> {
    let (ip, prefix) = s
        .split_once('/')
        .ok_or_else(|| "預期為 位址/前綴（例如 192.0.2.10/24）".to_string())?;
    let max = if v6 {
        Ipv6Addr::from_str(ip).map_err(|_| "不是有效的 IPv6 位址".to_string())?;
        128
    } else {
        Ipv4Addr::from_str(ip).map_err(|_| "不是有效的 IPv4 位址".to_string())?;
        32
    };
    match prefix.parse::<u8>() {
        Ok(p) if p <= max => Ok(()),
        _ => Err(format!("前綴必須為 0-{max}")),
    }
}

/// WPA2-PSK passphrase constraint, shared by hotspot form validation.
pub fn validate_wifi_password(pw: &str) -> Result<(), String> {
    let len = pw.chars().count();
    if (8..=63).contains(&len) {
        Ok(())
    } else {
        Err("密碼必須為 8-63 個字元".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_validation() {
        assert!(validate_cidr("192.0.2.10/24", false).is_ok());
        assert!(validate_cidr("192.0.2.10", false).is_err());
        assert!(validate_cidr("192.0.2.10/33", false).is_err());
        assert!(validate_cidr("fe80::1/64", true).is_ok());
        assert!(validate_cidr("fe80::1/129", true).is_err());
        assert!(validate_cidr("192.0.2.10/24", true).is_err());
    }

    #[test]
    fn draft_validation_catches_manual_without_address() {
        let mut d = ConnectionDraft::new_ethernet();
        d.name = "x".into();
        d.ipv4.method = IpMethod::Manual;
        assert!(d.validate().is_err());
        d.ipv4.addresses = "192.0.2.10/24".into();
        assert!(d.validate().is_ok());
        d.ipv4.dns = "9.9.9.9, nonsense".into();
        assert!(d.validate().is_err());
    }

    #[test]
    fn wifi_password_bounds() {
        assert!(validate_wifi_password("hunter22").is_ok());
        assert!(validate_wifi_password("short").is_err());
        assert!(validate_wifi_password(&"x".repeat(64)).is_err());
    }

    #[test]
    fn field_validation_allows_empty_optional_values() {
        assert!(validate_gateway_field("", false).is_ok());
        assert!(validate_dns_field("", true).is_ok());
        assert!(validate_address_field("", false).is_err());
    }
}
