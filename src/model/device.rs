//! Owned device types held in App state.

use crate::parse::device::DeviceRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    Ethernet,
    Wifi,
    Loopback,
    Bridge,
    Tun,
    WifiP2p,
    Other(String),
}

impl DeviceType {
    pub fn parse(s: &str) -> Self {
        match s {
            "ethernet" => Self::Ethernet,
            "wifi" => Self::Wifi,
            "loopback" => Self::Loopback,
            "bridge" => Self::Bridge,
            "tun" => Self::Tun,
            "wifi-p2p" => Self::WifiP2p,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Ethernet => "ethernet",
            Self::Wifi => "wifi",
            Self::Loopback => "loopback",
            Self::Bridge => "bridge",
            Self::Tun => "tun",
            Self::WifiP2p => "wifi-p2p",
            Self::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceState {
    Connected,
    Connecting,
    Disconnected,
    Unavailable,
    Unmanaged,
    Deactivating,
    Other(String),
}

impl DeviceState {
    /// Terse states may carry a suffix like `connecting (configuring)` —
    /// match on the prefix, zero-alloc on the common paths.
    pub fn parse(s: &str) -> Self {
        if s == "connected" || s.starts_with("connected ") {
            Self::Connected
        } else if s.starts_with("connecting") {
            Self::Connecting
        } else if s == "disconnected" {
            Self::Disconnected
        } else if s == "unavailable" {
            Self::Unavailable
        } else if s == "unmanaged" {
            Self::Unmanaged
        } else if s.starts_with("deactivating") {
            Self::Deactivating
        } else {
            Self::Other(s.to_string())
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Unavailable => "unavailable",
            Self::Unmanaged => "unmanaged",
            Self::Deactivating => "deactivating",
            Self::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub dtype: DeviceType,
    pub state: DeviceState,
    pub connection: Option<String>,
}

impl From<DeviceRow<'_>> for Device {
    fn from(r: DeviceRow<'_>) -> Self {
        Self {
            dtype: DeviceType::parse(&r.dtype),
            state: DeviceState::parse(&r.state),
            name: r.name.into_owned(),
            connection: r.connection.map(|c| c.into_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_prefix_matching() {
        assert_eq!(DeviceState::parse("connected"), DeviceState::Connected);
        assert_eq!(
            DeviceState::parse("connecting (configuring)"),
            DeviceState::Connecting
        );
        assert_eq!(
            DeviceState::parse("weird"),
            DeviceState::Other("weird".into())
        );
    }
}
