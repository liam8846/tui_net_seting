//! Parsers for `nmcli -t -f DEVICE,TYPE,STATE,CONNECTION device status`
//! and the `KEY:VALUE` lines of `nmcli -t device show <dev>`.

use std::borrow::Cow;

use crate::parse::error::ParseError;
use crate::parse::terse::{split_fields, split_kv};

#[derive(Debug)]
pub struct DeviceRow<'a> {
    pub name: Cow<'a, str>,
    pub dtype: Cow<'a, str>,
    pub state: Cow<'a, str>,
    /// `--` in nmcli output means "no active connection" → `None`.
    pub connection: Option<Cow<'a, str>>,
}

pub fn parse_devices(raw: &str) -> Result<Vec<DeviceRow<'_>>, ParseError> {
    raw.lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(i, line)| {
            let [name, dtype, state, connection] = split_fields::<4>(line, i + 1)?;
            Ok(DeviceRow {
                name,
                dtype,
                state,
                connection: none_if_dash(connection),
            })
        })
        .collect()
}

/// `--` placeholders become `None`.
pub fn none_if_dash(value: Cow<'_, str>) -> Option<Cow<'_, str>> {
    if value == "--" || value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub struct KvLine<'a> {
    pub key: &'a str,
    pub value: Cow<'a, str>,
}

/// Borrowing iterator over `KEY:VALUE` lines (`device show` output). Used at
/// draw time straight from the stored raw buffer — fully zero-copy.
pub fn parse_kv_lines(raw: &str) -> impl Iterator<Item = KvLine<'_>> {
    raw.lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| split_kv(line).map(|(key, value)| KvLine { key, value }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const SAMPLE: &str = "eth0:ethernet:connected:Wired connection 1\n\
                          wlan0:wifi:disconnected:--\n\
                          lo:loopback:unmanaged:--\n";

    #[test]
    fn parses_device_status() {
        let rows = parse_devices(SAMPLE).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "eth0");
        assert_eq!(rows[0].connection.as_deref(), Some("Wired connection 1"));
        assert!(rows[1].connection.is_none());
        assert_eq!(rows[2].state, "unmanaged");
    }

    #[test]
    fn empty_output_is_empty_vec() {
        assert!(parse_devices("").unwrap().is_empty());
        assert!(parse_devices("\n").unwrap().is_empty());
    }

    #[test]
    fn wrong_column_count_is_reported_with_line_number() {
        let err = parse_devices("eth0:ethernet:connected:x\nbroken:line\n").unwrap_err();
        assert!(matches!(err, ParseError::ColumnCount { line_no: 2, .. }));
    }

    #[test]
    fn rows_borrow_from_input() {
        let rows = parse_devices(SAMPLE).unwrap();
        assert!(matches!(rows[0].name, Cow::Borrowed(_)));
        assert!(matches!(rows[0].connection, Some(Cow::Borrowed(_))));
    }

    #[test]
    fn kv_lines_borrow_and_unescape() {
        let raw = "GENERAL.DEVICE:eth0\nGENERAL.HWADDR:AA\\:BB\\:CC\\:DD\\:EE\\:FF\n";
        let kvs: Vec<_> = parse_kv_lines(raw).collect();
        assert_eq!(kvs[0].key, "GENERAL.DEVICE");
        assert!(matches!(kvs[0].value, Cow::Borrowed(_)));
        assert_eq!(kvs[1].value, "AA:BB:CC:DD:EE:FF");
    }
}
