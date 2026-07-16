//! Parser for `nmcli -g IP4.GATEWAY,IP6.GATEWAY device show` output.

#[derive(Debug, PartialEq, Eq)]
pub struct GatewayRow<'a> {
    pub ipv4: Option<&'a str>,
    pub ipv6: Option<&'a str>,
}

pub fn parse_gateway(raw: &str) -> GatewayRow<'_> {
    let mut lines = raw.split_terminator('\n');
    let ipv4 = lines.next().and_then(present);
    let ipv6 = lines.next().and_then(present);
    GatewayRow { ipv4, ipv6 }
}

fn present(value: &str) -> Option<&str> {
    let value = value.trim_end_matches('\r').trim();
    (!value.is_empty() && value != "--").then_some(value)
}
