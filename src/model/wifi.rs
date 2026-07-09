//! Owned Wi-Fi and radio types held in App state.

use crate::parse::wifi::WifiApRow;

#[derive(Debug, Clone)]
pub struct WifiAp {
    pub in_use: bool,
    pub ssid: String,
    pub bssid: String,
    pub signal: u8,
    pub security: String,
    pub chan: String,
    pub rate: String,
}

impl WifiAp {
    /// Open networks report an empty or `--` SECURITY field.
    pub fn is_secured(&self) -> bool {
        !self.security.is_empty() && self.security != "--"
    }

    pub fn is_hidden(&self) -> bool {
        self.ssid.is_empty()
    }
}

impl From<WifiApRow<'_>> for WifiAp {
    fn from(r: WifiApRow<'_>) -> Self {
        Self {
            in_use: r.in_use,
            ssid: r.ssid.into_owned(),
            bssid: r.bssid.into_owned(),
            signal: r.signal,
            security: r.security.into_owned(),
            chan: r.chan.into_owned(),
            rate: r.rate.into_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioState {
    pub networking: bool,
    pub wifi: bool,
}
