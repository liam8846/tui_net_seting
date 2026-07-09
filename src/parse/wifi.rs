//! Parsers for `nmcli -t -f IN-USE,SSID,BSSID,SIGNAL,SECURITY,CHAN,RATE
//! device wifi list` and the one-word radio/networking status outputs.
//!
//! SSIDs are where terse escaping actually happens in the wild (an SSID may
//! contain `:` or `\`), so this parser leans on [`crate::parse::terse`].

use std::borrow::Cow;

use crate::parse::error::ParseError;
use crate::parse::terse::split_fields;

#[derive(Debug)]
pub struct WifiApRow<'a> {
    pub in_use: bool,
    pub ssid: Cow<'a, str>,
    pub bssid: Cow<'a, str>,
    pub signal: u8,
    pub security: Cow<'a, str>,
    pub chan: Cow<'a, str>,
    pub rate: Cow<'a, str>,
}

pub fn parse_wifi_list(raw: &str) -> Result<Vec<WifiApRow<'_>>, ParseError> {
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(i, line)| {
            let line_no = i + 1;
            let [in_use, ssid, bssid, signal, security, chan, rate] =
                split_fields::<7>(line, line_no)?;
            let signal = signal.parse::<u8>().map_err(|_| ParseError::BadValue {
                line_no,
                field: "SIGNAL",
                value: signal.to_string(),
            })?;
            Ok(WifiApRow {
                in_use: in_use == "*",
                ssid,
                bssid,
                signal,
                security,
                chan,
                rate,
            })
        })
        .collect()
}

/// `nmcli networking` / `nmcli radio wifi` print a single word.
pub fn parse_enabled(raw: &str) -> bool {
    raw.trim() == "enabled"
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_wifi_list_with_escaped_ssid() {
        let raw = "*:HomeWifi:AA\\:BB\\:CC\\:DD\\:EE\\:FF:87:WPA2:36:270 Mbit/s\n\
                   :my\\:cafe\\\\net:11\\:22\\:33\\:44\\:55\\:66:52:WPA1 WPA2:6:130 Mbit/s\n\
                   ::77\\:88\\:99\\:AA\\:BB\\:CC:30::1:54 Mbit/s\n";
        let rows = parse_wifi_list(raw).unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].in_use);
        assert_eq!(rows[0].bssid, "AA:BB:CC:DD:EE:FF");
        assert_eq!(rows[1].ssid, "my:cafe\\net");
        assert_eq!(rows[1].signal, 52);
        assert!(!rows[1].in_use);
        // hidden network: empty SSID, empty SECURITY
        assert_eq!(rows[2].ssid, "");
        assert_eq!(rows[2].security, "");
    }

    #[test]
    fn bad_signal_is_a_typed_error() {
        let err = parse_wifi_list("*:x:bssid:strong:WPA2:1:54 Mbit/s\n").unwrap_err();
        assert!(matches!(
            err,
            ParseError::BadValue {
                field: "SIGNAL",
                ..
            }
        ));
    }

    #[test]
    fn radio_status() {
        assert!(parse_enabled("enabled\n"));
        assert!(!parse_enabled("disabled\n"));
    }
}
