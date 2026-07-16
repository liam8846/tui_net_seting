//! Parsers for `nmcli -t -f NAME,UUID,TYPE,DEVICE,ACTIVE,AUTOCONNECT
//! connection show` and the `-g <fields>` profile dump used by the editor.

use std::borrow::Cow;

use crate::parse::device::none_if_dash;
use crate::parse::error::ParseError;
use crate::parse::terse::{split_fields, unescape};

#[derive(Debug)]
pub struct ConnectionRow<'a> {
    pub name: Cow<'a, str>,
    pub uuid: Cow<'a, str>,
    pub ctype: Cow<'a, str>,
    pub device: Option<Cow<'a, str>>,
    pub active: bool,
    pub autoconnect: bool,
}

pub fn parse_connections(raw: &str) -> Result<Vec<ConnectionRow<'_>>, ParseError> {
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(i, line)| {
            let [name, uuid, ctype, device, active, autoconnect] = split_fields::<6>(line, i + 1)?;
            Ok(ConnectionRow {
                name,
                uuid,
                ctype,
                device: none_if_dash(device),
                active: active == "yes",
                autoconnect: autoconnect == "yes",
            })
        })
        .collect()
}

/// Values of the `-g` profile dump, one field per output line, in the exact
/// order of [`crate::nm::command::PROFILE_FIELDS`].
pub struct ProfileRow<'a> {
    pub name: Cow<'a, str>,
    pub autoconnect: bool,
    pub ctype: Cow<'a, str>,
    pub ifname: Cow<'a, str>,
    pub ipv4_method: Cow<'a, str>,
    pub ipv4_addresses: Cow<'a, str>,
    pub ipv4_gateway: Cow<'a, str>,
    pub ipv4_dns: Cow<'a, str>,
    pub ipv6_method: Cow<'a, str>,
    pub ipv6_addresses: Cow<'a, str>,
    pub ipv6_gateway: Cow<'a, str>,
    pub ipv6_dns: Cow<'a, str>,
}

const PROFILE_LINES: usize = 12;

pub fn parse_profile(raw: &str) -> Result<ProfileRow<'_>, ParseError> {
    let mut lines = raw.lines();
    // Missing trailing lines (empty last values) are padded with "".
    let mut field = |_name: &'static str| lines.next().map(unescape).unwrap_or(Cow::Borrowed(""));
    let name = field("connection.id");
    let autoconnect = field("connection.autoconnect");
    let ctype = field("connection.type");
    let row = ProfileRow {
        name,
        autoconnect: autoconnect == "yes",
        ctype,
        ifname: field("connection.interface-name"),
        ipv4_method: field("ipv4.method"),
        ipv4_addresses: field("ipv4.addresses"),
        ipv4_gateway: field("ipv4.gateway"),
        ipv4_dns: field("ipv4.dns"),
        ipv6_method: field("ipv6.method"),
        ipv6_addresses: field("ipv6.addresses"),
        ipv6_gateway: field("ipv6.gateway"),
        ipv6_dns: field("ipv6.dns"),
    };
    let extra = lines.filter(|l| !l.is_empty()).count();
    if extra > 0 {
        return Err(ParseError::ColumnCount {
            line_no: PROFILE_LINES + 1,
            expected: PROFILE_LINES,
            got: PROFILE_LINES + extra,
        });
    }
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_connection_list() {
        let raw = "Wired connection 1:11-22:802-3-ethernet:eth0:yes:yes\n\
                   HomeWifi:33-44:802-11-wireless:--:no:yes\n";
        let rows = parse_connections(raw).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].active);
        assert_eq!(rows[1].device, None);
        assert_eq!(rows[1].ctype, "802-11-wireless");
    }

    #[test]
    fn parses_profile_dump() {
        let raw = "office\nyes\n802-3-ethernet\neth0\nmanual\n192.0.2.10/24\n192.0.2.1\n\
                   9.9.9.9,1.1.1.1\nauto\n\n\n\n";
        let p = parse_profile(raw).unwrap();
        assert_eq!(p.name, "office");
        assert!(p.autoconnect);
        assert_eq!(p.ifname, "eth0");
        assert_eq!(p.ipv4_method, "manual");
        assert_eq!(p.ipv4_dns, "9.9.9.9,1.1.1.1");
        assert_eq!(p.ipv6_method, "auto");
        assert_eq!(p.ipv6_dns, "");
    }

    #[test]
    fn profile_with_missing_trailing_lines_is_padded() {
        let p = parse_profile("n\nno\n802-3-ethernet\n\nauto\n").unwrap();
        assert_eq!(p.name, "n");
        assert!(!p.autoconnect);
        assert_eq!(p.ipv6_dns, "");
    }
}
